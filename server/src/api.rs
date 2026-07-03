//! HTTP API 层（axum）。
//!
//! 配置：
//!   GET    /api/status           是否已配置 + 计数 + 数据源类型
//!   GET    /api/config           当前配置
//!   PUT    /api/config           设置/切换数据源（连接并重建索引）
//! 读：
//!   GET    /api/folders          笔记本树
//!   POST   /api/folders          新建笔记本 { parent_id?, title }
//!   PUT    /api/folders/{id}     重命名笔记本 { title }
//!   PUT    /api/folders/{id}/move 移动笔记本（改 parent_id；防环）
//!   GET    /api/notes?folder=ID  笔记列表
//!   GET    /api/notes/{id}       笔记详情
//!   GET    /api/tags             标签列表（含篇数）
//!   GET    /api/tags/{id}/notes  某标签下的笔记
//!   GET    /api/notes/{id}/tags  某笔记的标签
//! 写：
//!   POST   /api/notes/{id}/tags        给笔记打标签 { title }（标签不存在则新建，兼容 Joplin）
//!   DELETE /api/notes/{id}/tags/{tid}  从笔记去掉某标签（删 note_tag 关联，保留标签本身）
//!   GET    /api/resources        资源清单（含引用计数）
//!   GET    /api/resources/{id}   资源二进制
//!   GET    /api/search?q=...     全文搜索
//!   GET    /api/events           SSE 变更流（事件 `change`：{kind, op, id}；前端按需刷新）
//! 写：
//!   POST   /api/notes            新建笔记
//!   PUT    /api/notes/{id}       更新笔记
//!   PUT    /api/notes/{id}/move  移动笔记到另一笔记本（改 parent_id）
//!   DELETE /api/notes/{id}       删除笔记
//!   POST   /api/resources        上传资源（二进制为体），写 .resource/<id> + <id>.md
//!   PUT    /api/resources/{id}   重命名资源
//!   DELETE /api/resources/{id}   删除资源（二进制 + 元数据）

use crate::config::{self, ConfigStore, SourceConfig};
use crate::library::Library;
use crate::model::{MarkupLanguage, Note};
use crate::serialize;
use crate::storage::StorageBackend;
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, Query, Request, State},
    http::{header, HeaderMap, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    routing::{delete, get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};

pub struct AppState {
    /// Arc 化以便把引用递进插件 dispatch（wasmi Store 拥有 ctx、spawn_blocking 要 'static）。
    pub library: Arc<RwLock<Library>>,
    pub storage: RwLock<Option<Arc<dyn StorageBackend>>>,
    /// Arc 化以便与 PluginHost 共享同一配置库连接。
    pub config: Arc<Mutex<ConfigStore>>,
    pub cache: crate::cache::CacheStore,
    /// 只读模式：为真时中间件拒绝一切写方法（/api/config 除外）。运行时可切换。
    pub read_only: AtomicBool,
    /// 插件宿主（--features plugins；关闭或初始化失败时为 None）。
    pub plugins: Option<Arc<crate::plugins::PluginHost>>,
    /// 变更事件总线（SSE /api/events）：一切写路径在此广播，前端按需刷新。
    pub events: crate::events::EventBus,
}

fn storage_of(state: &Arc<AppState>) -> Option<Arc<dyn StorageBackend>> {
    state.storage.read().unwrap().clone()
}

pub fn router(state: Arc<AppState>) -> Router {
    use tower_http::cors::CorsLayer;
    use tower_http::trace::TraceLayer;
    Router::new()
        .route("/api/status", get(status))
        .route("/api/config", get(get_config).put(apply_config))
        .route("/api/folders", get(folders).post(create_folder))
        .route("/api/folders/{id}", put(rename_folder))
        .route("/api/folders/{id}/move", put(move_folder))
        .route("/api/notes", get(notes_list).post(create_note))
        .route("/api/notes/{id}", get(note_detail).put(update_note).delete(delete_note))
        .route("/api/notes/{id}/move", put(move_note))
        .route("/api/tags", get(tags_list))
        .route("/api/tags/{id}/notes", get(tag_notes))
        .route("/api/notes/{id}/tags", get(note_tags_list).post(add_note_tag))
        .route("/api/notes/{id}/tags/{tag_id}", delete(remove_note_tag))
        .route("/api/resources", get(list_resources).post(upload_resource))
        .route("/api/resources/{id}", get(resource).put(rename_resource).delete(delete_resource))
        .route("/api/search", get(search))
        .route("/api/events", get(events_sse))
        // 插件管理路由（feature off = 空路由）。挂在只读守卫之内 → 只读时插件写操作同样被拦。
        .merge(crate::plugins::api_router())
        // 只读守卫：只读模式下拦截一切写方法（/api/config 除外）。放在最内层，
        // 保证它能拿到 State 且早于任何 handler 运行。
        .layer(axum::middleware::from_fn_with_state(state.clone(), guard_read_only))
        // 资源上传可能较大（图片/附件），放宽请求体上限到 100MB
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        // 每个请求一条 method+path+status+耗时 的结构化日志（tower_http=debug 时可见，见 main.rs 默认过滤）
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// 只读强制拦截（核心安全保证）：只读开启时，凡写方法（POST/PUT/DELETE/PATCH）一律 403，
/// 唯一例外是 `/api/config`（否则无法在设置页把只读关回去）。读方法（GET/HEAD/OPTIONS）放行。
/// 按 HTTP 方法集中拦截 → 将来新增任何写路由自动被覆盖，不依赖逐个 handler 记得判断。
async fn guard_read_only(State(state): State<Arc<AppState>>, req: Request, next: Next) -> Response {
    let is_write = matches!(
        *req.method(),
        Method::POST | Method::PUT | Method::DELETE | Method::PATCH
    );
    let is_config = req.uri().path() == "/api/config";
    if is_write && !is_config && state.read_only.load(Ordering::Relaxed) {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "read_only", "message": "服务处于只读模式，写操作被拒绝" })),
        )
            .into_response();
    }
    next.run(req).await
}

// ---------- DTO ----------

