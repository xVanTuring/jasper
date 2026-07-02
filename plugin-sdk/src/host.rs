//! 宿主能力的类型化封装（spec §6.5「插件 → host」方法）。
//! 每个函数对应一个 host_call 方法；所需能力见 spec §7，未授权时返回 code="forbidden"。

use crate::rt::{call_host, PluginError};
use base64::Engine as _;
use jasper_core::model::Note;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// `log`：宿主日志（无需能力）。失败静默忽略——日志不该让业务失败。
pub fn log(level: &str, message: &str) {
    let _ = call_host("log", json!({ "level": level, "message": message }));
}

/// `time.now`：当前时间 Unix 毫秒（无需能力，spec 0.2）。
/// 沙箱内没有时钟（`SystemTime::now()` 在 wasm32 会 panic）——签名协议（S3 SigV4 等）用这个。
pub fn now_ms() -> Result<i64, PluginError> {
    let r = call_host("time.now", json!({}))?;
    r.get("unix_ms")
        .and_then(Value::as_i64)
        .ok_or_else(|| PluginError::internal("time.now 响应缺 unix_ms"))
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

// ---- notes.*（能力 notes:read / notes:write，spec 0.3）----
// 仅在 command / ui 分发上下文可用；hooks/storage 分发内调用返回 code="unsupported"（spec §6.5）。

/// 搜索结果条目（spec §6.5 NoteRef）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteRef {
    pub id: String,
    pub title: String,
    pub parent_id: String,
}

/// 笔记本条目（spec §6.5 FolderRef）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderRef {
    pub id: String,
    pub title: String,
    pub parent_id: String,
}

/// `notes.get`：按 id 取完整笔记（能力 `notes:read`）。不存在 → code="not_found"。
pub fn notes_get(id: &str) -> Result<Note, PluginError> {
    let r = call_host("notes.get", json!({ "id": id }))?;
    parse_note(&r)
}

/// `notes.search`：标题/正文全文搜索（能力 `notes:read`）。`limit` 缺省宿主取 20、上限 100。
pub fn notes_search(query: &str, limit: Option<u32>) -> Result<Vec<NoteRef>, PluginError> {
    let mut params = json!({ "query": query });
    if let Some(l) = limit {
        params["limit"] = json!(l);
    }
    let r = call_host("notes.search", params)?;
    serde_json::from_value(r.get("notes").cloned().unwrap_or(Value::Null))
        .map_err(|e| PluginError::internal(format!("notes.search 响应解析失败: {e}")))
}

/// `notes.list_folders`：全部笔记本（能力 `notes:read`），宿主按 (title, id) 排序。
pub fn notes_list_folders() -> Result<Vec<FolderRef>, PluginError> {
    let r = call_host("notes.list_folders", json!({}))?;
    serde_json::from_value(r.get("folders").cloned().unwrap_or(Value::Null))
        .map_err(|e| PluginError::internal(format!("notes.list_folders 响应解析失败: {e}")))
}

/// `notes.upsert`/`notes.create` 的结果。`pending=true` = 提案已交宿主 UI 确认、
/// **尚未落盘**（spec §6.5 写确认=提案回传）；插件不应把它当成写入成功。
#[derive(Debug, Clone)]
pub struct UpsertResult {
    pub note: Note,
    pub pending: bool,
}

/// `notes.upsert`：改标题/正文（能力 `notes:write`）。`None` 字段保持原值；
/// 其余元数据宿主逐字保留。默认走提案回传（见 [`UpsertResult`]）。
pub fn notes_upsert(id: &str, title: Option<&str>, body: Option<&str>) -> Result<UpsertResult, PluginError> {
    let mut params = json!({ "id": id });
    if let Some(t) = title {
        params["title"] = json!(t);
    }
    if let Some(b) = body {
        params["body"] = json!(b);
    }
    let r = call_host("notes.upsert", params)?;
    parse_upsert(&r)
}

/// `notes.create`：新建笔记（能力 `notes:write`）。`parent_id` 必须是已存在笔记本（否则 invalid）。
/// `pending=true` 时 `note.id` 为空串——id 由宿主在用户批准时生成，无法链式写入。
pub fn notes_create(parent_id: &str, title: Option<&str>, body: Option<&str>) -> Result<UpsertResult, PluginError> {
    let mut params = json!({ "parent_id": parent_id });
    if let Some(t) = title {
        params["title"] = json!(t);
    }
    if let Some(b) = body {
        params["body"] = json!(b);
    }
    let r = call_host("notes.create", params)?;
    parse_upsert(&r)
}

fn parse_note(r: &Value) -> Result<Note, PluginError> {
    serde_json::from_value(r.get("note").cloned().unwrap_or(Value::Null))
        .map_err(|e| PluginError::internal(format!("note 解析失败: {e}")))
}

fn parse_upsert(r: &Value) -> Result<UpsertResult, PluginError> {
    Ok(UpsertResult {
        note: parse_note(r)?,
        pending: r.get("pending").and_then(Value::as_bool).unwrap_or(false),
    })
}

// ---- ai.complete（能力 host:ai，spec 0.3）----

/// 对话消息（spec §6.5 Message）；`role` ∈ system|user|assistant。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: "assistant".into(), content: content.into() }
    }
}

/// `ai.complete` 的可选项；`model` 缺省用宿主配置。宿主钳制 temperature 0..=2、max_tokens 1..=32768。
#[derive(Debug, Clone, Default)]
pub struct AiOptions {
    pub model: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
}

/// `ai.complete`：宿主代理的一次性补全（能力 `host:ai`）。密钥/端点在宿主，插件不可见；
/// 宿主未配置 AI → code="internal"（message 指明去设置页配置）。网络等待不计插件 CPU 墙钟。
pub fn ai_complete(messages: &[Message], options: Option<&AiOptions>) -> Result<String, PluginError> {
    let mut params = json!({ "messages": messages });
    if let Some(o) = options {
        let mut opts = json!({});
        if let Some(m) = &o.model {
            opts["model"] = json!(m);
        }
        if let Some(t) = o.temperature {
            opts["temperature"] = json!(t);
        }
        if let Some(mt) = o.max_tokens {
            opts["max_tokens"] = json!(mt);
        }
        params["options"] = opts;
    }
    let r = call_host("ai.complete", params)?;
    r.get("content")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| PluginError::internal("ai.complete 响应缺 content"))
}
