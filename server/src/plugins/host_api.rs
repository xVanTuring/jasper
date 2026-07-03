//! host_call 方法表（spec §6.5「插件 → host」+ §7 能力门控）。
//! 一切失败以 JSON 错误信封返回给插件；本模块绝不 panic。

use super::runtime::{HostCtx, NotesCtx};
use crate::model::{MarkupLanguage, Note};
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
        // 当前 UI 语言（免能力，spec 0.4 §6.5）：插件据此本地化自己运行时产出的文字
        // （chat 回复 / 动态 UI 文案等）。宿主持久化在 config.db，前端切换时同步。
        "system.locale" => Ok(json!({ "locale": ctx.config.lock().unwrap().ui_locale() })),
        "settings.get" => need(ctx, "settings").and_then(|_| settings_get(ctx, &params)),
        "settings.set" => need(ctx, "settings").and_then(|_| settings_set(ctx, &params)),
        "http.request" => need(ctx, "host:http").and_then(|_| http_request(ctx, &params)),
        "notes.get" => need(ctx, "notes:read").and_then(|_| notes_get(ctx, &params)),
        "notes.search" => need(ctx, "notes:read").and_then(|_| notes_search(ctx, &params)),
        "notes.list_folders" => need(ctx, "notes:read").and_then(|_| notes_list_folders(ctx)),
        "notes.upsert" => need(ctx, "notes:write").and_then(|_| notes_upsert(ctx, &params)),
        "notes.create" => need(ctx, "notes:write").and_then(|_| notes_create(ctx, &params)),
        "ai.complete" => need(ctx, "host:ai").and_then(|_| ai_complete(ctx, &params)),
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

