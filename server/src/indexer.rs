//! 从存储后端构建内存索引：负责拉取（rayon 并行）与增量缓存协调，
//! 拿到原始内容后交给 `jasper_core::library::Library::from_contents` 做纯解析/索引。
//!
//! 之所以放在 server 而非 core：这里依赖 storage(IO)、cache(SQLite)、rayon(线程)，
//! 都是不可移植到 WASM 的部分；core 只保留纯计算。

use crate::cache::CacheStore;
use crate::library::{BuildStats, Library};
use crate::storage::StorageBackend;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// 全量构建（不走缓存）：列条目 → 并行拉取 → 解析建索引。
#[allow(dead_code)] // 保留作回退/测试入口；当前路径都走 build_cached
pub fn build(storage: &dyn StorageBackend) -> anyhow::Result<(Library, BuildStats)> {
    let item_stats = storage.list_items()?;
    let contents: Vec<String> = item_stats
        .par_iter()
        .filter_map(|s| storage.get_item(&s.name).ok())
        .collect();
    let fetched = contents.len();
    let (lib, mut stats) = Library::from_contents(contents);
    stats.fetched = fetched;
    stats.errors += item_stats.len() - fetched; // 拉取失败计入错误
    Ok((lib, stats))
}

/// 增量构建：复用 SQLite 缓存，只拉取新增/变化（mtime 不同）的条目。
/// `source` 为数据源标识（见 config::source_key），用于隔离不同数据源的缓存。
pub fn build_cached(
    storage: &dyn StorageBackend,
    cache: &CacheStore,
    source: &str,
) -> anyhow::Result<(Library, BuildStats)> {
    let listed = storage.list_items()?;
    let cached = cache.load(source).unwrap_or_default();
    let listed_names: HashSet<&str> = listed.iter().map(|s| s.name.as_str()).collect();

    // 划分：命中缓存（mtime 相同且非 0）直接复用，其余待拉取。
    let mut reuse_contents: Vec<String> = Vec::new();
    let mut to_fetch: Vec<&str> = Vec::new();
    for s in &listed {
        match cached.get(&s.name) {
            Some(c) if s.updated_time != 0 && c.updated_time == s.updated_time => {
                reuse_contents.push(c.content.clone())
            }
            _ => to_fetch.push(&s.name),
        }
    }
    let cached_count = reuse_contents.len();

    // 只并行拉取变化的条目。返回 (name, updated_time, content) 以便写回缓存。
    let mtime_of: HashMap<&str, i64> =
        listed.iter().map(|s| (s.name.as_str(), s.updated_time)).collect();
    let fetched: Vec<(String, i64, String)> = to_fetch
        .par_iter()
        .filter_map(|name| {
            storage
                .get_item(name)
                .ok()
                .map(|content| (name.to_string(), mtime_of[name], content))
        })
        .collect();

    // 组装：复用 + 新拉取的内容一起解析建索引。
    let mut all_contents = reuse_contents;
    all_contents.extend(fetched.iter().map(|(_, _, c)| c.clone()));
    let (lib, mut stats) = Library::from_contents(all_contents);
    stats.cached = cached_count;
    stats.fetched = fetched.len();
    stats.errors += to_fetch.len() - fetched.len(); // 拉取失败计入错误

    // 持久化缓存：写入变化项，清理数据源中已删除的条目。
    let removed: Vec<String> = cached
        .keys()
        .filter(|k| !listed_names.contains(k.as_str()))
        .cloned()
        .collect();
    if let Err(e) = cache.sync(source, &fetched, &removed) {
        eprintln!("缓存写入失败（不影响本次运行）: {e}");
    }
    Ok((lib, stats))
}
