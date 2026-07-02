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
        .route("/api/plugins/{id}/assets/{*path}", get(asset))
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
    let params = json!({ "id": cmd, "args": req.args });
    let r = tokio::task::spawn_blocking(move || {
        host.dispatch(&id, "command", params, super::runtime::CallClass::Normal)
    })
    .await;
    match r {
        Ok(Ok(result)) => Json(json!({ "result": result })).into_response(),
        Ok(Err(super::runtime::CallError::Plugin { code, message })) => {
            let status = match code.as_str() {
                "forbidden" => StatusCode::FORBIDDEN,
                "not_found" => StatusCode::NOT_FOUND,
                "invalid" => StatusCode::BAD_REQUEST,
                _ => StatusCode::BAD_GATEWAY, // 插件内部/上游失败（如 AI 端点报错）
            };
            (status, Json(json!({ "error": code, "message": message }))).into_response()
        }
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "internal", "message": e.to_string() })),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
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
            library: RwLock::new(Library::default()),
            storage: RwLock::new(None),
            config,
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(read_only),
            plugins: Some(host),
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

    /// 极简 stub AI 端点：任意 POST 都返回一段固定的 Messages API 响应。
    fn spawn_stub_ai(response_json: &'static str) -> String {
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

    /// ai-polish 全链路：装插件 → 启用（授权 settings+host:http）→ 存 API 参数（secret 不回显）
    /// → POST commands/polish → wasm 经 host:http 调 stub AI → 返回优化正文。
    #[tokio::test(flavor = "multi_thread")]
    async fn ai_polish_command_end_to_end() {
        let examples =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples/ai-polish");
        if !examples.join("plugin.wasm").exists() {
            eprintln!("跳过：ai-polish/plugin.wasm 未构建（先跑 plugins-examples/build-wasm.sh）");
            return;
        }
        let stub = spawn_stub_ai(
            r#"{"content":[{"type":"text","text":"优化后的正文"}],"stop_reason":"end_turn"}"#,
        );

        // 装好插件（直接落目录再建宿主）
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("ai-polish");
        std::fs::create_dir_all(&dst).unwrap();
        for f in ["manifest.toml", "plugin.wasm"] {
            std::fs::copy(examples.join(f), dst.join(f)).unwrap();
        }
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(dir.path().to_path_buf(), config.clone()).unwrap();
        let state = Arc::new(AppState {
            library: RwLock::new(Library::default()),
            storage: RwLock::new(None),
            config,
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(false),
            plugins: Some(host),
        });

        // 未启用时命令 404
        let (st, _) = send(
            state.clone(),
            "POST",
            "/api/plugins/ai-polish/commands/polish",
            b"{\"args\":{}}".to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::NOT_FOUND);

        // 启用（= 能力授权）
        let (st, body) =
            send(state.clone(), "POST", "/api/plugins/ai-polish/enable", b"{\"enabled\":true}".to_vec()).await;
        assert_eq!(st, StatusCode::OK, "{body}");

        // 未配置 key → 插件返回 invalid → 400，带可读提示
        let (st, body) = send(
            state.clone(),
            "POST",
            "/api/plugins/ai-polish/commands/polish",
            r#"{"args":{"note_id":"x","title":"t","body":"原文内容"}}"#.to_string().into_bytes(),
        )
        .await;
        assert_eq!(st, StatusCode::BAD_REQUEST, "{body}");
        assert!(body["message"].as_str().unwrap().contains("API Key"));

        // 存 AI 参数（api_url 指向 stub）
        let settings =
            serde_json::json!({ "values": { "api_url": stub, "api_key": "sk-test", "model": "claude-opus-4-8" } });
        let (st, _) = send(
            state.clone(),
            "PUT",
            "/api/plugins/ai-polish/settings",
            settings.to_string().into_bytes(),
        )
        .await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        // secret 不回显
        let (_, body) = send(state.clone(), "GET", "/api/plugins/ai-polish/settings", vec![]).await;
        assert!(body["values"].get("api_key").is_none());
        assert_eq!(body["secret_set"]["api_key"], true);

        // 触发命令 → 优化后的正文
        let (st, body) = send(
            state.clone(),
            "POST",
            "/api/plugins/ai-polish/commands/polish",
            r#"{"args":{"note_id":"x","title":"t","body":"原文内容，有一些病句。。"}}"#.to_string().into_bytes(),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["body"], "优化后的正文");

        // 只读模式下命令被写方法守卫拦截
        state.read_only.store(true, std::sync::atomic::Ordering::Relaxed);
        let (st, _) = send(
            state.clone(),
            "POST",
            "/api/plugins/ai-polish/commands/polish",
            b"{\"args\":{}}".to_vec(),
        )
        .await;
        assert_eq!(st, StatusCode::FORBIDDEN);
    }

    /// ai-polish 的 OpenAI 格式路径：provider=openai → 打 OpenAI Chat Completions 形状的 stub。
    #[tokio::test(flavor = "multi_thread")]
    async fn ai_polish_openai_format() {
        let examples =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples/ai-polish");
        if !examples.join("plugin.wasm").exists() {
            eprintln!("跳过：ai-polish/plugin.wasm 未构建");
            return;
        }
        let stub = spawn_stub_ai(
            r#"{"choices":[{"message":{"role":"assistant","content":"openai 优化后"},"finish_reason":"stop"}]}"#,
        );
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("ai-polish");
        std::fs::create_dir_all(&dst).unwrap();
        for f in ["manifest.toml", "plugin.wasm"] {
            std::fs::copy(examples.join(f), dst.join(f)).unwrap();
        }
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(dir.path().to_path_buf(), config.clone()).unwrap();
        let state = Arc::new(AppState {
            library: RwLock::new(Library::default()),
            storage: RwLock::new(None),
            config,
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(false),
            plugins: Some(host),
        });
        send(state.clone(), "POST", "/api/plugins/ai-polish/enable", b"{\"enabled\":true}".to_vec()).await;
        let settings = serde_json::json!({ "values": {
            "provider": "openai", "api_url": stub, "api_key": "sk-test", "model": "gpt-4o"
        } });
        let (st, _) = send(state.clone(), "PUT", "/api/plugins/ai-polish/settings", settings.to_string().into_bytes()).await;
        assert_eq!(st, StatusCode::NO_CONTENT);
        let (st, body) = send(
            state.clone(),
            "POST",
            "/api/plugins/ai-polish/commands/polish",
            r#"{"args":{"note_id":"x","title":"t","body":"原文"}}"#.to_string().into_bytes(),
        )
        .await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["result"]["body"], "openai 优化后");
    }
}
