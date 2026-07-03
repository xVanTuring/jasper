//! 插件管理 HTTP 路由（design §9）。挂在 api::router 的只读守卫之内：
//! 只读模式下写方法（install/enable/uninstall/settings PUT）自动被 403。

use super::host::{HostOpError, PluginHost};
use super::install::InstallError;
use super::manifest;
use crate::api::AppState;
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/plugins", get(list))
        .route("/api/plugins/install", post(install))
        .route("/api/plugins/{id}", delete(uninstall))
        .route("/api/plugins/{id}/enable", post(enable))
        .route("/api/plugins/{id}/settings", get(get_settings).put(put_settings))
        .route("/api/plugins/{id}/commands/{cmd}", post(run_command))
        .route("/api/plugins/{id}/ui/{view}", post(ui_view))
        .route("/api/plugins/{id}/auto-approve", axum::routing::put(put_auto_approve))
        .route("/api/plugins/{id}/assets/{*path}", get(asset))
        // 宿主级 AI 配置（spec 0.3 §9.5）：host:ai 的密钥/端点，插件永不可见。
        // PUT 在只读守卫之内自动被拦；GET 回显 api_key（与数据源密码同姿势，本机信任模型）。
        .route("/api/ai/config", get(get_ai_config).put(put_ai_config))
}

async fn get_ai_config(State(state): State<Arc<AppState>>) -> Json<crate::config::AiConfig> {
    Json(state.config.lock().unwrap().ai_config())
}

async fn put_ai_config(
    State(state): State<Arc<AppState>>,
    Json(cfg): Json<crate::config::AiConfig>,
) -> Response {
    if !cfg.provider.is_empty() && cfg.provider != "anthropic" && cfg.provider != "openai" {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid", "message": "provider 须为 anthropic|openai（或留空表示未配置）" })),
        )
            .into_response();
    }
    match state.config.lock().unwrap().save_ai_config(&cfg) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal", "message": e.to_string() })),
        )
            .into_response(),
    }
}

fn host_of(state: &Arc<AppState>) -> Result<Arc<PluginHost>, Response> {
    state.plugins.clone().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "plugins_unavailable", "message": "插件宿主初始化失败" })),
        )
            .into_response()
    })
}

fn op_error(e: HostOpError) -> Response {
    let (status, code) = match &e {
        HostOpError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        HostOpError::InUse => (StatusCode::CONFLICT, "in_use"),
        HostOpError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid"),
        HostOpError::Wasm(_) => (StatusCode::UNPROCESSABLE_ENTITY, "wasm_error"),
        HostOpError::Other(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal"),
    };
    (status, Json(json!({ "error": code, "message": e.to_string() }))).into_response()
}

async fn list(State(state): State<Arc<AppState>>) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    Json(json!({
        "host": {
            "version": env!("CARGO_PKG_VERSION"),
            "api_versions": manifest::HOST_API_VERSIONS,
        },
        "plugins": host.list_info(),
    }))
    .into_response()
}

#[derive(Deserialize)]
struct InstallQuery {
    #[serde(default)]
    force: bool,
}

/// POST /api/plugins/install —— 请求体为 zip 原始字节（同资源上传惯例）。
async fn install(
    State(state): State<Arc<AppState>>,
    Query(q): Query<InstallQuery>,
    body: Bytes,
) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    let installed = tokio::task::spawn_blocking(move || host.install(&body, q.force)).await;
    match installed {
        Ok(Ok(info)) => {
            let needs_consent = info.has_backend && !info.enabled;
            (StatusCode::CREATED, Json(json!({ "plugin": info, "needs_consent": needs_consent })))
                .into_response()
        }
        Ok(Err(InstallError::VersionConflict { installed })) => (
            StatusCode::CONFLICT,
            Json(json!({ "error": "version_conflict", "installed": installed })),
        )
            .into_response(),
        Ok(Err(e @ (InstallError::BadZip(_) | InstallError::BadManifest(_)))) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "bad_manifest", "message": e.to_string() })),
        )
            .into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal", "message": e.to_string() })),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn uninstall(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    let r = tokio::task::spawn_blocking(move || host.uninstall(&id)).await;
    match r {
        Ok(Ok(())) => StatusCode::NO_CONTENT.into_response(),
        Ok(Err(e)) => op_error(e),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
struct EnableReq {
    enabled: bool,
}

async fn enable(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<EnableReq>,
) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    let r = tokio::task::spawn_blocking(move || host.set_enabled(&id, req.enabled)).await;
    match r {
        Ok(Ok(info)) => Json(info).into_response(),
        Ok(Err(e)) => op_error(e),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn get_settings(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    match host.settings_values(&id) {
        Ok((values, secret_set)) => {
            Json(json!({ "values": values, "secret_set": secret_set })).into_response()
        }
        Err(e) => op_error(e),
    }
}

#[derive(Deserialize)]
struct PutSettingsReq {
    #[serde(default)]
    values: serde_json::Map<String, Value>,
}

async fn put_settings(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PutSettingsReq>,
) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    match host.set_settings(&id, &req.values) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => op_error(e),
    }
}

