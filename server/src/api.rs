//! HTTP API 层（axum）。
//!
//! 配置：
//!   GET    /api/status           是否已配置 + 计数 + 数据源类型
//!   GET    /api/config           当前配置
//!   PUT    /api/config           设置/切换数据源（连接并重建索引）
//! 读：
//!   GET    /api/folders          笔记本树
//!   GET    /api/notes?folder=ID  笔记列表
//!   GET    /api/notes/{id}       笔记详情
//!   GET    /api/resources/{id}   资源二进制
//!   GET    /api/search?q=...     全文搜索
//! 写：
//!   POST   /api/notes            新建笔记
//!   PUT    /api/notes/{id}       更新笔记
//!   DELETE /api/notes/{id}       删除笔记

use crate::config::{self, ConfigStore, SourceConfig};
use crate::library::Library;
use crate::model::{MarkupLanguage, Note};
use crate::serialize;
use crate::storage::StorageBackend;
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, RwLock};

pub struct AppState {
    pub library: RwLock<Library>,
    pub storage: RwLock<Option<Arc<dyn StorageBackend>>>,
    pub config: Mutex<ConfigStore>,
}

fn storage_of(state: &Arc<AppState>) -> Option<Arc<dyn StorageBackend>> {
    state.storage.read().unwrap().clone()
}

pub fn router(state: Arc<AppState>) -> Router {
    use tower_http::cors::CorsLayer;
    Router::new()
        .route("/api/status", get(status))
        .route("/api/config", get(get_config).put(apply_config))
        .route("/api/folders", get(folders))
        .route("/api/notes", get(notes_list).post(create_note))
        .route("/api/notes/{id}", get(note_detail).put(update_note).delete(delete_note))
        .route("/api/resources/{id}", get(resource))
        .route("/api/search", get(search))
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
struct CreateNoteReq {
    parent_id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    body: String,
}

fn summarize(n: &Note) -> NoteSummary {
    NoteSummary {
        id: n.id.clone(),
        title: n.title.clone(),
        updated_time: n.updated_time,
        parent_id: n.parent_id.clone(),
        is_todo: n.is_todo,
        todo_completed: n.todo_completed,
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
    let built = tokio::task::spawn_blocking(move || -> anyhow::Result<(Library, crate::library::BuildStats)> {
        if create_new {
            st.init_new()?;
        }
        let (lib, stats) = Library::build(st.as_ref())?;
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

async fn create_note(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateNoteReq>,
) -> Result<Json<NoteDetail>, StatusCode> {
    let id = serialize::new_id();
    let content =
        serialize::new_note_md(&id, &req.parent_id, &req.title, &req.body, serialize::now_ms());

    write_item(&state, format!("{id}.md"), content.clone()).await?;

    let mut lib = state.library.write().unwrap();
    lib.upsert_note(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let n = lib.note(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(detail_of(n)))
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