/// notes/ai 上下文：仅 command/ui 分发提供（spec 0.3 §6.5）。
fn notes_ctx(notes: &Option<NotesCtx>) -> Result<&NotesCtx, (String, String)> {
    notes.as_ref().ok_or_else(|| {
        ("unsupported".into(), "notes.*/ai.complete 仅在 command/ui 调用上下文可用".into())
    })
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

// ---------- notes.*（spec 0.3 §6.5；能力 notes:read / notes:write）----------

fn notes_get(ctx: &mut HostCtx, params: &Value) -> HostResult {
    let n = notes_ctx(&ctx.notes)?;
    let id = str_param(params, "id")?;
    let lib = n.library.read().unwrap();
    match lib.note(id) {
        Some(note) => Ok(json!({ "note": note })),
        None => Err(("not_found".into(), format!("笔记不存在: {id}"))),
    }
}

fn notes_search(ctx: &mut HostCtx, params: &Value) -> HostResult {
    let n = notes_ctx(&ctx.notes)?;
    let query = str_param(params, "query")?;
    let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(20).clamp(1, 100) as usize;
    let lib = n.library.read().unwrap();
    let notes: Vec<Value> = lib
        .search(query)
        .into_iter()
        .take(limit)
        .map(|note| json!({ "id": note.id, "title": note.title, "parent_id": note.parent_id }))
        .collect();
    Ok(json!({ "notes": notes }))
}

fn notes_list_folders(ctx: &mut HostCtx) -> HostResult {
    let n = notes_ctx(&ctx.notes)?;
    let lib = n.library.read().unwrap();
    let mut folders: Vec<(String, String, String)> = lib
        .folders
        .values()
        .map(|f| (f.title.clone(), f.id.clone(), f.parent_id.clone()))
        .collect();
    folders.sort();
    let folders: Vec<Value> = folders
        .into_iter()
        .map(|(title, id, parent_id)| json!({ "id": id, "title": title, "parent_id": parent_id }))
        .collect();
    Ok(json!({ "folders": folders }))
}

/// `notes.upsert`：写确认 = 提案回传（spec §6.5）。免确认关（默认）→ 不落盘，
/// 提案进 pending 累积器、返回 pending:true；开 → 直写（**跳过 before-save 钩子**，防重入）。
fn notes_upsert(ctx: &mut HostCtx, params: &Value) -> HostResult {
    // 拆字段借用：notes 只读、io_time 可写，互不冲突
    let HostCtx { notes, io_time, plugin_id, .. } = ctx;
    let n = notes_ctx(notes)?;
    if n.read_only {
        return Err(("forbidden".into(), "服务处于只读模式".into()));
    }
    let id = str_param(params, "id")?;
    let (original_raw, proposed, orig_title, orig_body) = {
        let lib = n.library.read().unwrap();
        let raw = lib
            .note_raw(id)
            .map(str::to_string)
            .ok_or_else(|| ("not_found".to_string(), format!("笔记不存在: {id}")))?;
        let mut note =
            lib.note(id).cloned().ok_or_else(|| ("not_found".to_string(), format!("笔记不存在: {id}")))?;
        let (orig_title, orig_body) = (note.title.clone(), note.body.clone());
        if let Some(t) = params.get("title").and_then(Value::as_str) {
            note.title = t.to_string();
        }
        if let Some(b) = params.get("body").and_then(Value::as_str) {
            note.body = b.to_string();
        }
        (raw, note, orig_title, orig_body)
    };

    if !n.auto_approve {
        n.pending.lock().unwrap().push(json!({
            "action": "update",
            "plugin_id": plugin_id,
            "note": proposed,
            "original": { "title": orig_title, "body": orig_body },
        }));
        return Ok(json!({ "note": proposed, "pending": true }));
    }

    let storage =
        n.storage.clone().ok_or_else(|| ("internal".to_string(), "数据源未配置".to_string()))?;
    let content =
        crate::serialize::update_note_md(&original_raw, &proposed.title, &proposed.body, crate::serialize::now_ms())
            .map_err(|e| ("internal".to_string(), e.to_string()))?;
    let started = Instant::now();
    let saved = crate::api::persist_note_blocking(&n.library, storage.as_ref(), &n.events, id, &content);
    *io_time += started.elapsed();
    let saved = saved.map_err(|e| ("internal".to_string(), format!("写入失败: {e}")))?;
    Ok(json!({ "note": saved, "pending": false }))
}

/// `notes.create`：parent 必须存在（invalid）；pending 提案的 note.id 为空串（spec §6.5）。
fn notes_create(ctx: &mut HostCtx, params: &Value) -> HostResult {
    let HostCtx { notes, io_time, plugin_id, .. } = ctx;
    let n = notes_ctx(notes)?;
    if n.read_only {
        return Err(("forbidden".into(), "服务处于只读模式".into()));
    }
    let parent_id = str_param(params, "parent_id")?;
    {
        let lib = n.library.read().unwrap();
        if !lib.folders.contains_key(parent_id) {
            return Err(("invalid".into(), format!("笔记本不存在: {parent_id}")));
        }
    }
    let title = params.get("title").and_then(Value::as_str).unwrap_or("");
    let body = params.get("body").and_then(Value::as_str).unwrap_or("");
    let now = crate::serialize::now_ms();

    if !n.auto_approve {
        let proposed = Note {
            id: String::new(), // 批准落盘时由宿主生成
            parent_id: parent_id.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            created_time: now,
            updated_time: now,
            markup_language: MarkupLanguage::Markdown,
            is_todo: false,
            todo_completed: false,
            is_conflict: false,
            source_url: String::new(),
            order: 0,
        };
        n.pending.lock().unwrap().push(json!({
            "action": "create",
            "plugin_id": plugin_id,
            "note": proposed,
            "original": Value::Null,
        }));
        return Ok(json!({ "note": proposed, "pending": true }));
    }

    let storage =
        n.storage.clone().ok_or_else(|| ("internal".to_string(), "数据源未配置".to_string()))?;
    let id = crate::serialize::new_id();
    let content = crate::serialize::new_note_md(&id, parent_id, title, body, false, now);
    let started = Instant::now();
    let saved = crate::api::persist_note_blocking(&n.library, storage.as_ref(), &n.events, &id, &content);
    *io_time += started.elapsed();
    let saved = saved.map_err(|e| ("internal".to_string(), format!("写入失败: {e}")))?;
    Ok(json!({ "note": saved, "pending": false }))
}

/// `ai.complete`（spec 0.3 §6.5）：宿主代理的一次性补全，实现在 ai.rs（genai）。
fn ai_complete(ctx: &mut HostCtx, params: &Value) -> HostResult {
    let HostCtx { notes, io_time, .. } = ctx;
    let n = notes_ctx(notes)?;
    super::ai::complete(n, io_time, params)
}