#[derive(Deserialize)]
struct RunCommandReq {
    #[serde(default)]
    args: Value,
}

/// POST /api/plugins/{id}/commands/{cmd} —— 执行 backend 命令（spec §6.5 `command`）。
/// 返回插件的 result 原样；插件业务错误按 code 映射 HTTP 状态。
/// 写方法 → 只读模式下被 guard_read_only 自动拦截。
/// command/ui 分发的 notes/ai 上下文（spec 0.3 §6.5）：快照 AppState 各件 + 空提案累积器。
fn notes_ctx_of(state: &Arc<AppState>, plugin_id: &str) -> super::runtime::NotesCtx {
    let (auto_approve, ai) = {
        let cfg = state.config.lock().unwrap();
        (cfg.plugin_write_auto_approve(plugin_id), cfg.ai_config())
    };
    super::runtime::NotesCtx {
        library: state.library.clone(),
        storage: state.storage.read().unwrap().clone(),
        read_only: state.read_only.load(std::sync::atomic::Ordering::Relaxed),
        auto_approve,
        handle: tokio::runtime::Handle::current(),
        ai,
        pending: Arc::new(std::sync::Mutex::new(Vec::new())),
        events: state.events.clone(),
    }
}

/// 统一的 dispatch 错误 → HTTP 状态映射（command 与 ui 共用）。
fn dispatch_error(e: super::runtime::CallError) -> Response {
    match e {
        super::runtime::CallError::Plugin { code, message } => {
            let status = match code.as_str() {
                "forbidden" => StatusCode::FORBIDDEN,
                "not_found" => StatusCode::NOT_FOUND,
                "invalid" => StatusCode::BAD_REQUEST,
                _ => StatusCode::BAD_GATEWAY, // 插件内部/上游失败（如 AI 端点报错）
            };
            (status, Json(json!({ "error": code, "message": message }))).into_response()
        }
        e => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal", "message": e.to_string() })),
        )
            .into_response(),
    }
}

