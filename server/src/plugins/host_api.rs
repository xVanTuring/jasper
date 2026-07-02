//! host_call 方法表（spec §6.5「插件 → host」+ §7 能力门控）。
//! 一切失败以 JSON 错误信封返回给插件；本模块绝不 panic。

use super::runtime::HostCtx;
use base64::Engine as _;
use serde_json::{json, Value};
use std::io::Read;
use std::time::{Duration, Instant};

pub fn ok_envelope(result: Value) -> Value {
    json!({ "ok": true, "result": result })
}

pub fn err_envelope(code: &str, message: impl Into<String>) -> Value {
    json!({ "ok": false, "error": { "message": message.into(), "code": code } })
}

/// 方法分发。返回响应信封。
pub fn handle(ctx: &mut HostCtx, method: &str, params: Value) -> Value {
    let r = match method {
        "log" => log(ctx, &params),
        // 沙箱无时钟（SystemTime 在 wasm32 panic）；签名类协议（S3 SigV4 等）需要当前时间。
        // 与 log 一样免能力：粒度毫秒，泄露面可忽略。
        "time.now" => Ok(json!({ "unix_ms": crate::serialize::now_ms() })),
        "settings.get" => need(ctx, "settings").and_then(|_| settings_get(ctx, &params)),
        "settings.set" => need(ctx, "settings").and_then(|_| settings_set(ctx, &params)),
        "http.request" => need(ctx, "host:http").and_then(|_| http_request(ctx, &params)),
        other => Err(("unsupported".into(), format!("宿主不支持方法: {other}"))),
    };
    match r {
        Ok(v) => ok_envelope(v),
        Err((code, msg)) => err_envelope(&code, msg),
    }
}

type HostResult = Result<Value, (String, String)>;

fn need(ctx: &HostCtx, cap: &str) -> Result<(), (String, String)> {
    if ctx.has_cap(cap) {
        Ok(())
    } else {
        Err(("forbidden".into(), format!("未授权能力: {cap}")))
    }
}

fn str_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, (String, String)> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ("invalid".into(), format!("缺参数 {key}")))
}

fn log(ctx: &mut HostCtx, params: &Value) -> HostResult {
    let level = params.get("level").and_then(Value::as_str).unwrap_or("info");
    let message = params.get("message").and_then(Value::as_str).unwrap_or("");
    println!("[plugin:{}] [{level}] {message}", ctx.plugin_id);
    Ok(json!({}))
}

fn settings_get(ctx: &mut HostCtx, params: &Value) -> HostResult {
    let key = str_param(params, "key")?;
    let store = ctx.config.lock().unwrap();
    let value = store
        .plugin_settings(&ctx.plugin_id)
        .into_iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| serde_json::from_str::<Value>(&v).ok())
        .unwrap_or(Value::Null);
    Ok(json!({ "value": value }))
}

fn settings_set(ctx: &mut HostCtx, params: &Value) -> HostResult {
    let key = str_param(params, "key")?;
    let value = params.get("value").cloned().unwrap_or(Value::Null);
    let text = serde_json::to_string(&value).map_err(|e| ("internal".to_string(), e.to_string()))?;
    ctx.config
        .lock()
        .unwrap()
        .set_plugin_setting(&ctx.plugin_id, key, Some(&text))
        .map_err(|e| ("internal".to_string(), e.to_string()))?;
    Ok(json!({}))
}

/// `http.request`（spec §6.5，0.2）：宿主代理执行。
/// 非 2xx 状态照常以 ok 返回（错误语义留给插件）；网络失败 → internal。
/// IO 耗时计入 ctx.io_time（CPU 墙钟豁免，spec §11）。
fn http_request(ctx: &mut HostCtx, params: &Value) -> HostResult {
    let method = str_param(params, "method")?.to_uppercase();
    let url = str_param(params, "url")?;
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(("invalid".into(), "仅支持 http/https".into()));
    }
    let timeout_ms = params
        .get("timeout_ms")
        .and_then(Value::as_u64)
        .unwrap_or(30_000)
        .min(120_000);

    let mut req = ctx.http.request(&method, url).timeout(Duration::from_millis(timeout_ms));
    if let Some(headers) = params.get("headers").and_then(Value::as_object) {
        for (k, v) in headers {
            if let Some(v) = v.as_str() {
                req = req.set(k, v);
            }
        }
    }
    let body = match params.get("body_b64").and_then(Value::as_str) {
        Some(b64) => Some(
            base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| ("invalid".to_string(), format!("body_b64 解码失败: {e}")))?,
        ),
        None => None,
    };

    let started = Instant::now();
    let result = match body {
        Some(bytes) => req.send_bytes(&bytes),
        None => req.call(),
    };
    ctx.io_time += started.elapsed();

    let resp = match result {
        Ok(resp) => resp,
        // 非 2xx：拿到响应照常返回（WebDAV 需要 404/207 等语义）
        Err(ureq::Error::Status(_, resp)) => resp,
        Err(e) => return Err(("internal".into(), format!("网络请求失败: {e}"))),
    };

    let status = resp.status();
    let mut headers = serde_json::Map::new();
    for name in resp.headers_names() {
        if let Some(v) = resp.header(&name) {
            headers.insert(name, Value::String(v.to_string()));
        }
    }
    let cap = ctx.http_response_cap;
    let mut buf = Vec::new();
    let read_started = Instant::now();
    let read = resp.into_reader().take(cap as u64 + 1).read_to_end(&mut buf);
    ctx.io_time += read_started.elapsed();
    read.map_err(|e| ("internal".to_string(), format!("读响应失败: {e}")))?;
    if buf.len() > cap {
        return Err(("internal".into(), format!("响应体超过上限 {} MiB", cap / 1024 / 1024)));
    }

    Ok(json!({
        "status": status,
        "headers": headers,
        "body_b64": base64::engine::general_purpose::STANDARD.encode(&buf),
    }))
}
