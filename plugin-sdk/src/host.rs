//! 宿主能力的类型化封装（spec §6.5「插件 → host」方法）。
//! 每个函数对应一个 host_call 方法；所需能力见 spec §7，未授权时返回 code="forbidden"。

use crate::rt::{call_host, PluginError};
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// `log`：宿主日志（无需能力）。失败静默忽略——日志不该让业务失败。
pub fn log(level: &str, message: &str) {
    let _ = call_host("log", json!({ "level": level, "message": message }));
}

/// `settings.get`：读插件作用域 KV（能力 `settings`）。键不存在返回 Null。
pub fn settings_get(key: &str) -> Result<Value, PluginError> {
    let result = call_host("settings.get", json!({ "key": key }))?;
    Ok(result.get("value").cloned().unwrap_or(Value::Null))
}

/// `settings.set`：写插件作用域 KV（能力 `settings`）。
pub fn settings_set(key: &str, value: Value) -> Result<(), PluginError> {
    call_host("settings.set", json!({ "key": key, "value": value }))?;
    Ok(())
}

/// HTTP 请求（能力 `host:http`，spec 0.2）。二进制体在这里做 base64 编解码。
#[derive(Debug, Clone, Default)]
pub struct HttpRequest {
    /// GET/PUT/PROPFIND/…（原样透传给宿主）
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    /// 非 2xx 也照常返回（spec §6.5：错误语义留给插件判断）
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// `http.request`：宿主代理执行 HTTP(S)。网络失败（连接/超时）→ Err(internal)；
/// 拿到响应（含 4xx/5xx）→ Ok(HttpResponse)。
pub fn http_request(req: &HttpRequest) -> Result<HttpResponse, PluginError> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut params = json!({
        "method": req.method,
        "url": req.url,
        "headers": req.headers,
    });
    if let Some(body) = &req.body {
        params["body_b64"] = Value::String(b64.encode(body));
    }
    if let Some(t) = req.timeout_ms {
        params["timeout_ms"] = json!(t);
    }
    let result = call_host("http.request", params)?;
    let status = result
        .get("status")
        .and_then(Value::as_u64)
        .ok_or_else(|| PluginError::internal("http.request 响应缺 status"))? as u16;
    let headers = result
        .get("headers")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let body = match result.get("body_b64").and_then(Value::as_str) {
        Some(s) => b64
            .decode(s)
            .map_err(|e| PluginError::internal(format!("body_b64 解码失败: {e}")))?,
        None => Vec::new(),
    };
    Ok(HttpResponse { status, headers, body })
}