#[derive(Serialize)]
struct StatusResp {
    configured: bool,
    source_type: String,
    notes: usize,
    folders: usize,
    read_only: bool,
    /// 服务端版本（市场 UI 拿它做 minHostVersion 兼容过滤）
    version: &'static str,
}

#[derive(Serialize)]
struct ConfigResult {
    ok: bool,
    error: Option<String>,
    notes: usize,
    folders: usize,
}

impl ConfigResult {
    fn err(e: impl std::fmt::Display) -> Self {
        Self { ok: false, error: Some(e.to_string()), notes: 0, folders: 0 }
    }
}

#[derive(Deserialize)]
struct ApplyConfigReq {
    source_type: String,
    #[serde(default)]
    local_path: String,
    #[serde(default)]
    webdav_url: String,
    #[serde(default)]
    webdav_user: String,
    #[serde(default)]
    webdav_pass: String,
    /// source_type=="plugin"（插件存储 provider，spec 0.2）
    #[serde(default)]
    plugin_id: String,
    #[serde(default)]
    plugin_storage: String,
    #[serde(default)]
    plugin_config: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    read_only: bool,
    #[serde(default)]
    create_new: bool,
}

#[derive(Serialize)]
struct FolderNode {
    id: String,
    title: String,
    note_count: usize,
    children: Vec<FolderNode>,
}

#[derive(Serialize)]
struct NoteSummary {
    id: String,
    title: String,
    updated_time: i64,
    parent_id: String,
    is_todo: bool,
    todo_completed: bool,
    /// 正文内 markdown 任务清单的完成/总数（总数 0 = 无任务清单）。
    task_done: usize,
    task_total: usize,
}

#[derive(Serialize)]
struct NoteDetail {
    id: String,
    title: String,
    body: String,
    markup_language: i32, // 1=Markdown, 2=HTML
    parent_id: String,
    created_time: i64,
    updated_time: i64,
    source_url: String,
    is_todo: bool,
    todo_completed: bool,
}

#[derive(Deserialize)]
struct UpdateNoteReq {
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct MoveNoteReq {
    parent_id: String,
}

#[derive(Deserialize)]
struct CreateNoteReq {
    parent_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
    /// 是否建为待办（is_todo: 1）。
    #[serde(default)]
    is_todo: bool,
}

#[derive(Deserialize)]
struct CreateFolderReq {
    #[serde(default)]
    parent_id: String, // 空 = 根
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct MoveFolderReq {
    parent_id: String, // 空 = 移到根
}

#[derive(Deserialize)]
struct RenameFolderReq {
    title: String,
}

#[derive(Serialize)]
struct FolderRef {
    id: String,
    title: String,
    parent_id: String,
}

fn summarize(n: &Note) -> NoteSummary {
    let (task_done, task_total) = crate::library::count_tasks(&n.body);
    NoteSummary {
        id: n.id.clone(),
        title: n.title.clone(),
        updated_time: n.updated_time,
        parent_id: n.parent_id.clone(),
        is_todo: n.is_todo,
        todo_completed: n.todo_completed,
        task_done,
        task_total,
    }
}

fn detail_of(n: &Note) -> NoteDetail {
    NoteDetail {
        id: n.id.clone(),
        title: n.title.clone(),
        body: n.body.clone(),
        markup_language: match n.markup_language {
            MarkupLanguage::Markdown => 1,
            MarkupLanguage::Html => 2,
        },
        parent_id: n.parent_id.clone(),
        created_time: n.created_time,
        updated_time: n.updated_time,
        source_url: n.source_url.clone(),
        is_todo: n.is_todo,
        todo_completed: n.todo_completed,
    }
}

// ---------- 配置 handlers ----------

async fn status(State(state): State<Arc<AppState>>) -> Json<StatusResp> {
    let configured = state.storage.read().unwrap().is_some();
    let (notes, folders) = {
        let lib = state.library.read().unwrap();
        (lib.notes.len(), lib.folders.len())
    };
    let source_type = state
        .config
        .lock()
        .unwrap()
        .load()
        .map(|c| c.source_type)
        .unwrap_or_default();
    let read_only = state.read_only.load(Ordering::Relaxed);
    Json(StatusResp { configured, source_type, notes, folders, read_only, version: env!("CARGO_PKG_VERSION") })
}

async fn get_config(State(state): State<Arc<AppState>>) -> Json<SourceConfig> {
    Json(state.config.lock().unwrap().load().unwrap_or_default())
}

async fn apply_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApplyConfigReq>,
) -> Json<ConfigResult> {
    tracing::info!(source_type = %req.source_type, create_new = req.create_new, "applying data source config");
    let mut cfg = SourceConfig {
        source_type: req.source_type,
        local_path: req.local_path,
        webdav_url: req.webdav_url,
        webdav_user: req.webdav_user,
        webdav_pass: req.webdav_pass,
        plugin_id: req.plugin_id,
        plugin_storage: req.plugin_storage,
        plugin_config: serde_json::to_string(&req.plugin_config).unwrap_or_default(),
        read_only: req.read_only,
        ..Default::default()
    };
    // 插件数据源：按贡献的 config_schema 校验/规范化，并算出缓存隔离键（不含 secret）
    if cfg.source_type == "plugin" {
        if let Err(e) = crate::plugins::prepare_plugin_source(&mut cfg, state.plugins.as_ref()) {
            return Json(ConfigResult::err(e));
        }
    }
    let storage = match config::build_storage(&cfg, state.plugins.as_ref()) {
        Ok(s) => s,
        Err(e) => return Json(ConfigResult::err(e)),
    };

    // 连接 / 初始化 / 建索引放到 blocking 线程（含网络/磁盘 IO）
    let st = storage.clone();
    let create_new = req.create_new;
    let source = config::source_key(&cfg);
    let state2 = state.clone();
    let built = tokio::task::spawn_blocking(move || -> anyhow::Result<(Library, crate::library::BuildStats)> {
        if create_new {
            st.init_new()?;
        }
        let (lib, stats) = crate::indexer::build_cached(st.as_ref(), &state2.cache, &source)?;
        Ok((lib, stats))
    })
    .await;

    match built {
        Ok(Ok((lib, stats))) => {
            tracing::info!(
                source_type = %cfg.source_type, notes = stats.notes, folders = stats.folders,
                "data source switched",
            );
            *state.library.write().unwrap() = lib;
            *state.storage.write().unwrap() = Some(storage);
            if let Err(e) = state.config.lock().unwrap().save(&cfg) {
                return Json(ConfigResult::err(e));
            }
            // 运行时同步只读开关（设置页切换即时生效，无需重启）
            state.read_only.store(cfg.read_only, Ordering::Relaxed);
            // 整库被替换：通知所有客户端全量刷新（含发起切换的那个之外的标签页）
            state.events.library_reloaded();
            Json(ConfigResult { ok: true, error: None, notes: stats.notes, folders: stats.folders })
        }
        Ok(Err(e)) => {
            tracing::warn!("data source switch failed: {e}");
            Json(ConfigResult::err(e))
        }
        Err(_) => {
            tracing::warn!("data source switch task panicked");
            Json(ConfigResult::err("处理任务失败"))
        }
    }
}

// ---------- 读 handlers ----------

fn build_folder_tree(lib: &Library, parent_id: &str) -> Vec<FolderNode> {
    lib.child_folder_ids_sorted(parent_id)
        .into_iter()
        .filter_map(|id| {
            lib.folders.get(&id).map(|f| FolderNode {
                id: f.id.clone(),
                title: f.title.clone(),
                note_count: lib.note_count(&f.id),
                children: build_folder_tree(lib, &f.id),
            })
        })
        .collect()
}

async fn folders(State(state): State<Arc<AppState>>) -> Json<Vec<FolderNode>> {
    let lib = state.library.read().unwrap();
    let mut nodes = build_folder_tree(&lib, "");
    // 未挂在任何笔记本下的笔记（parent_id==""）用一个合成节点表示，放最前面；
    // id 仍是 ""，前端已有的 selectFolder("")/api.notes("") 直接复用。标题留空，
    // 由前端按 id==="" 特判取本地化文案（服务端不做 i18n）。
    let root_count = lib.note_count("");
    if root_count > 0 {
        nodes.insert(0, FolderNode { id: String::new(), title: String::new(), note_count: root_count, children: Vec::new() });
    }
    Json(nodes)
}

#[derive(Deserialize)]
struct NotesQuery {
    folder: Option<String>,
}

async fn notes_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<NotesQuery>,
) -> Json<Vec<NoteSummary>> {
    let lib = state.library.read().unwrap();
    let folder = q.folder.unwrap_or_default();
    Json(lib.notes_in_folder_sorted(&folder).into_iter().map(summarize).collect())
}

