//! 存储后端抽象：屏蔽「本地文件夹」与「WebDAV」的差异。
//! 只读客户端只需要三种能力：列条目、取条目文本、取资源二进制。

pub mod local;
pub mod webdav;

use anyhow::Result;

/// 一个条目文件的元信息（用于增量同步比对）。
#[derive(Debug, Clone)]
pub struct ItemStat {
    /// 文件名，如 `<32hex>.md`
    pub name: String,
    /// 修改时间（Unix 毫秒），来自文件 mtime 或 WebDAV getlastmodified
    pub updated_time: i64,
}

pub trait StorageBackend: Send + Sync {
    /// 列出同步根目录下所有条目文件（仅 `<32hex>.md`）。
    fn list_items(&self) -> Result<Vec<ItemStat>>;

    /// 读取一个条目文件的文本内容。`name` 形如 `<32hex>.md`。
    fn get_item(&self, name: &str) -> Result<String>;

    /// 读取资源二进制。`resource_id` 形如 `<32hex>`（对应 `.resource/<id>`）。
    fn get_resource(&self, resource_id: &str) -> Result<Vec<u8>>;
}

/// 判断文件名是否为合法条目文件：32 位十六进制 + `.md`。
/// 来源：joplin/packages/lib/models/BaseItem.ts:174-183 (isSystemPath)
pub fn is_item_filename(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 35
        && name.ends_with(".md")
        && name[..32].chars().all(|c| c.is_ascii_hexdigit())
}
