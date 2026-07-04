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
//!     - `read-note` / `search-notes` / `list-folders`  notes:read 链路（spec 0.3）
//!     - `write-note` / `make-note`   notes:write 链路（提案回传 vs 免确认直写）
//!     - `ai-echo`    host:ai 链路（宿主代理补全）
//!     - `chat`       chat widget 契约（args {messages,input,note_id} → result.reply）
//! - `ui`          server-driven UI（spec §9.3）：view "main" 返回静态小树；
//!                 view "notes" 调 notes.search 组 list（证明 ui 上下文有 notes 能力）
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
                // 免能力读「系统语言」（spec 0.4 §6.5 system.locale）：验证插件运行时能拿到 UI 语言
                "read-locale" => Ok(json!({ "locale": sdk::host::system_locale()? })),
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
                "read-note" => {
                    let id = args.get("id").and_then(Value::as_str).unwrap_or("");
                    let note = sdk::host::notes_get(id)?;
                    Ok(json!({ "note": note }))
                }
                "search-notes" => {
                    let q = args.get("q").and_then(Value::as_str).unwrap_or("");
                    let limit = args.get("limit").and_then(Value::as_u64).map(|v| v as u32);
                    let notes = sdk::host::notes_search(q, limit)?;
                    Ok(json!({ "notes": notes }))
                }
                "list-folders" => {
                    let folders = sdk::host::notes_list_folders()?;
                    Ok(json!({ "folders": folders }))
                }
                "write-note" => {
                    let id = args.get("id").and_then(Value::as_str).unwrap_or("");
                    let body = args.get("body").and_then(Value::as_str);
                    let title = args.get("title").and_then(Value::as_str);
                    let r = sdk::host::notes_upsert(id, title, body)?;
                    Ok(json!({ "pending": r.pending, "note": r.note }))
                }
                "make-note" => {
                    let parent_id = args.get("parent_id").and_then(Value::as_str).unwrap_or("");
                    let title = args.get("title").and_then(Value::as_str);
                    let body = args.get("body").and_then(Value::as_str);
                    let r = sdk::host::notes_create(parent_id, title, body)?;
                    Ok(json!({ "pending": r.pending, "note": r.note }))
                }
                "ai-echo" => {
                    let prompt = args.get("prompt").and_then(Value::as_str).unwrap_or("");
                    let content =
                        sdk::host::ai_complete(&[sdk::host::Message::user(prompt)], None)?;
                    Ok(json!({ "content": content }))
                }
                // chat widget 契约（spec §9.2）：回显输入作 reply
                "chat" => {
                    let input = args.get("input").and_then(Value::as_str).unwrap_or("");
                    Ok(json!({ "reply": format!("echo: {input}") }))
                }
                other => Err(PluginError::unsupported(format!("未知命令: {other}"))),
            }
        }
        "ui" => {
            let view = params.get("view").and_then(Value::as_str).unwrap_or("");
            match view {
                "main" => Ok(json!({
                    "type": "markdown",
                    "props": { "source": "**testbed**" },
                    "children": [
                        { "type": "button", "props": { "label": "Echo", "command": "echo-args", "args": { "from": "ui" } } },
                        { "type": "list", "props": { "items": [ { "id": "1", "title": "one" } ], "command": "echo-args" } },
                    ],
                })),
                // ui 分发上下文同样有 notes 能力（spec §6.5）
                "notes" => {
                    let q = params
                        .get("state")
                        .and_then(|s| s.get("q"))
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let items: Vec<Value> = sdk::host::notes_search(q, None)?
                        .into_iter()
                        .map(|n| json!({ "id": n.id, "title": n.title }))
                        .collect();
                    Ok(json!({ "type": "list", "props": { "items": items } }))
                }
                other => Err(PluginError::not_found(format!("未知 view: {other}"))),
            }
        }
        // 编辑期文本变换（spec §3.7/§6.5 editor.transform）：可观测的确定性变换——
        // 标出相位 + 全大写，供宿主/前端链路断言。手写 ABI，不经 register! 的 editor 槽。
        "editor.transform" => {
            let phase = params.get("phase").and_then(Value::as_str).unwrap_or("");
            let text = params.get("text").and_then(Value::as_str).unwrap_or("");
            Ok(json!({ "text": format!("[{phase}] {}", text.to_uppercase()) }))
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
