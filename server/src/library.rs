//! 内存索引层：把存储后端里的全部条目解析后组织成可查询的库。
//!
//! 只读、构建一次即不可变；后续刷新通过重建+原子替换实现（见 AppState）。
//! 阶段2 接入 WebDAV 时，可在此之上加 SQLite 持久化缓存。

use crate::model::*;
use crate::parser;
use crate::storage::StorageBackend;
use rayon::prelude::*;
use std::collections::HashMap;

#[derive(Default)]
pub struct Library {
    pub folders: HashMap<String, Folder>,
    pub notes: HashMap<String, Note>,
    pub resources: HashMap<String, Resource>,
    pub tags: HashMap<String, Tag>,
    pub note_tags: Vec<NoteTag>,

    /// folder_id -> 该笔记本下的笔记 id（直属，不含子笔记本）
    notes_by_folder: HashMap<String, Vec<String>>,
    /// parent folder_id -> 子笔记本 id（根用空串 ""）
    child_folders: HashMap<String, Vec<String>>,
}

#[derive(Debug, Default, Clone)]
pub struct BuildStats {
    pub notes: usize,
    pub folders: usize,
    pub resources: usize,
    pub tags: usize,
    pub note_tags: usize,
    pub others: usize,
    pub encrypted: usize,
    pub errors: usize,
}

impl Library {
    /// 从存储后端全量构建。
    pub fn build(storage: &dyn StorageBackend) -> anyhow::Result<(Library, BuildStats)> {
        let mut lib = Library::default();
        let mut stats = BuildStats::default();

        // 并行拉取 + 解析（WebDAV 场景下数百个文件串行下载太慢）。
        let item_stats = storage.list_items()?;
        let parsed: Vec<Option<RawItem>> = item_stats
            .par_iter()
            .map(|s| {
                storage
                    .get_item(&s.name)
                    .ok()
                    .and_then(|content| parser::parse_item(&content).ok())
            })
            .collect();

        // 分类（顺序执行，构建索引）
        for item in parsed {
            let raw = match item {
                Some(r) => r,
                None => {
                    stats.errors += 1;
                    continue;
                }
            };
            if raw.is_encrypted() {
                stats.encrypted += 1;
                continue;
            }
            match raw.item_type() {
                ItemType::Note => match parser::to_note(&raw) {
                    Ok(n) => {
                        lib.notes.insert(n.id.clone(), n);
                        stats.notes += 1;
                    }
                    Err(_) => stats.errors += 1,
                },
                ItemType::Folder => match parser::to_folder(&raw) {
                    Ok(f) => {
                        lib.folders.insert(f.id.clone(), f);
                        stats.folders += 1;
                    }
                    Err(_) => stats.errors += 1,
                },
                ItemType::Resource => match parser::to_resource(&raw) {
                    Ok(r) => {
                        lib.resources.insert(r.id.clone(), r);
                        stats.resources += 1;
                    }
                    Err(_) => stats.errors += 1,
                },
                ItemType::Tag => match parser::to_tag(&raw) {
                    Ok(t) => {
                        lib.tags.insert(t.id.clone(), t);
                        stats.tags += 1;
                    }
                    Err(_) => stats.errors += 1,
                },
                ItemType::NoteTag => match parser::to_note_tag(&raw) {
                    Ok(nt) => {
                        lib.note_tags.push(nt);
                        stats.note_tags += 1;
                    }
                    Err(_) => stats.errors += 1,
                },
                ItemType::Other => stats.others += 1,
            }
        }

        lib.build_indexes();
        Ok((lib, stats))
    }

    fn build_indexes(&mut self) {
        let mut notes_by_folder: HashMap<String, Vec<String>> = HashMap::new();
        for n in self.notes.values() {
            notes_by_folder
                .entry(n.parent_id.clone())
                .or_default()
                .push(n.id.clone());
        }
        let mut child_folders: HashMap<String, Vec<String>> = HashMap::new();
        for f in self.folders.values() {
            child_folders
                .entry(f.parent_id.clone())
                .or_default()
                .push(f.id.clone());
        }
        self.notes_by_folder = notes_by_folder;
        self.child_folders = child_folders;
    }

    /// 某笔记本（含根 ""）直属笔记数。
    pub fn note_count(&self, folder_id: &str) -> usize {
        self.notes_by_folder.get(folder_id).map(|v| v.len()).unwrap_or(0)
    }

    /// 子笔记本，按标题排序。
    pub fn child_folder_ids_sorted(&self, parent_id: &str) -> Vec<String> {
        let mut ids = self.child_folders.get(parent_id).cloned().unwrap_or_default();
        ids.sort_by(|a, b| {
            let ta = self.folders.get(a).map(|f| f.title.as_str()).unwrap_or("");
            let tb = self.folders.get(b).map(|f| f.title.as_str()).unwrap_or("");
            ta.cmp(tb)
        });
        ids
    }

    /// 某笔记本下的笔记，按更新时间倒序。
    pub fn notes_in_folder_sorted(&self, folder_id: &str) -> Vec<&Note> {
        let mut notes: Vec<&Note> = self
            .notes_by_folder
            .get(folder_id)
            .map(|ids| ids.iter().filter_map(|id| self.notes.get(id)).collect())
            .unwrap_or_default();
        notes.sort_by(|a, b| b.updated_time.cmp(&a.updated_time));
        notes
    }

    pub fn note(&self, id: &str) -> Option<&Note> {
        self.notes.get(id)
    }

    pub fn resource(&self, id: &str) -> Option<&Resource> {
        self.resources.get(id)
    }

    /// 简单全文搜索：标题/正文不区分大小写包含。按更新时间倒序，限制 200 条。
    pub fn search(&self, query: &str) -> Vec<&Note> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return vec![];
        }
        let mut hits: Vec<&Note> = self
            .notes
            .values()
            .filter(|n| {
                n.title.to_lowercase().contains(&q) || n.body.to_lowercase().contains(&q)
            })
            .collect();
        hits.sort_by(|a, b| b.updated_time.cmp(&a.updated_time));
        hits.truncate(200);
        hits
    }
}
