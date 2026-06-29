//! 浏览器 WASM demo：把 joplin-core 编进浏览器，配内置演示库，
//! 暴露与后端 /api 同形的只读查询（返回 JSON 字符串，前端 JSON.parse）。
//!
//! 这不是完整后端——只覆盖 demo 需要的读路径（笔记本树 / 笔记列表 / 详情 / 搜索）。

use joplin_core::library::Library;
use joplin_core::model::{MarkupLanguage, Note};
use serde::Serialize;
use wasm_bindgen::prelude::*;

mod demo;

// ---- DTO：与 web/src/lib/api.ts 的接口一一对应 ----

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
    markup_language: i32,
    parent_id: String,
    created_time: i64,
    updated_time: i64,
    source_url: String,
    is_todo: bool,
    todo_completed: bool,
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

/// 浏览器侧的“库”：用内置演示数据构建，一次解析常驻。
#[wasm_bindgen]
pub struct Demo {
    lib: Library,
}

#[wasm_bindgen]
impl Demo {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Demo {
        let (lib, _stats) = Library::from_contents(demo::items());
        Demo { lib }
    }

    /// 笔记本树（嵌套 + 篇数），JSON。
    pub fn folders(&self) -> String {
        serde_json::to_string(&self.tree("")).unwrap_or_else(|_| "[]".into())
    }

    /// 某笔记本下的笔记列表（按更新时间倒序），JSON。
    pub fn notes(&self, folder: &str) -> String {
        let list: Vec<NoteSummary> =
            self.lib.notes_in_folder_sorted(folder).into_iter().map(summarize).collect();
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
    }

    /// 单篇笔记详情，JSON；不存在返回 "null"。
    pub fn note(&self, id: &str) -> String {
        match self.lib.note(id) {
            Some(n) => serde_json::to_string(&detail_of(n)).unwrap_or_else(|_| "null".into()),
            None => "null".into(),
        }
    }

    /// 标题/正文全文搜索，JSON。
    pub fn search(&self, q: &str) -> String {
        let list: Vec<NoteSummary> =
            self.lib.search(q).into_iter().map(summarize).collect();
        serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
    }
}

impl Demo {
    fn tree(&self, parent: &str) -> Vec<FolderNode> {
        self.lib
            .child_folder_ids_sorted(parent)
            .into_iter()
            .filter_map(|id| {
                self.lib.folders.get(&id).map(|f| FolderNode {
                    id: f.id.clone(),
                    title: f.title.clone(),
                    note_count: self.lib.note_count(&f.id),
                    children: self.tree(&f.id),
                })
            })
            .collect()
    }
}
