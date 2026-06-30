//! 内存索引层：把一批条目原始内容解析后组织成可查询的库（纯计算，无 IO）。
//!
//! 只读、构建一次即不可变。从存储拉取 + 增量缓存的协调放在 server 的 `indexer`，
//! 它拿到原始内容后调用 `Library::from_contents`。本模块不引入线程/IO，可编译到 WASM。

use crate::model::*;
use crate::parser;
use std::collections::{HashMap, HashSet};

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

    /// 笔记的原始 .md 内容（用于保存时保留元数据，避免重新拉取）
    raw_notes: HashMap<String, String>,
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
    /// 命中增量缓存、免去拉取的条目数。
    pub cached: usize,
    /// 本轮实际从数据源拉取的条目数。
    pub fetched: usize,
}

impl Library {
    /// 把一批原始 .md 内容解析、分类、建索引为 Library，返回类型计数 + 解析错误数。
    /// 拉取（并行/缓存）由调用方负责；这里只做纯解析与索引，便于编译到 WASM。
    pub fn from_contents(contents: Vec<String>) -> (Library, BuildStats) {
        let mut lib = Library::default();
        let mut stats = BuildStats::default();

        // 顺序解析（核心库不引入线程）；保留笔记原始内容用于写回。
        let parsed: Vec<Option<(String, RawItem)>> = contents
            .into_iter()
            .map(|content| parser::parse_item(&content).ok().map(|raw| (content, raw)))
            .collect();

        // 分类，构建索引
        for item in parsed {
            let (content, raw) = match item {
                Some(x) => x,
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
                        lib.raw_notes.insert(n.id.clone(), content);
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
        (lib, stats)
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

    /// 笔记的原始 .md 内容（保存时用于保留元数据）。
    pub fn note_raw(&self, id: &str) -> Option<&str> {
        self.raw_notes.get(id).map(|s| s.as_str())
    }

    /// 新增或更新一篇笔记（写回成功后同步内存）。返回笔记 id。
    pub fn upsert_note(&mut self, content: &str) -> anyhow::Result<String> {
        let raw = parser::parse_item(content)?;
        let note = parser::to_note(&raw)?;
        let id = note.id.clone();
        self.raw_notes.insert(id.clone(), content.to_string());
        self.notes.insert(id.clone(), note);
        self.build_indexes();
        Ok(id)
    }

    /// 新增或更新一个笔记本（写回成功后同步内存）。返回笔记本 id。
    pub fn upsert_folder(&mut self, content: &str) -> anyhow::Result<String> {
        let raw = parser::parse_item(content)?;
        let folder = parser::to_folder(&raw)?;
        let id = folder.id.clone();
        self.folders.insert(id.clone(), folder);
        self.build_indexes();
        Ok(id)
    }

    /// `candidate` 是否就是 `root` 本身、或位于 `root` 子树之下。
    /// 用于移动笔记本时防止把笔记本移进它自己或其后代（成环）。
    pub fn is_self_or_descendant(&self, root: &str, candidate: &str) -> bool {
        let mut cur = candidate.to_string();
        // 上限步数防御坏数据里的既有环，避免死循环
        for _ in 0..=self.folders.len() {
            if cur == root {
                return true;
            }
            match self.folders.get(&cur) {
                Some(f) if !f.parent_id.is_empty() => cur = f.parent_id.clone(),
                _ => return false,
            }
        }
        false
    }

    /// 新增或更新一个资源（上传成功后同步内存，使 /api/resources 能拿到 mime）。返回资源 id。
    pub fn upsert_resource(&mut self, content: &str) -> anyhow::Result<String> {
        let raw = parser::parse_item(content)?;
        let r = parser::to_resource(&raw)?;
        let id = r.id.clone();
        self.resources.insert(id.clone(), r);
        Ok(id)
    }

    /// 删除一篇笔记（写回成功后同步内存）。
    pub fn remove_note(&mut self, id: &str) {
        self.notes.remove(id);
        self.raw_notes.remove(id);
        self.build_indexes();
    }

    /// 删除一个资源（写回成功后同步内存）。
    pub fn remove_resource(&mut self, id: &str) {
        self.resources.remove(id);
    }

    /// 每个资源被多少篇笔记引用（扫描所有笔记正文里的 `:/<id>`）。
    /// 未出现在结果中的资源即为无人引用的“孤儿”。
    pub fn resource_usage(&self) -> HashMap<String, usize> {
        let mut usage: HashMap<String, usize> = HashMap::new();
        for n in self.notes.values() {
            for id in scan_resource_refs(&n.body) {
                *usage.entry(id).or_insert(0) += 1;
            }
        }
        usage
    }
}

/// 统计 markdown 任务清单（GFM checkbox）的完成/总数：`(已完成, 总数)`。
/// 仅认行首（去缩进后）形如 `- [ ] ` / `* [x] ` / `+ [X] ` 的列表项。
pub fn count_tasks(body: &str) -> (usize, usize) {
    let mut done = 0;
    let mut total = 0;
    for line in body.lines() {
        let b = line.trim_start().as_bytes();
        // `<-|*|+>` SP `[` <mark> `]`  且其后是空白或行尾
        if b.len() >= 5
            && matches!(b[0], b'-' | b'*' | b'+')
            && b[1] == b' '
            && b[2] == b'['
            && b[4] == b']'
        {
            let after_ok = b.len() == 5 || b[5] == b' ' || b[5] == b'\t';
            match (after_ok, b[3]) {
                (true, b' ') => total += 1,
                (true, b'x') | (true, b'X') => {
                    total += 1;
                    done += 1;
                }
                _ => {}
            }
        }
    }
    (done, total)
}

/// 扫描文本里的 Joplin 资源引用 `:/<32hex>`，返回去重后的 id 集合（每篇笔记内同一资源只计一次）。
fn scan_resource_refs(body: &str) -> HashSet<String> {
    let b = body.as_bytes();
    let mut out = HashSet::new();
    let mut i = 0;
    while i + 34 <= b.len() {
        // `:/` 后接恰好 32 个十六进制，且第 33 位不再是十六进制（id 长度精确为 32）
        if b[i] == b':' && b[i + 1] == b'/' {
            let hex = &b[i + 2..i + 34];
            let bounded = i + 34 >= b.len() || !b[i + 34].is_ascii_hexdigit();
            if bounded && hex.iter().all(|c| c.is_ascii_hexdigit()) {
                out.insert(String::from_utf8_lossy(hex).to_lowercase());
                i += 34;
                continue;
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_resource_refs() {
        let body = "见图 ![a](:/0123456789abcdef0123456789abcdef) 和附件 [f](:/0123456789ABCDEF0123456789ABCDEF)\n\
            重复同一个 :/0123456789abcdef0123456789abcdef 再来个 :/deadbeefdeadbeefdeadbeefdeadbeef";
        let refs = scan_resource_refs(body);
        // 大小写归一后，第一个引用去重为 1 个，另有 deadbeef 一个 → 共 2 个不同 id
        assert_eq!(refs.len(), 2);
        assert!(refs.contains("0123456789abcdef0123456789abcdef"));
        assert!(refs.contains("deadbeefdeadbeefdeadbeefdeadbeef"));
    }

    #[test]
    fn counts_task_list() {
        let body = "标题\n- [ ] 待办一\n- [x] 已完成\n* [X] 也完成\n+ [ ] 第四\n普通行\n-[ ] 无空格不算\n- [] 非法不算";
        assert_eq!(count_tasks(body), (2, 4));
        assert_eq!(count_tasks("没有任何任务"), (0, 0));
        assert_eq!(count_tasks("  - [x] 缩进也算"), (1, 1));
    }

    #[test]
    fn ignores_non_32hex() {
        // 太短 / 太长 / 非 hex 都不算
        let refs = scan_resource_refs(":/short :/0123456789abcdef0123456789abcdefEXTRA :/zzzz");
        assert!(refs.is_empty());
    }
}
