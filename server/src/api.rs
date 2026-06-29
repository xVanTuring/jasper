//! HTTP API 层（axum）。
//!
//! 路由：
//!   GET /api/folders          笔记本树（嵌套 + 篇数）
//!   GET /api/notes?folder=ID   某笔记本下的笔记列表（folder 省略=根）
//!   GET /api/notes/{id}        单篇笔记详情
//!   GET /api/resources/{id}    资源二进制（按需读取，带 mime 头）
//!   GET /api/search?q=...      全文搜索

use crate::library::Library;
use crate::model::MarkupLanguage;
use crate::storage::StorageBackend;
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};

pub struct AppState {
    pub library: RwLock<Arc<Library>>,
    pub storage: Arc<dyn StorageBackend>,
}

impl AppState {
    fn lib(&self) -> Arc<Library> {
        self.library.read().unwrap().clone()
    }
}

pub fn router(state: Arc<AppState>) -> Router {
    use tower_http::cors::CorsLayer;
    Router::new()
        .route("/api/folders", get(folders))
        .route("/api/notes", get(notes_list))
        .route("/api/notes/{id}", get(note_detail))
        .route("/api/resources/{id}", get(resource))
        .route("/api/search", get(search))
        // 开发期允许跨源（Vite dev server 5173 → API）。阶段2 加鉴权时收紧。
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// ---------- DTO ----------

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
    /// 1 = Markdown, 2 = HTML
    markup_language: i32,
    parent_id: String,
    created_time: i64,
    updated_time: i64,
    source_url: String,
    is_todo: bool,
    todo_completed: bool,
}

fn markup_int(m: MarkupLanguage) -> i32 {
    match m {
        MarkupLanguage::Markdown => 1,
        MarkupLanguage::Html => 2,
    }
}

fn summarize(n: &crate::model::Note) -> NoteSummary {
    NoteSummary {
        id: n.id.clone(),
        title: n.title.clone(),
        updated_time: n.updated_time,
        parent_id: n.parent_id.clone(),
        is_todo: n.is_todo,
        todo_completed: n.todo_completed,
    }
}

// ---------- handlers ----------

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
    let lib = state.lib();
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
    let lib = state.lib();
    let folder = q.folder.unwrap_or_default();
    let notes = lib
        .notes_in_folder_sorted(&folder)
        .into_iter()
        .map(summarize)
        .collect();
    Json(notes)
}

async fn note_detail(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<NoteDetail>, StatusCode> {
    let lib = state.lib();
    let n = lib.note(&id).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(NoteDetail {
        id: n.id.clone(),
        title: n.title.clone(),
        body: n.body.clone(),
        markup_language: markup_int(n.markup_language),
        parent_id: n.parent_id.clone(),
        created_time: n.created_time,
        updated_time: n.updated_time,
        source_url: n.source_url.clone(),
        is_todo: n.is_todo,
        todo_completed: n.todo_completed,
    }))
}

async fn resource(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    let lib = state.lib();
    let mime = lib
        .resource(&id)
        .map(|r| r.mime.clone())
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    // 资源可能来自 WebDAV（阻塞网络 IO），放到 blocking 线程，避免阻塞异步执行器。
    let storage = state.storage.clone();
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
    let lib = state.lib();
    let q = sq.q.unwrap_or_default();
    Json(lib.search(&q).into_iter().map(summarize).collect())
}
