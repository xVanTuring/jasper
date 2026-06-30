//! HTTP API 层（axum）。
//!
//! 配置：
//!   GET    /api/status           是否已配置 + 计数 + 数据源类型
//!   GET    /api/config           当前配置
//!   PUT    /api/config           设置/切换数据源（连接并重建索引）
//! 读：
//!   GET    /api/folders          笔记本树
//!   POST   /api/folders          新建笔记本 { parent_id?, title }
//!   PUT    /api/folders/{id}/move 移动笔记本（改 parent_id；防环）
//!   GET    /api/notes?folder=ID  笔记列表
//!   GET    /api/notes/{id}       笔记详情
//!   GET    /api/resources        资源清单（含引用计数）
//!   GET    /api/resources/{id}   资源二进制
//!   GET    /api/search?q=...     全文搜索
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
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, RwLock};

pub struct AppState {
    pub library: RwLock<Library>,
    pub storage: RwLock<Option<Arc<dyn StorageBackend>>>,
    pub config: Mutex<ConfigStore>,
    pub cache: crate::cache::CacheStore,
}

fn storage_of(state: &Arc<AppState>) -> Option<Arc<dyn StorageBackend>> {
    state.storage.read().unwrap().clone()
}

pub fn router(state: Arc<AppState>) -> Router {
    use tower_http::cors::CorsLayer;
    Router::new()
        .route("/api/status", get(status))
        .route("/api/config", get(get_config).put(apply_config))
        .route("/api/folders", get(folders).post(create_folder))
        .route("/api/folders/{id}/move", put(move_folder))
        .route("/api/notes", get(notes_list).post(create_note))
        .route("/api/notes/{id}", get(note_detail).put(update_note).delete(delete_note))
        .route("/api/notes/{id}/move", put(move_note))
        .route("/api/resources", get(list_resources).post(upload_resource))
        .route("/api/resources/{id}", get(resource).put(rename_resource).delete(delete_resource))
        .route("/api/search", get(search))
        // 资源上传可能较大（图片/附件），放宽请求体上限到 100MB
        .layer(DefaultBodyLimit::max(100 * 1024 * 1024))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ---------- DTO ----------

#[derive(Serialize)]
struct StatusResp {
    configured: bool,
    source_type: String,
    notes: usize,
    folders: usize,
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
    Json(StatusResp { configured, source_type, notes, folders })
}

async fn get_config(State(state): State<Arc<AppState>>) -> Json<SourceConfig> {
    Json(state.config.lock().unwrap().load().unwrap_or_default())
}

async fn apply_config(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ApplyConfigReq>,
) -> Json<ConfigResult> {
    let cfg = SourceConfig {
        source_type: req.source_type,
        local_path: req.local_path,
        webdav_url: req.webdav_url,
        webdav_user: req.webdav_user,
        webdav_pass: req.webdav_pass,
    };
    let storage = match config::build_storage(&cfg) {
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
            *state.library.write().unwrap() = lib;
            *state.storage.write().unwrap() = Some(storage);
            if let Err(e) = state.config.lock().unwrap().save(&cfg) {
                return Json(ConfigResult::err(e));
            }
            Json(ConfigResult { ok: true, error: None, notes: stats.notes, folders: stats.folders })
        }
        Ok(Err(e)) => Json(ConfigResult::err(e)),
        Err(_) => Json(ConfigResult::err("处理任务失败")),
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
    Json(build_folder_tree(&lib, ""))
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

// ---------- 写 handlers ----------

async fn write_item(state: &Arc<AppState>, name: String, content: String) -> Result<(), StatusCode> {
    let storage = storage_of(state).ok_or(StatusCode::SERVICE_UNAVAILABLE)?;
    tokio::task::spawn_blocking(move || storage.put_item(&name, &content))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

async fn update_note(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateNoteReq>,
) -> Result<Json<NoteDetail>, StatusCode> {
    let original = {
        let lib = state.library.read().unwrap();
        lib.note_raw(&id).map(|s| s.to_string()).ok_or(StatusCode::NOT_FOUND)?
    };
    let content = serialize::update_note_md(&original, &req.title, &req.body, serialize::now_ms())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    write_item(&state, format!("{id}.md"), content.clone()).await?;

    let mut lib = state.library.write().unwrap();
    lib.upsert_note(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let n = lib.note(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(detail_of(n)))
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

    write_item(&state, format!("{id}.md"), content.clone()).await?;

    let mut lib = state.library.write().unwrap();
    lib.upsert_note(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let n = lib.note(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(detail_of(n)))
}

async fn create_note(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNoteReq>,
) -> Result<Json<NoteDetail>, StatusCode> {
    let id = serialize::new_id();
    let content =
        serialize::new_note_md(&id, &req.parent_id, &req.title, &req.body, req.is_todo, serialize::now_ms());

    write_item(&state, format!("{id}.md"), content.clone()).await?;

    let mut lib = state.library.write().unwrap();
    lib.upsert_note(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let n = lib.note(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(detail_of(n)))
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
    let mut lib = state.library.write().unwrap();
    lib.upsert_folder(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(FolderRef { id, title, parent_id: parent }))
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
    let mut lib = state.library.write().unwrap();
    lib.upsert_folder(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let f = lib.folders.get(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(FolderRef { id: id.clone(), title: f.title.clone(), parent_id: f.parent_id.clone() }))
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
            StatusCode::NO_CONTENT
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}
