//! 测试夹具插件（不随发行分发）：
//! - `echo`        原样返回 params（ABI 往返验证）
//! - `spin`        死循环（fuel/CPU 墙钟中止验证）
//! - `alloc_bomb`  无限分配（StoreLimits 内存上限验证，OOM → trap 不崩宿主）
//! - `bad_json`    返回非法 JSON（宿主容错验证）
//! - `call_http`   调 host `http.request`（manifest 未申请 host:http → 应得 forbidden）
//! - `command`     命令槽（宿主 commands 端点链路验证）：
//!     - `echo-args`  原样返回 args（参数透传验证）
//!     - `relay`      读 settings（target_url + secret token）→ host:http GET → 响应文本作
//!                    `result.body`；缺 target_url → invalid（错误码映射验证）
//!
//! 故意不用 `register!` 宏：手写 ABI，同时验证「不用 SDK 宏的插件」也符合规范。
//! 测试按需在临时目录写自己的 manifest（能力集各测各的），只复用这里的 plugin.wasm。

use jasper_plugin_sdk as sdk;
use sdk::rt::{self, PluginError};
use sdk::serde_json::{json, Value};

fn dispatch_impl(method: &str, params: Value) -> Result<Value, PluginError> {
    match method {
        "metadata" => Ok(json!({ "ok": true })),
        "echo" => Ok(json!({ "echo": params })),
        "spin" => loop {
            std::hint::black_box(0u64);
        },
        "alloc_bomb" => {
            let mut hold: Vec<Vec<u8>> = Vec::new();
            loop {
                hold.push(vec![1u8; 1 << 20]); // 每次 1 MiB，直到内存上限 trap
                std::hint::black_box(&hold);
            }
        }
        "call_http" => rt::call_host(
            "http.request",
            json!({ "method": "GET", "url": "http://127.0.0.1:1/" }),
        ),
        "command" => {
            let id = params.get("id").and_then(Value::as_str).unwrap_or("");
            let args = params.get("args").cloned().unwrap_or(Value::Null);
            match id {
                "echo-args" => Ok(json!({ "echoed": args })),
                "relay" => {
                    let url = sdk::host::settings_get("target_url")?;
                    let Some(url) = url.as_str().filter(|s| !s.is_empty()) else {
                        return Err(PluginError::invalid("缺 target_url 设置"));
                    };
                    let mut req = sdk::host::HttpRequest {
                        method: "GET".into(),
                        url: url.to_string(),
                        ..Default::default()
                    };
                    if let Some(token) = sdk::host::settings_get("token")?.as_str() {
                        req.headers.insert("authorization".into(), format!("Bearer {token}"));
                    }
                    let resp = sdk::host::http_request(&req)?;
                    Ok(json!({ "body": resp.body_text() }))
                }
                other => Err(PluginError::unsupported(format!("未知命令: {other}"))),
            }
        }
        other => Err(PluginError::unsupported(format!("未知方法: {other}"))),
    }
}

#[cfg(target_arch = "wasm32")]
mod abi {
    use super::*;

    #[no_mangle]
    pub extern "C" fn plugin_alloc(size: u32) -> u32 {
        rt::alloc(size as usize) as u32
    }

    #[no_mangle]
    pub extern "C" fn plugin_free(ptr: u32, size: u32) {
        rt::free(ptr as usize, size as usize)
    }

    #[no_mangle]
    pub extern "C" fn plugin_dispatch(ptr: u32, len: u32) -> u64 {
        // bad_json 绕过信封：直接返回非法 JSON 字节
        let req = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) };
        if let Ok(r) = sdk::serde_json::from_slice::<rt::Req>(req) {
            if r.method == "bad_json" {
                return rt::write_out(b"this is not json {");
            }
        }
        rt::dispatch(ptr as usize, len as usize, dispatch_impl)
    }
}
