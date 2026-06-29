//! HTTP API 层（axum）。
//!
//! 读：
//!   GET    /api/folders          笔记本树（嵌套 + 篇数）
//!   GET    /api/notes?folder=ID   某笔记本下的笔记列表
//!   GET    /api/notes/{id}        单篇笔记详情
//!   GET    /api/resources/{id}    资源二进制（按需读取，带 mime 头）
//!   GET    /api/search?q=...      全文搜索
//! 写：
//!   POST   /api/notes            新建笔记 { parent_id, title?, body? }
//!   PUT    /api/notes/{id}        更新笔记 { title, body }
//!   DELETE /api/notes/{id}        删除笔记

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
use std::sync::{Arc, RwLock};

pub struct AppState {
    pub library: RwLock<Library>,
    pub storage: Arc<dyn StorageBackend>,
}

pub fn router(state: Arc<AppState>) -> Router {
    use tower_http::cors::CorsLayer;
    Router::new()
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
    let mime = {
        let lib = state.library.read().unwrap();
        lib.resource(&id)
            .map(|r| r.mime.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "application/octet-stream".to_string())
    };

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
    let lib = state.library.read().unwrap();
    let q = sq.q.unwrap_or_default();
    Json(lib.search(&q).into_iter().map(summarize).collect())
}

// ---------- 写 handlers ----------

// 把文件写回存储后端（阻塞 IO 放到 blocking 线程）
async fn write_item(state: &Arc<AppState>, name: String, content: String) -> Result<(), StatusCode> {
    let storage = state.storage.clone();
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
    // 取原始内容（保留元数据）
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
    let content = serialize::new_note_md(&id, &req.parent_id, &req.title, &req.body, serialize::now_ms());

    write_item(&state, format!("{id}.md"), content.clone()).await?;

    let mut lib = state.library.write().unwrap();
    lib.upsert_note(&content).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let n = lib.note(&id).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(detail_of(n)))
}

async fn delete_note(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> StatusCode {
    let storage = state.storage.clone();
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