async fn run_command(
    State(state): State<Arc<AppState>>,
    Path((id, cmd)): Path<(String, String)>,
    Json(req): Json<RunCommandReq>,
) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    if !host.has_backend_command(&id, &cmd) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "message": "插件未启用或无此 backend 命令" })),
        )
            .into_response();
    }
    let ctx = notes_ctx_of(&state, &id);
    let pending = ctx.pending.clone();
    let params = json!({ "id": cmd, "args": req.args });
    let r = tokio::task::spawn_blocking(move || {
        host.dispatch_with_notes(&id, "command", params, super::runtime::CallClass::Normal, Some(ctx))
    })
    .await;
    match r {
        // pending_writes 恒在（无提案 = 空数组）；调用失败时提案随 ctx 一起丢弃（spec §6.5）
        Ok(Ok(result)) => {
            let writes = std::mem::take(&mut *pending.lock().unwrap());
            Json(json!({ "result": result, "pending_writes": writes })).into_response()
        }
        Ok(Err(e)) => dispatch_error(e),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
struct UiViewReq {
    #[serde(default)]
    state: Value,
}

/// POST /api/plugins/{id}/ui/{view}（spec §9.5）：取 server-driven UI 声明树。
/// POST（携带 state）→ 只读模式下被写守卫拦截（已知取舍，spec §9.5）。
async fn ui_view(
    State(state): State<Arc<AppState>>,
    Path((id, view)): Path<(String, String)>,
    Json(req): Json<UiViewReq>,
) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    if !host.has_ui_view(&id, &view) {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "not_found", "message": "插件未启用或未声明该 view" })),
        )
            .into_response();
    }
    let ctx = notes_ctx_of(&state, &id);
    let pending = ctx.pending.clone();
    let params = json!({ "view": view, "state": req.state });
    let r = tokio::task::spawn_blocking(move || {
        host.dispatch_with_notes(&id, "ui", params, super::runtime::CallClass::Normal, Some(ctx))
    })
    .await;
    match r {
        Ok(Ok(ui)) => {
            let writes = std::mem::take(&mut *pending.lock().unwrap());
            Json(json!({ "ui": ui, "pending_writes": writes })).into_response()
        }
        Ok(Err(e)) => dispatch_error(e),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[derive(Deserialize)]
struct AutoApproveReq {
    enabled: bool,
}

/// PUT /api/plugins/{id}/auto-approve（spec §9.5）：notes:write 的「写入免确认」开关。
/// 宿主托管（不进插件 settings，防插件自改）；返回刷新后的插件信息。
async fn put_auto_approve(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AutoApproveReq>,
) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    let Some(_) = host.info(&id) else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "not_found" }))).into_response();
    };
    if let Err(e) = state.config.lock().unwrap().set_plugin_write_auto_approve(&id, req.enabled) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal", "message": e.to_string() })),
        )
            .into_response();
    }
    match host.info(&id) {
        Some(info) => Json(info).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// GET /api/plugins/{id}/assets/{path} —— 仅 enabled 插件；双重防逃逸
/// （规范化相对路径 + canonicalize 前缀校验）。
async fn asset(State(state): State<Arc<AppState>>, Path((id, path)): Path<(String, String)>) -> Response {
    let host = match host_of(&state) {
        Ok(h) => h,
        Err(r) => return r,
    };
    if !manifest::rel_path_ok(&path) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Some(root) = host.asset_root(&id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let mime = mime_of(&path);
    let bytes = tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
        let full = root.join(&path).canonicalize().ok()?;
        let root = root.canonicalize().ok()?;
        if !full.starts_with(&root) {
            return None;
        }
        std::fs::read(full).ok()
    })
    .await;
    match bytes {
        Ok(Some(bytes)) => (
            [
                (header::CONTENT_TYPE, mime.to_string()),
                // 插件可升级/卸载：不做长缓存，前端用 ?v=<version> 破缓存
                (header::CACHE_CONTROL, "no-cache".to_string()),
            ],
            bytes,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{router, AppState};
    use crate::config::ConfigStore;
    use crate::library::Library;
    use axum::body::Body;
    use axum::http::Request;
    use std::io::Write;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Mutex, RwLock};
    use tower::ServiceExt;

    fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(content.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        buf.into_inner()
    }

    fn state_with_host(read_only: bool) -> (tempfile::TempDir, Arc<AppState>) {
        let dir = tempfile::tempdir().unwrap();
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(dir.path().to_path_buf(), config.clone()).unwrap();
        let state = Arc::new(AppState {
            library: Arc::new(RwLock::new(Library::default())),
            storage: RwLock::new(None),
            config,
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(read_only),
            auth: crate::auth::AuthState::from_config(&crate::config::AuthConfig::default()),
            plugins: Some(host),
            events: crate::events::EventBus::new(),
        });
        (dir, state)
    }

    async fn send(state: Arc<AppState>, method: &str, uri: &str, body: Vec<u8>) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = router(state).oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, json)
    }

    const THEME_MANIFEST: &str = "id = \"demo-theme\"\nname = \"Demo 主题\"\nversion = \"1.0.0\"\napiVersion = \"0.1\"\n\n[[contributes.theme]]\nid = \"demo\"\nname = \"Demo\"\nbase = \"dark\"\ncss = \"assets/demo.css\"\n";

    #[tokio::test]
    async fn install_list_asset_disable_uninstall_flow() {
        let (_dir, state) = state_with_host(false);
        let zip = make_zip(&[("manifest.toml", THEME_MANIFEST), ("assets/demo.css", ":root{}")]);

        // 安装（零代码 → 自动启用，无需 consent）
        let (st, body) = send(state.clone(), "POST", "/api/plugins/install", zip.clone()).await;
        assert_eq!(st, StatusCode::CREATED, "{body}");
        assert_eq!(body["plugin"]["enabled"], true);
        assert_eq!(body["needs_consent"], false);

        // 重装同版本 → 409
        let (st, body) = send(state.clone(), "POST", "/api/plugins/install", zip).await;
        assert_eq!(st, StatusCode::CONFLICT);
        assert_eq!(body["error"], "version_conflict");

        // 列表
        let (st, body) = send(state.clone(), "GET", "/api/plugins", vec![]).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["plugins"][0]["id"], "demo-theme");
        assert_eq!(body["plugins"][0]["contributes"]["theme"][0]["css"], "assets/demo.css");

        // 资产可取；逃逸 404
        let (st, _) = send(state.clone(), "GET", "/api/plugins/demo-theme/assets/assets/demo.css", vec![]).await;
        assert_eq!(st, StatusCode::OK);
        let (st, _) =
            send(state.clone(), "GET", "/api/plugins/demo-theme/assets/..%2Fmanifest.toml", vec![]).await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // 停用后资产不可取
        let (st, _) = send(state.clone(), "POST", "/api/plugins/demo-theme/enable", b"{\"enabled\":false}".to_vec()).await;
        assert_eq!(st, StatusCode::OK);
        let (st, _) = send(state.clone(), "GET", "/api/plugins/demo-theme/assets/assets/demo.css", vec![]).await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // 卸载
        let (st, _) = send(state.clone(), "DELETE", "/api/plugins/demo-theme", vec![]).await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        let (_, body) = send(state, "GET", "/api/plugins", vec![]).await;
        assert_eq!(body["plugins"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn read_only_blocks_plugin_writes_but_allows_reads() {
        let (_dir, state) = state_with_host(true);
        let zip = make_zip(&[("manifest.toml", THEME_MANIFEST)]);
        let (st, body) = send(state.clone(), "POST", "/api/plugins/install", zip).await;
        assert_eq!(st, StatusCode::FORBIDDEN, "{body}"); // guard_read_only 拦截
        let (st, _) = send(state.clone(), "DELETE", "/api/plugins/x", vec![]).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        let (st, _) = send(state, "GET", "/api/plugins", vec![]).await;
        assert_eq!(st, StatusCode::OK); // 读放行（只读下主题继续可用）
    }

    #[tokio::test]
    async fn bad_zip_is_400() {
        let (_dir, state) = state_with_host(false);
        let (st, body) = send(state, "POST", "/api/plugins/install", b"not a zip".to_vec()).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "bad_manifest");
    }

    #[tokio::test]
    async fn ai_config_round_trip_and_validation() {
        let (_dir, state) = state_with_host(false);
        // 初始：全空
        let (st, body) = send(state.clone(), "GET", "/api/ai/config", vec![]).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["provider"], "");

        // 非法 provider → 400
        let (st, body) = send(
            state.clone(),
            "PUT",
            "/api/ai/config",
            br#"{"provider":"gemini","base_url":"","api_key":"k","model":"m"}"#.to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");

        // 保存 + 回读（api_key 回显，与数据源密码同姿势）
        let (st, _) = send(
            state.clone(),
            "PUT",
            "/api/ai/config",
            br#"{"provider":"openai","base_url":"http://127.0.0.1:11434/v1/","api_key":"sk-x","model":"qwen3"}"#
                .to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        let (_, body) = send(state, "GET", "/api/ai/config", vec![]).await;
        assert_eq!(body["provider"], "openai");
        assert_eq!(body["api_key"], "sk-x");
        assert_eq!(body["model"], "qwen3");
    }

    /// 插件宿主可用时，设置描述符须包含 AI 段（其字段/当前值/save 动作），并回显已存 api_key。
    #[tokio::test]
    async fn settings_schema_includes_ai_when_plugins_available() {
        let (_dir, state) = state_with_host(false);
        // 先存一份 AI 配置，验证描述符回显当前值
        let (st, _) = send(
            state.clone(),
            "PUT",
            "/api/ai/config",
            br#"{"provider":"openai","base_url":"","api_key":"sk-x","model":"qwen3"}"#.to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);

        let (st, body) = send(state, "GET", "/api/settings/schema", vec![]).await;
        assert_eq!(st, StatusCode::OK);
        let ai = body["sections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == "ai")
            .expect("ai section present with plugins")
            .clone();
        assert_eq!(ai["values"]["provider"], "openai");
        assert_eq!(ai["values"]["api_key"], "sk-x"); // 回显，非遮罩
        assert_eq!(ai["actions"][0]["request"]["url"], "/api/ai/config");
        assert!(ai["fields"].as_array().unwrap().iter().any(|f| f["key"] == "api_key"));
    }

    #[tokio::test]
    async fn ai_config_put_blocked_in_read_only() {
        let (_dir, state) = state_with_host(true);
        let (st, _) = send(
            state.clone(),
            "PUT",
            "/api/ai/config",
            br#"{"provider":"openai","base_url":"","api_key":"k","model":"m"}"#.to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        let (st, _) = send(state, "GET", "/api/ai/config", vec![]).await;
        assert_eq!(st, StatusCode::OK);
    }

    /// 极简 stub HTTP 端点：任意请求都返回一段固定的响应体。
    fn spawn_stub_http(response_json: &'static str) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { break };
                // 读到头结束 + 按 Content-Length 读完请求体（防 ureq 阻塞在写）
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                let (mut header_end, mut content_len) = (0usize, 0usize);
                loop {
                    let Ok(n) = s.read(&mut tmp) else { break };
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if header_end == 0 {
                        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                            header_end = pos + 4;
                            let head = String::from_utf8_lossy(&buf[..pos]);
                            content_len = head
                                .lines()
                                .find(|l| l.to_ascii_lowercase().starts_with("content-length:"))
                                .and_then(|l| l.split(':').nth(1))
                                .and_then(|v| v.trim().parse().ok())
                                .unwrap_or(0);
                        }
                    }
                    if header_end > 0 && buf.len() >= header_end + content_len {
                        break;
                    }
                }
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_json.len(),
                    response_json
                );
                let _ = s.write_all(resp.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    /// 后端命令全链路（testbed 的 relay 命令作夹具）：装插件 → 启用（授权 settings+host:http）
    /// → 存设置（secret 不回显）→ POST commands/relay → wasm 读 settings、经 host:http 调 stub
    /// → 返回 result.body。插件自身的业务逻辑（如 ai-polish 的 provider 请求形状）在
    /// jasper-plugins 仓库里做纯函数单测，这里只测宿主链路。
    #[tokio::test(flavor = "multi_thread")]
    async fn backend_command_end_to_end() {
        let examples =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples/testbed");
        if !examples.join("plugin.wasm").exists() {
            eprintln!("skipping: testbed/plugin.wasm not built (run plugins-examples/build-wasm.sh first)");
            return;
        }
        let stub = spawn_stub_http("relayed body");

        // 装好插件：manifest 本测试自写（settings+host:http + relay 命令），只复用 testbed 的 wasm
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("testbed");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::copy(examples.join("plugin.wasm"), dst.join("plugin.wasm")).unwrap();
        std::fs::write(
            dst.join("manifest.toml"),
            r#"
id = "testbed"
name = "Testbed"
version = "0.1.0"
apiVersion = "0.2"

[backend]
wasm = "plugin.wasm"
capabilities = ["settings", "host:http"]

[[contributes.command]]
id = "relay"
title = "Relay"
target = "backend"

[settings.schema]
target_url = { type = "string" }
token = { type = "secret" }
"#,
        )
        .unwrap();
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(dir.path().to_path_buf(), config.clone()).unwrap();
        let state = Arc::new(AppState {
            library: Arc::new(RwLock::new(Library::default())),
            storage: RwLock::new(None),
            config,
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(false),
            auth: crate::auth::AuthState::from_config(&crate::config::AuthConfig::default()),
            plugins: Some(host),
            events: crate::events::EventBus::new(),
        });

        // 未启用时命令 404
        let (st, _) = send(
            state.clone(),
            "POST",
            "/api/plugins/testbed/commands/relay",
            b"{\"args\":{}}".to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // 启用（= 能力授权）
        let (st, body) =
            send(state.clone(), "POST", "/api/plugins/testbed/enable", b"{\"enabled\":true}".to_vec()).await;
        assert_eq!(st, StatusCode::OK, "{body}");

        // 未配置 target_url → 插件返回 invalid → 400，带可读提示
        let (st, body) = send(
            state.clone(),
            "POST",
            "/api/plugins/testbed/commands/relay",
            r#"{"args":{"note_id":"x","title":"t","body":"原文内容"}}"#.to_string().into_bytes(),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        assert!(body["message"].as_str().unwrap().contains("target_url"));

        // 存设置（target_url 指向 stub；token 是 secret）
        let settings = serde_json::json!({ "values": { "target_url": stub, "token": "sk-secret" } });
        let (st, _) = send(
            state.clone(),
            "PUT",
            "/api/plugins/testbed/settings",
            settings.to_string().into_bytes(),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        // secret 不回显；非 secret 正常回显
        let (_, body) = send(state.clone(), "GET", "/api/plugins/testbed/settings", vec![]).await;
        assert!(body["values"].get("token").is_none());
        assert_eq!(body["secret_set"]["token"], true);
        assert_eq!(body["values"]["target_url"].as_str().unwrap(), stub);

        // 触发命令 → wasm 读 settings、经 host:http 取回 stub 响应作新正文
        let (st, body) = send(
            state.clone(),
            "POST",
            "/api/plugins/testbed/commands/relay",
            r#"{"args":{"note_id":"x","title":"t","body":"原文内容"}}"#.to_string().into_bytes(),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["body"], "relayed body");

        // 只读模式下命令被写方法守卫拦截
        state.read_only.store(true, std::sync::atomic::Ordering::Relaxed);
        let (st, _) = send(
            state.clone(),
            "POST",
            "/api/plugins/testbed/commands/relay",
            b"{\"args\":{}}".to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }

    /// system.locale 全链路（spec 0.4 §6.5）：宿主持久化 UI 语言 → 插件免能力读到同一值。
    #[tokio::test(flavor = "multi_thread")]
    async fn plugin_reads_system_locale() {
        let examples =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples/testbed");
        if !examples.join("plugin.wasm").exists() {
            eprintln!("skipping: testbed/plugin.wasm not built (run plugins-examples/build-wasm.sh first)");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("testbed");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::copy(examples.join("plugin.wasm"), dst.join("plugin.wasm")).unwrap();
        // read-locale 命令无需任何能力（system.locale 免能力）
        std::fs::write(
            dst.join("manifest.toml"),
            r#"
id = "testbed"
name = "Testbed"
version = "0.1.0"
apiVersion = "0.4"

[backend]
wasm = "plugin.wasm"
capabilities = []

[[contributes.command]]
id = "read-locale"
title = "Read locale"
target = "backend"
"#,
        )
        .unwrap();
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        // 前端持久化过的 UI 语言（这里直接写库模拟）
        config.lock().unwrap().set_ui_locale("fr").unwrap();
        let host = PluginHost::init_at(dir.path().to_path_buf(), config.clone()).unwrap();
        let state = Arc::new(AppState {
            library: Arc::new(RwLock::new(Library::default())),
            storage: RwLock::new(None),
            config,
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(false),
            auth: crate::auth::AuthState::from_config(&crate::config::AuthConfig::default()),
            plugins: Some(host),
            events: crate::events::EventBus::new(),
        });
        let (st, _) =
            send(state.clone(), "POST", "/api/plugins/testbed/enable", b"{\"enabled\":true}".to_vec()).await;
        assert_eq!(st, StatusCode::OK);

        let (st, body) = send(
            state.clone(),
            "POST",
            "/api/plugins/testbed/commands/read-locale",
            b"{\"args\":{}}".to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["locale"], "fr"); // 插件读到宿主持久化的 UI 语言
    }

    /// pending_writes 全链路 + ui 端点 + auto-approve 开关（spec 0.3 §9.5）。
    #[tokio::test(flavor = "multi_thread")]
    async fn ui_and_pending_writes_full_chain() {
        let examples =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples/testbed");
        if !examples.join("plugin.wasm").exists() {
            eprintln!("skipping: testbed/plugin.wasm not built (run plugins-examples/build-wasm.sh first)");
            return;
        }

        // 装 testbed（notes 能力 + write-note 命令 + 动态 sidebar view）
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("testbed");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::copy(examples.join("plugin.wasm"), dst.join("plugin.wasm")).unwrap();
        std::fs::write(
            dst.join("manifest.toml"),
            r#"
id = "testbed"
name = "Testbed"
version = "0.1.0"
apiVersion = "0.3"

[backend]
wasm = "plugin.wasm"
capabilities = ["notes:read", "notes:write"]

[[contributes.command]]
id = "write-note"
title = "Write"
target = "backend"

[[contributes.sidebar]]
id = "tools"
title = "工具"
widget = "markdown"
view = "main"
"#,
        )
        .unwrap();

        // 真实数据源：本地临时目录 + 预置库（1 笔记本 1 笔记）
        let folder_id = "f".repeat(32);
        let note_id = "a".repeat(32);
        let contents = vec![
            crate::serialize::new_folder_md(&folder_id, "", "收件箱", 1_700_000_000_000),
            crate::serialize::new_note_md(&note_id, &folder_id, "标题", "原始正文", false, 1_700_000_000_000),
        ];
        let source_dir = tempfile::tempdir().unwrap();
        std::fs::write(source_dir.path().join(format!("{folder_id}.md")), &contents[0]).unwrap();
        std::fs::write(source_dir.path().join(format!("{note_id}.md")), &contents[1]).unwrap();
        let (lib, _) = Library::from_contents(contents);

        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(dir.path().to_path_buf(), config.clone()).unwrap();
        let state = Arc::new(AppState {
            library: Arc::new(RwLock::new(lib)),
            storage: RwLock::new(Some(Arc::new(crate::storage::local::LocalStorage::new(
                source_dir.path(),
            )))),
            config,
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(false),
            auth: crate::auth::AuthState::from_config(&crate::config::AuthConfig::default()),
            plugins: Some(host),
            events: crate::events::EventBus::new(),
        });
        let (st, _) =
            send(state.clone(), "POST", "/api/plugins/testbed/enable", b"{\"enabled\":true}".to_vec()).await;
        assert_eq!(st, StatusCode::OK);

        // 默认（免确认关）：命令返回提案，盘不动
        let req = format!(r#"{{"args":{{"id":"{note_id}","body":"插件改写"}}}}"#);
        let (st, body) = send(
            state.clone(),
            "POST",
            "/api/plugins/testbed/commands/write-note",
            req.clone().into_bytes(),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["pending"], true);
        let writes = body["pending_writes"].as_array().unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0]["action"], "update");
        assert_eq!(writes[0]["plugin_id"], "testbed");
        assert_eq!(writes[0]["note"]["body"], "插件改写");
        assert_eq!(writes[0]["original"]["body"], "原始正文");
        let disk = std::fs::read_to_string(source_dir.path().join(format!("{note_id}.md"))).unwrap();
        assert!(disk.contains("原始正文"), "提案不落盘");

        // 开免确认（宿主托管端点）→ 返回刷新后的 PluginInfo
        let (st, body) = send(
            state.clone(),
            "PUT",
            "/api/plugins/testbed/auto-approve",
            b"{\"enabled\":true}".to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["write_auto_approve"], true);
        // 未知插件 → 404
        let (st, _) =
            send(state.clone(), "PUT", "/api/plugins/nope/auto-approve", b"{\"enabled\":true}".to_vec()).await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // 再写：直写落盘，pending_writes 空
        let (st, body) =
            send(state.clone(), "POST", "/api/plugins/testbed/commands/write-note", req.into_bytes()).await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["pending"], false);
        assert_eq!(body["pending_writes"].as_array().unwrap().len(), 0);
        let disk = std::fs::read_to_string(source_dir.path().join(format!("{note_id}.md"))).unwrap();
        assert!(disk.contains("插件改写"), "免确认应直写: {disk:?}");

        // ui 端点：声明过的 view 返回树；未声明的 404
        let (st, body) =
            send(state.clone(), "POST", "/api/plugins/testbed/ui/main", b"{\"state\":null}".to_vec()).await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["ui"]["type"], "markdown");
        assert_eq!(body["ui"]["children"][0]["type"], "button");
        assert_eq!(body["pending_writes"].as_array().unwrap().len(), 0);
        let (st, _) =
            send(state.clone(), "POST", "/api/plugins/testbed/ui/nope", b"{}".to_vec()).await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // 只读：ui / auto-approve 均被写守卫拦截
        state.read_only.store(true, std::sync::atomic::Ordering::Relaxed);
        let (st, _) = send(state.clone(), "POST", "/api/plugins/testbed/ui/main", b"{}".to_vec()).await;
        assert_eq!(st, StatusCode::FORBIDDEN);
        let (st, _) = send(
            state.clone(),
            "PUT",
            "/api/plugins/testbed/auto-approve",
            b"{\"enabled\":false}".to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }
}