async fn note_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<NoteDetail>, StatusCode> {
    let lib = state.library.read().unwrap();
    let n = lib.note(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(detail_of(n)))
}

#[derive(Serialize)]
struct TagInfo {
    id: String,
    title: String,
    /// 打了该标签且仍存在的笔记数。
    note_count: usize,
}

/// GET /api/tags —— 全部标签（按标题排序，含篇数）。
async fn tags_list(State(state): State<Arc<AppState>>) -> Json<Vec<TagInfo>> {
    let lib = state.library.read().unwrap();
    Json(
        lib.tags_sorted()
            .into_iter()
            .map(|t| TagInfo { id: t.id.clone(), title: t.title.clone(), note_count: lib.tag_note_count(&t.id) })
            .collect(),
    )
}

/// GET /api/tags/{id}/notes —— 打了某标签的笔记（按更新时间倒序）。
async fn tag_notes(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Json<Vec<NoteSummary>> {
    let lib = state.library.read().unwrap();
    Json(lib.notes_with_tag(&id).into_iter().map(summarize).collect())
}

#[derive(Serialize)]
struct TagRef {
    id: String,
    title: String,
}

fn tag_ref(t: &crate::model::Tag) -> TagRef {
    TagRef { id: t.id.clone(), title: t.title.clone() }
}

#[derive(Deserialize)]
struct AddTagReq {
    title: String,
}

/// GET /api/notes/{id}/tags —— 某笔记的标签（按标题排序）。
async fn note_tags_list(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<TagRef>>, StatusCode> {
    let lib = state.library.read().unwrap();
    if lib.note(&id).is_none() {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(Json(lib.tags_of_note(&id).into_iter().map(tag_ref).collect()))
}

/// 打标签的执行计划（在读锁内算好，避免持锁做 IO / 借用冲突）。
enum TagPlan {
    /// 笔记已有该标签 → 幂等无操作。
    Noop,
    /// 需落盘：可选的新建标签条目 + 必建的 note_tag 关联条目（均为 (文件名, 内容)）。
    Create {
        new_tag: Option<(String, String)>,
        note_tag: (String, String),
    },
}

/// POST /api/notes/{id}/tags —— 给笔记打标签 { title }。
/// 语义对齐 Joplin `addNoteTagByTitle`：标题 trim + 不区分大小写复用已有标签，
/// 不存在则新建（type_=5），再建 note_tag 关联（type_=6）；笔记已有该标签则幂等。
async fn add_note_tag(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AddTagReq>,
) -> Result<Json<Vec<TagRef>>, StatusCode> {
    // 提早确认有数据源（写路径）。
    storage_of(&state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let title = req.title.trim().to_string();
    if title.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let now = serialize::now_ms();

    let plan = {
        let lib = state.library.read().unwrap();
        if lib.note(&id).is_none() {
            return Err(StatusCode::NOT_FOUND);
        }
        match lib.tag_id_by_title(&title) {
            Some(tid) if lib.note_has_tag(&id, &tid) => TagPlan::Noop,
            Some(tid) => {
                let ntid = serialize::new_id();
                TagPlan::Create {
                    new_tag: None,
                    note_tag: (ntid.clone(), serialize::new_note_tag_md(&ntid, &id, &tid, now)),
                }
            }
            None => {
                let tid = serialize::new_id();
                let ntid = serialize::new_id();
                TagPlan::Create {
                    new_tag: Some((tid.clone(), serialize::new_tag_md(&tid, &title, now))),
                    note_tag: (ntid.clone(), serialize::new_note_tag_md(&ntid, &id, &tid, now)),
                }
            }
        }
    };

    if let TagPlan::Create { new_tag, note_tag } = plan {
        // 先落标签再落关联：读者先看到关联时，其引用的标签已存在。
        if let Some((tid, content)) = new_tag {
            write_item(&state, format!("{tid}.md"), content.clone()).await?;
            state.library.write().unwrap().upsert_tag(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        }
        let (ntid, content) = note_tag;
        write_item(&state, format!("{ntid}.md"), content.clone()).await?;
        state.library.write().unwrap().upsert_note_tag(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state.events.tags_changed(&id);
    }

    let lib = state.library.read().unwrap();
    Ok(Json(lib.tags_of_note(&id).into_iter().map(tag_ref).collect()))
}

/// DELETE /api/notes/{id}/tags/{tag_id} —— 从笔记去掉某标签（删 note_tag 关联，保留标签本身）。
/// 对齐 Joplin `removeNote`：删除该 (note,tag) 的全部关联；无关联则幂等返回当前标签。
async fn remove_note_tag(
    State(state): State<Arc<AppState>>,
    Path((id, tag_id)): Path<(String, String)>,
) -> Result<Json<Vec<TagRef>>, StatusCode> {
    storage_of(&state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let ntids = state.library.read().unwrap().note_tag_ids_for(&id, &tag_id);
    if !ntids.is_empty() {
        for ntid in &ntids {
            delete_item(&state, format!("{ntid}.md")).await?;
            state.library.write().unwrap().remove_note_tag(ntid);
        }
        state.events.tags_changed(&id);
    }
    let lib = state.library.read().unwrap();
    Ok(Json(lib.tags_of_note(&id).into_iter().map(tag_ref).collect()))
}

async fn resource(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> Response {
    let storage = match storage_of(&state) {
        Some(s) => s,
        None => return StatusCode::NOT_FOUND.into_response(),
    };
    let mime = {
        let lib = state.library.read().unwrap();
        lib.resource(&id)
            .map(|r| r.mime.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "application/octet-stream".to_string())
    };
    let bytes = tokio::task::spawn_blocking(move || storage.get_resource(&id)).await;
    match bytes {
        Ok(Ok(bytes)) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, "public, max-age=31536000".to_string()),
            ],
            bytes,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

// ---------- 资源上传 ----------

#[derive(Deserialize)]
struct UploadQuery {
    #[serde(default)]
    filename: String,
}

#[derive(Serialize)]
struct UploadResp {
    id: String,
    title: String,
    mime: String,
    file_extension: String,
    size: usize,
    /// 可直接插入笔记正文的引用片段（图片用 `![]`，其它用 `[]`）。
    markdown: String,
}

/// 从文件名末尾取扩展名（小写，限字母数字、长度 ≤8）。
fn ext_from_filename(name: &str) -> Option<String> {
    let (_, e) = name.rsplit_once('.')?;
    let e = e.to_lowercase();
    (!e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric())).then_some(e)
}

/// 常见 MIME → 扩展名兜底（文件名无扩展名时用）。
fn ext_from_mime(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/avif" => "avif",
        "application/pdf" => "pdf",
        _ => return None,
    })
}

/// POST /api/resources?filename=<名> —— 请求体为原始二进制，Content-Type 为其 MIME。
/// 写入 `.resource/<id>` 二进制 + `<id>.md` 元数据条目，返回可插入正文的引用片段。
async fn upload_resource(
    State(state): State<Arc<AppState>>,
    Query(q): Query<UploadQuery>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<UploadResp>, StatusCode> {
    let storage = storage_of(&state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    if body.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let mime = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let filename = q.filename.trim().to_string();
    let ext = ext_from_filename(&filename)
        .or_else(|| ext_from_mime(&mime).map(|s| s.to_string()))
        .unwrap_or_default();
    let id = serialize::new_id();
    let title = match (filename.is_empty(), ext.is_empty()) {
        (false, _) => filename.clone(),
        (true, false) => format!("{id}.{ext}"),
        (true, true) => id.clone(),
    };
    let size = body.len();
    let content = serialize::new_resource_md(&id, &title, &mime, &ext, size as i64, serialize::now_ms());

    // 先写二进制再写元数据（读者看到元数据时二进制已就绪）。IO 放 blocking 线程。
    let st = storage.clone();
    let id2 = id.clone();
    let content2 = content.clone();
    let bytes = body.to_vec();
    let written = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        st.put_resource(&id2, &bytes)?;
        st.put_item(&format!("{id2}.md"), &content2)?;
        Ok(())
    })
    .await;
    if !matches!(written, Ok(Ok(()))) {
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    state
        .library
        .write()
        .unwrap()
        .upsert_resource(&content)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let markdown = if mime.starts_with("image/") {
        format!("![{title}](:/{id})")
    } else {
        format!("[{title}](:/{id})")
    };
    Ok(Json(UploadResp { id, title, mime, file_extension: ext, size, markdown }))
}

// ---------- 资源管理 ----------

#[derive(Serialize)]
struct ResourceInfo {
    id: String,
    title: String,
    mime: String,
    file_extension: String,
    size: i64,
    updated_time: i64,
    /// 引用该资源的笔记数（0 = 无人引用的孤儿）。
    used_by: usize,
}

fn resource_info(r: &crate::model::Resource, usage: &std::collections::HashMap<String, usize>) -> ResourceInfo {
    ResourceInfo {
        id: r.id.clone(),
        title: r.title.clone(),
        mime: r.mime.clone(),
        file_extension: r.file_extension.clone(),
        size: r.size,
        updated_time: r.updated_time,
        used_by: usage.get(&r.id).copied().unwrap_or(0),
    }
}

/// GET /api/resources —— 资源清单（含引用计数）。孤儿在前，其次按体积降序。
async fn list_resources(State(state): State<Arc<AppState>>) -> Json<Vec<ResourceInfo>> {
    let lib = state.library.read().unwrap();
    let usage = lib.resource_usage();
    let mut out: Vec<ResourceInfo> = lib.resources.values().map(|r| resource_info(r, &usage)).collect();
    out.sort_by(|a, b| a.used_by.cmp(&b.used_by).then(b.size.cmp(&a.size)));
    Json(out)
}

#[derive(Deserialize)]
struct RenameResourceReq {
    title: String,
}

/// PUT /api/resources/{id} —— 重命名资源（改标题元数据，刷新更新时间）。
async fn rename_resource(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<RenameResourceReq>,
) -> Result<Json<ResourceInfo>, StatusCode> {
    let storage = storage_of(&state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let name = format!("{id}.md");

    // 取原元数据 → 改标题 → 写回（IO 走 blocking）
    let st = storage.clone();
    let name2 = name.clone();
    let original = tokio::task::spawn_blocking(move || st.get_item(&name2))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let content = serialize::update_resource_md(&original, req.title.trim(), serialize::now_ms())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    write_item(&state, name, content.clone()).await?;

    let mut lib = state.library.write().unwrap();
    lib.upsert_resource(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let usage = lib.resource_usage();
    let r = lib.resource(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(resource_info(r, &usage)))
}

/// DELETE /api/resources/{id} —— 删除资源（二进制 + 元数据条目）。
/// 不检查引用：调用方（前端）应先就引用情况向用户确认。
async fn delete_resource(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> StatusCode {
    let storage = match storage_of(&state) {
        Some(s) => s,
        None => return StatusCode::SERVICE_UNAVAILABLE,
    };
    let id2 = id.clone();
    let res = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        storage.delete_item(&format!("{id2}.md"))?; // 先删元数据（决定是否出现在清单）
        storage.delete_resource(&id2)?; // 再删二进制（幂等）
        Ok(())
    })
    .await;
    match res {
        Ok(Ok(())) => {
            state.library.write().unwrap().remove_resource(&id);
            StatusCode::NO_CONTENT
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

async fn search(
    State(state): State<Arc<AppState>>,
    Query(sq): Query<SearchQuery>,
) -> Json<Vec<NoteSummary>> {
    let lib = state.library.read().unwrap();
    let q = sq.q.unwrap_or_default();
    Json(lib.search(&q).into_iter().map(summarize).collect())
}

/// GET /api/events —— SSE 变更流（事件名 `change`，data 为 ChangeEvent JSON）。
/// 只带 (kind, op, id)，内容由前端按需再拉；接收端落后（lagged，慢消费者被丢事件）
/// 折算成一条 library reload，前端全量刷新兜底。GET 方法天然通过只读守卫。
async fn events_sse(
    State(state): State<Arc<AppState>>,
) -> axum::response::sse::Sse<impl tokio_stream::Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>>
{
    use axum::response::sse::{Event, KeepAlive, Sse};
    use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
    use tokio_stream::wrappers::BroadcastStream;
    use tokio_stream::StreamExt as _;

    let stream = BroadcastStream::new(state.events.subscribe()).filter_map(|item| {
        let ev = match item {
            Ok(ev) => ev,
            Err(BroadcastStreamRecvError::Lagged(_)) => crate::events::ChangeEvent::reload(),
        };
        // 事件是纯 &'static str + String 结构，序列化不会失败；防御性丢弃而非 panic
        Event::default().event("change").json_data(&ev).ok().map(Ok)
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------- 写 handlers ----------

async fn write_item(state: &Arc<AppState>, name: String, content: String) -> Result<(), StatusCode> {
    let storage = storage_of(state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    tokio::task::spawn_blocking(move || storage.put_item(&name, &content))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn delete_item(state: &Arc<AppState>, name: String) -> Result<(), StatusCode> {
    let storage = storage_of(state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    tokio::task::spawn_blocking(move || storage.delete_item(&name))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// 笔记保存原语（阻塞）：写存储 + 刷内存索引 + 广播变更事件，返回最新 Note 快照。
/// api handlers（经 [`persist_note`]）与插件 notes.upsert/create 直写路径（已在阻塞线程）共用；
/// **不含 before-save 钩子**——由调用方决定（插件直写路径明确跳过，spec 0.3 §6.5 防重入）。
/// 事件在这个单一咽喉上发，普通 API 写入 / 插件免确认直写 / 外部 curl 写入天然全覆盖。
pub(crate) fn persist_note_blocking(
    library: &RwLock<Library>,
    storage: &dyn StorageBackend,
    events: &crate::events::EventBus,
    id: &str,
    content: &str,
) -> anyhow::Result<Note> {
    tracing::debug!(note_id = id, bytes = content.len(), "persisting note");
    if let Err(e) = storage.put_item(&format!("{id}.md"), content) {
        tracing::warn!(note_id = id, "failed to write note to storage: {e}");
        return Err(e);
    }
    let mut lib = library.write().unwrap();
    lib.upsert_note(content)?;
    let note = lib.note(id).cloned().ok_or_else(|| anyhow::anyhow!("写入后索引缺失: {id}"))?;
    events.note_upserted(id);
    Ok(note)
}

async fn persist_note(state: &Arc<AppState>, id: String, content: String) -> Result<Note, StatusCode> {
    let storage = storage_of(state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let library = state.library.clone();
    let events = state.events.clone();
    tokio::task::spawn_blocking(move || persist_note_blocking(&library, storage.as_ref(), &events, &id, &content))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update_note(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteReq>,
) -> Result<Json<NoteDetail>, StatusCode> {
    let (original, hook_note) = {
        let lib = state.library.read().unwrap();
        let original = lib.note_raw(&id).map(|s| s.to_string()).ok_or(StatusCode::NOT_FOUND)?;
        // 给 before-save 钩子的笔记形状：现有元数据 + 新标题/正文
        let mut n = lib.note(&id).ok_or(StatusCode::NOT_FOUND)?.clone();
        n.title = req.title.clone();
        n.body = req.body.clone();
        (original, n)
    };
    // 插件 before-save 钩子（无插件时直通；插件失败回退原值，不丢数据）
    let (title, body) = crate::plugins::before_save(&state.plugins, hook_note).await;
    let content = serialize::update_note_md(&original, &title, &body, serialize::now_ms())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let n = persist_note(&state, id, content).await?;
    Ok(Json(detail_of(&n)))
}

/// PUT /api/notes/{id}/move —— 把笔记移动到另一个笔记本（改 parent_id）。
/// 目标须为已存在的笔记本；已在目标则直接返回，不写回。
async fn move_note(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<MoveNoteReq>,
) -> Result<Json<NoteDetail>, StatusCode> {
    let new_parent = req.parent_id.trim().to_string();
    let original = {
        let lib = state.library.read().unwrap();
        // 目标必须是已存在的笔记本，避免把笔记移到不存在的父级而“消失”
        if !lib.folders.contains_key(&new_parent) {
            return Err(StatusCode::BAD_REQUEST);
        }
        let note = lib.note(&id).ok_or(StatusCode::NOT_FOUND)?;
        if note.parent_id == new_parent {
            return Ok(Json(detail_of(note))); // 无变化，免写回
        }
        lib.note_raw(&id).map(|s| s.to_string()).ok_or(StatusCode::NOT_FOUND)?
    };
    let content = serialize::move_note_md(&original, &new_parent, serialize::now_ms())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let n = persist_note(&state, id, content).await?;
    Ok(Json(detail_of(&n)))
}

async fn create_note(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNoteReq>,
) -> Result<Json<NoteDetail>, StatusCode> {
    let id = serialize::new_id();
    let now = serialize::now_ms();
    // 插件 before-save 钩子：以「将要创建的笔记」形状调用（spec §8 也覆盖新建）
    let hook_note = Note {
        id: id.clone(),
        parent_id: req.parent_id.clone(),
        title: req.title.clone(),
        body: req.body.clone(),
        created_time: now,
        updated_time: now,
        markup_language: MarkupLanguage::Markdown,
        is_todo: req.is_todo,
        todo_completed: false,
        is_conflict: false,
        source_url: String::new(),
        order: 0,
    };
    let (title, body) = crate::plugins::before_save(&state.plugins, hook_note).await;
    let content = serialize::new_note_md(&id, &req.parent_id, &title, &body, req.is_todo, now);

    let n = persist_note(&state, id, content).await?;
    Ok(Json(detail_of(&n)))
}

/// POST /api/folders —— 新建笔记本。parent_id 空=根，非空须为已存在笔记本。
async fn create_folder(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateFolderReq>,
) -> Result<Json<FolderRef>, StatusCode> {
    let parent = req.parent_id.trim().to_string();
    let title = req.title.trim().to_string();
    if title.is_empty() {
        return Err(StatusCode::BAD_REQUEST); // 名称由前端保证非空（带本地化默认名）
    }
    if !parent.is_empty() {
        let lib = state.library.read().unwrap();
        if !lib.folders.contains_key(&parent) {
            return Err(StatusCode::BAD_REQUEST);
        }
    }
    let id = serialize::new_id();
    let content = serialize::new_folder_md(&id, &parent, &title, serialize::now_ms());
    write_item(&state, format!("{id}.md"), content.clone()).await?;
    {
        let mut lib = state.library.write().unwrap();
        lib.upsert_folder(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    state.events.folder_changed(&id);
    Ok(Json(FolderRef { id, title, parent_id: parent }))
}

/// PUT /api/folders/{id} —— 重命名笔记本（只改标题）。笔记本原始 .md 未缓存，故从存储现取。
async fn rename_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<RenameFolderReq>,
) -> Result<Json<FolderRef>, StatusCode> {
    let storage = storage_of(&state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let title = req.title.trim().to_string();
    if title.is_empty() {
        return Err(StatusCode::BAD_REQUEST); // 名称由前端保证非空
    }
    {
        let lib = state.library.read().unwrap();
        if !lib.folders.contains_key(&id) {
            return Err(StatusCode::NOT_FOUND);
        }
    }
    let name = format!("{id}.md");
    let st = storage.clone();
    let name2 = name.clone();
    let original = tokio::task::spawn_blocking(move || st.get_item(&name2))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let content = serialize::rename_folder_md(&original, &title, serialize::now_ms())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    write_item(&state, name, content.clone()).await?;
    let resp = {
        let mut lib = state.library.write().unwrap();
        lib.upsert_folder(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let f = lib.folders.get(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        FolderRef { id: id.clone(), title: f.title.clone(), parent_id: f.parent_id.clone() }
    };
    state.events.folder_changed(&id);
    Ok(Json(resp))
}

/// PUT /api/folders/{id}/move —— 移动笔记本到新父级。parent_id 空=移到根。
/// 防环：不能移进自身或自身的后代。笔记本原始 .md 未缓存，故从存储现取。
async fn move_folder(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<MoveFolderReq>,
) -> Result<Json<FolderRef>, StatusCode> {
    let storage = storage_of(&state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    let new_parent = req.parent_id.trim().to_string();
    {
        let lib = state.library.read().unwrap();
        if !lib.folders.contains_key(&id) {
            return Err(StatusCode::NOT_FOUND);
        }
        if !new_parent.is_empty() && !lib.folders.contains_key(&new_parent) {
            return Err(StatusCode::BAD_REQUEST);
        }
        if lib.is_self_or_descendant(&id, &new_parent) {
            return Err(StatusCode::BAD_REQUEST); // 移进自身/后代 → 成环
        }
        if let Some(f) = lib.folders.get(&id) {
            if f.parent_id == new_parent {
                return Ok(Json(FolderRef { id: id.clone(), title: f.title.clone(), parent_id: new_parent }));
            }
        }
    }
    let name = format!("{id}.md");
    let st = storage.clone();
    let name2 = name.clone();
    let original = tokio::task::spawn_blocking(move || st.get_item(&name2))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::NOT_FOUND)?;
    let content = serialize::move_folder_md(&original, &new_parent, serialize::now_ms())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    write_item(&state, name, content.clone()).await?;
    let resp = {
        let mut lib = state.library.write().unwrap();
        lib.upsert_folder(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let f = lib.folders.get(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        FolderRef { id: id.clone(), title: f.title.clone(), parent_id: f.parent_id.clone() }
    };
    state.events.folder_changed(&id);
    Ok(Json(resp))
}

async fn delete_note(State(state): State<Arc<AppState>>, Path(id): Path<String>) -> StatusCode {
    let storage = match storage_of(&state) {
        Some(s) => s,
        None => return StatusCode::SERVICE_UNAVAILABLE,
    };
    let name = format!("{id}.md");
    let res = tokio::task::spawn_blocking(move || storage.delete_item(&name)).await;
    match res {
        Ok(Ok(())) => {
            state.library.write().unwrap().remove_note(&id);
            state.events.note_deleted(&id);
            StatusCode::NO_CONTENT
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt; // 提供 Router::oneshot

    // 无存储的最小 AppState（守卫在 handler 之前运行，故无需真实数据源）。
    fn state_with_read_only(read_only: bool) -> Arc<AppState> {
        Arc::new(AppState {
            library: Arc::new(RwLock::new(Library::default())),
            storage: RwLock::new(None),
            config: Arc::new(Mutex::new(ConfigStore::in_memory().unwrap())),
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(read_only),
            plugins: None,
            events: crate::events::EventBus::new(),
        })
    }

    async fn status_of(state: Arc<AppState>, method: &str, uri: &str, body: &'static str) -> StatusCode {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        router(state).oneshot(req).await.unwrap().status()
    }

    #[tokio::test]
    async fn read_only_blocks_writes_allows_reads_and_config() {
        // 写方法 → 403（守卫拦截，未触达 handler）
        assert_eq!(status_of(state_with_read_only(true), "POST", "/api/notes", "{}").await, StatusCode::FORBIDDEN);
        assert_eq!(status_of(state_with_read_only(true), "DELETE", "/api/notes/abc", "").await, StatusCode::FORBIDDEN);
        assert_eq!(status_of(state_with_read_only(true), "PUT", "/api/notes/abc", "{}").await, StatusCode::FORBIDDEN);
        assert_eq!(status_of(state_with_read_only(true), "POST", "/api/folders", "{}").await, StatusCode::FORBIDDEN);
        assert_eq!(status_of(state_with_read_only(true), "PUT", "/api/folders/abc", "{}").await, StatusCode::FORBIDDEN);
        // 读方法放行（空库 → 200）
        assert_eq!(status_of(state_with_read_only(true), "GET", "/api/folders", "").await, StatusCode::OK);
        // /api/config 豁免（PUT 不应被守卫拦成 403）
        assert_ne!(
            status_of(state_with_read_only(true), "PUT", "/api/config", "{\"source_type\":\"\"}").await,
            StatusCode::FORBIDDEN
        );
    }

    #[tokio::test]
    async fn non_read_only_does_not_block_writes() {
        // 非只读：写方法不再被守卫拦截（无存储 → 走到 handler 返回 503，而非 403）
        let st = status_of(state_with_read_only(false), "POST", "/api/notes", "{\"parent_id\":\"x\"}").await;
        assert_ne!(st, StatusCode::FORBIDDEN);
        assert_eq!(st, StatusCode::SERVICE_UNAVAILABLE);
    }

    /// SSE 端点：订阅后广播的事件应以 `event: change` + ChangeEvent JSON 流出。
    #[tokio::test]
    async fn events_sse_streams_changes() {
        use tokio_stream::StreamExt as _;

        let state = state_with_read_only(false);
        let req = Request::builder().method("GET").uri("/api/events").body(Body::empty()).unwrap();
        let resp = router(state.clone()).oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .starts_with("text/event-stream"));

        // handler 已在 oneshot 里订阅；现在发事件、再读第一帧
        state.events.note_upserted("abc123");
        let mut body = resp.into_body().into_data_stream();
        let frame = tokio::time::timeout(std::time::Duration::from_secs(5), body.next())
            .await
            .expect("5s 内应收到事件帧")
            .expect("流不应结束")
            .expect("帧不应出错");
        let text = String::from_utf8_lossy(&frame);
        assert!(text.contains("event: change"), "SSE 帧: {text}");
        assert!(text.contains(r#""kind":"note""#) && text.contains("abc123"), "SSE 帧: {text}");
    }

    /// 保存原语是事件的单一咽喉：写入成功后广播 note upsert（插件直写路径共用此原语）。
    #[tokio::test]
    async fn persist_note_emits_change_event() {
        let dir = tempfile::tempdir().unwrap();
        let storage = crate::storage::local::LocalStorage::new(dir.path());
        let state = state_with_read_only(false);
        let mut rx = state.events.subscribe();

        let id = "a".repeat(32);
        let content = serialize::new_note_md(&id, "", "标题", "正文", false, 0);
        persist_note_blocking(&state.library, &storage, &state.events, &id, &content).unwrap();

        let ev = rx.try_recv().expect("写入后应有事件");
        assert_eq!((ev.kind, ev.op), ("note", "upsert"));
        assert_eq!(ev.id, id);
    }

    async fn get_json(state: Arc<AppState>, uri: &str) -> serde_json::Value {
        let req = Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap();
        let resp = router(state).oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// /api/tags 列表（含篇数）与 /api/tags/{id}/notes 过滤。
    #[tokio::test]
    async fn tags_endpoints_list_and_filter() {
        let state = state_with_read_only(false);
        let n1 = "1".repeat(32);
        let n2 = "2".repeat(32);
        let tag = "a".repeat(32);
        let contents = vec![
            serialize::new_note_md(&n1, "", "笔记一", "正文", false, 100),
            serialize::new_note_md(&n2, "", "笔记二", "正文", false, 200),
            // 标签（type_=5）+ 关联（type_=6）：仅 n1 打了该标签
            format!("待办\n\nid: {tag}\nparent_id: \ntype_: 5"),
            format!("id: {}\nnote_id: {n1}\ntag_id: {tag}\ntype_: 6", "b".repeat(32)),
        ];
        let (lib, _) = Library::from_contents(contents);
        *state.library.write().unwrap() = lib;

        let tags = get_json(state.clone(), "/api/tags").await;
        let arr = tags.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], tag);
        assert_eq!(arr[0]["title"], "待办");
        assert_eq!(arr[0]["note_count"], 1);

        // 该标签下只有 n1
        let notes = get_json(state.clone(), &format!("/api/tags/{tag}/notes")).await;
        let narr = notes.as_array().unwrap();
        assert_eq!(narr.len(), 1);
        assert_eq!(narr[0]["id"], n1);

        // 未知标签 → 空列表（非 404）
        let empty = get_json(state.clone(), &format!("/api/tags/{}/notes", "f".repeat(32))).await;
        assert_eq!(empty, serde_json::json!([]));
    }

    async fn send_json(
        state: Arc<AppState>,
        method: &str,
        uri: &str,
        body: &str,
    ) -> (StatusCode, serde_json::Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = router(state).oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
        let val = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        (status, val)
    }

    /// 打标签全链路：新建/复用（不区分大小写）/幂等 + 去标签 + Joplin 兼容落盘。
    #[tokio::test]
    async fn note_tag_add_reuse_and_remove() {
        let dir = tempfile::tempdir().unwrap();
        let storage = Arc::new(crate::storage::local::LocalStorage::new(dir.path())) as Arc<dyn StorageBackend>;
        let state = state_with_read_only(false);
        *state.storage.write().unwrap() = Some(storage.clone());
        let note_id = "1".repeat(32);
        state
            .library
            .write()
            .unwrap()
            .upsert_note(&serialize::new_note_md(&note_id, "", "N", "body", false, 100))
            .unwrap();

        let tags_uri = format!("/api/notes/{note_id}/tags");

        // 打标签 "Work" → 新建标签 + 关联
        let (st, body) = send_json(state.clone(), "POST", &tags_uri, r#"{"title":"Work"}"#).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["title"], "Work");
        let work_id = body[0]["id"].as_str().unwrap().to_string();

        // 大小写不同 + 幂等：复用同一标签，笔记标签数不增
        let (_, body) = send_json(state.clone(), "POST", &tags_uri, r#"{"title":"  work "}"#).await;
        assert_eq!(body.as_array().unwrap().len(), 1);

        // /api/tags 里 Work 篇数为 1
        let (_, tags) = send_json(state.clone(), "GET", "/api/tags", "").await;
        let work = tags.as_array().unwrap().iter().find(|t| t["title"] == "Work").unwrap();
        assert_eq!(work["note_count"], 1);

        // Joplin 兼容：落盘的 note_tag 文件是纯元数据（无标题、type_=6），标签文件 type_=5
        let files: Vec<String> = storage.list_items().unwrap().into_iter().map(|i| i.name).collect();
        let mut saw_note_tag = false;
        let mut saw_tag = false;
        for name in &files {
            if name == &format!("{note_id}.md") {
                continue;
            }
            let content = storage.get_item(name).unwrap();
            if content.contains("\ntype_: 6") {
                saw_note_tag = true;
                assert!(content.starts_with("id: "), "note_tag 应无标题: {content}");
                assert!(content.contains(&format!("note_id: {note_id}")));
            } else if content.contains("\ntype_: 5") {
                saw_tag = true;
                assert!(content.starts_with("Work\n\nid: "), "tag 首行应为标题: {content}");
            }
        }
        assert!(saw_note_tag && saw_tag, "应已落盘 tag + note_tag");

        // 第二个标签
        let (_, body) = send_json(state.clone(), "POST", &tags_uri, r#"{"title":"Idea"}"#).await;
        assert_eq!(body.as_array().unwrap().len(), 2);

        // 去掉 Work → 只剩 Idea；Work 变孤儿（仍在 /api/tags，篇数 0）
        let (st, body) = send_json(state.clone(), "DELETE", &format!("{tags_uri}/{work_id}"), "").await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body.as_array().unwrap().len(), 1);
        assert_eq!(body[0]["title"], "Idea");
        let (_, tags) = send_json(state.clone(), "GET", "/api/tags", "").await;
        let work = tags.as_array().unwrap().iter().find(|t| t["title"] == "Work").unwrap();
        assert_eq!(work["note_count"], 0);

        // 空标题 400；不存在的笔记 404
        let (st, _) = send_json(state.clone(), "POST", &tags_uri, r#"{"title":"   "}"#).await;
        assert_eq!(st, StatusCode::BAD_REQUEST);
        let (st, _) = send_json(state.clone(), "POST", &format!("/api/notes/{}/tags", "9".repeat(32)), r#"{"title":"x"}"#).await;
        assert_eq!(st, StatusCode::NOT_FOUND);
    }

    /// 只读模式拦截打/去标签（守卫按方法拦截，无需触达 handler）。
    #[tokio::test]
    async fn read_only_blocks_tag_writes() {
        let id = "1".repeat(32);
        assert_eq!(
            send_json(state_with_read_only(true), "POST", &format!("/api/notes/{id}/tags"), r#"{"title":"x"}"#).await.0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            send_json(state_with_read_only(true), "DELETE", &format!("/api/notes/{id}/tags/{}", "2".repeat(32)), "").await.0,
            StatusCode::FORBIDDEN
        );
    }

    async fn folders_json(state: Arc<AppState>) -> serde_json::Value {
        let req = Request::builder().method("GET").uri("/api/folders").body(Body::empty()).unwrap();
        let resp = router(state).oneshot(req).await.unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    /// 没有笔记本归属(parent_id=="")的笔记（Bug: 不能看没有分组的笔记）——
    /// /api/folders 应合成一个 id=="" 的节点带上根笔记数，前端据此渲染「未分类笔记」入口。
    #[tokio::test]
    async fn folders_endpoint_synthesizes_unfiled_node_for_root_notes() {
        let state = state_with_read_only(false);

        // 空库：没有未分类笔记 → 不出现合成节点
        assert_eq!(folders_json(state.clone()).await, serde_json::json!([]));

        // 插入一条不属于任何笔记本的笔记
        let id = "b".repeat(32);
        let content = serialize::new_note_md(&id, "", "无归属", "正文", false, 0);
        state.library.write().unwrap().upsert_note(&content).unwrap();

        let nodes = folders_json(state.clone()).await;
        let arr = nodes.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["id"], "");
        assert_eq!(arr[0]["note_count"], 1);
        assert_eq!(arr[0]["children"], serde_json::json!([]));
    }
}
