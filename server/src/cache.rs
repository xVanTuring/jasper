//! 增量缓存：持久化每个条目的原始 .md 内容 + 修改时间到本地 SQLite。
//!
//! 目的：启动/切换数据源时，只重新拉取 `list_items()` 里 **新增或 mtime 变化** 的条目，
//! 未变化的直接复用缓存内容（避免 WebDAV 场景下逐个 GET 几百个文件）。
//! 缓存按数据源（config::source_key）隔离；只缓存 `<id>.md` 文本，资源二进制不入此缓存。
//!
//! 缓存陈旧无害：判定依据是 `list_items()` 返回的实时 mtime，任何写入都会刷新 mtime，
//! 下次启动即视为变化重新拉取；缓存损坏/删除最坏退化为一次全量拉取。

use crate::config::config_base_dir;
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Mutex;

/// 缓存中的一条记录。
pub struct CachedItem {
    pub updated_time: i64,
    pub content: String,
}

pub struct CacheStore {
    conn: Mutex<Connection>,
}

impl CacheStore {
    pub fn open() -> Result<Self> {
        let path = config_base_dir()?.join("cache.db");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(&path).with_context(|| format!("打开缓存库失败 {path:?}"))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS items (
                source       TEXT NOT NULL,
                name         TEXT NOT NULL,
                updated_time INTEGER NOT NULL,
                content      TEXT NOT NULL,
                PRIMARY KEY (source, name)
            )",
            [],
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// 内存态缓存（无磁盘库时使用：始终未命中，相当于禁用缓存）。
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS items (
                source TEXT NOT NULL, name TEXT NOT NULL,
                updated_time INTEGER NOT NULL, content TEXT NOT NULL,
                PRIMARY KEY (source, name))",
            [],
        )?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// 读取某数据源的全部缓存条目：name -> {updated_time, content}。
    pub fn load(&self, source: &str) -> Result<HashMap<String, CachedItem>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT name, updated_time, content FROM items WHERE source=?1")?;
        let rows = stmt.query_map([source], |r| {
            Ok((
                r.get::<_, String>(0)?,
                CachedItem { updated_time: r.get(1)?, content: r.get(2)? },
            ))
        })?;
        let mut out = HashMap::new();
        for row in rows {
            let (name, item) = row?;
            out.insert(name, item);
        }
        Ok(out)
    }

    /// 同步缓存到最新一轮的拉取结果：
    /// - `upserts`：新增/变化的条目（name, updated_time, content），整体 INSERT OR REPLACE；
    /// - `removed`：数据源中已不存在的条目名，删除其缓存行。
    /// 在单个事务内完成。
    pub fn sync(&self, source: &str, upserts: &[(String, i64, String)], removed: &[String]) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        {
            let mut up = tx.prepare(
                "INSERT INTO items(source,name,updated_time,content) VALUES(?1,?2,?3,?4)
                 ON CONFLICT(source,name) DO UPDATE SET updated_time=?3, content=?4",
            )?;
            for (name, updated_time, content) in upserts {
                up.execute(rusqlite::params![source, name, updated_time, content])?;
            }
            let mut del = tx.prepare("DELETE FROM items WHERE source=?1 AND name=?2")?;
            for name in removed {
                del.execute(rusqlite::params![source, name])?;
            }
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_reuse_and_remove() {
        let cache = CacheStore::in_memory().unwrap();
        let src = "local:/tmp/x";

        // 初次：写入两条
        cache
            .sync(
                src,
                &[
                    ("a.md".into(), 100, "AAA".into()),
                    ("b.md".into(), 200, "BBB".into()),
                ],
                &[],
            )
            .unwrap();

        let loaded = cache.load(src).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded["a.md"].updated_time, 100);
        assert_eq!(loaded["b.md"].content, "BBB");

        // 第二轮：a 变化、删除 b
        cache
            .sync(src, &[("a.md".into(), 150, "AAA2".into())], &["b.md".into()])
            .unwrap();
        let loaded = cache.load(src).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["a.md"].updated_time, 150);
        assert_eq!(loaded["a.md"].content, "AAA2");
        assert!(!loaded.contains_key("b.md"));

        // 数据源隔离
        assert!(cache.load("webdav:other").unwrap().is_empty());
    }
}
