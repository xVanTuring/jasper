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

    /// 写入资源二进制到 `.resource/<id>`（不含扩展名）。`.resource/` 不存在时应先创建。
    fn put_resource(&self, resource_id: &str, bytes: &[u8]) -> Result<()>;

    /// 删除资源二进制 `.resource/<id>`；已不存在视作成功（幂等）。
    fn delete_resource(&self, resource_id: &str) -> Result<()>;

    /// 写入（新建或覆盖）一个条目文件。`name` 形如 `<32hex>.md`。
    fn put_item(&self, name: &str, content: &str) -> Result<()>;

    /// 删除一个条目文件。
    fn delete_item(&self, name: &str) -> Result<()>;

    /// 初始化一个全新的笔记库（创建根目录与 .resource、写入默认 info.json）。
    /// 仅在"新建库"时调用；"使用现有库"不应调用，以免覆盖已有 info.json。
    fn init_new(&self) -> Result<()>;
}

/// 全新笔记库的默认 info.json（Joplin 同步格式版本 3，未加密）。
pub const DEFAULT_INFO_JSON: &str = r#"{"version":3,"e2ee":{"value":false,"updatedTime":0},"activeMasterKeyId":{"value":"","updatedTime":0},"masterKeys":[],"ppk":{"value":null,"updatedTime":0},"appMinVersion":"3.0.0"}"#;

/// 判断文件名是否为合法条目文件：32 位十六进制 + `.md`。
/// 来源：joplin/packages/lib/models/BaseItem.ts:174-183 (isSystemPath)
pub fn is_item_filename(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 35
        && name.ends_with(".md")
        && name[..32].chars().all(|c| c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::is_item_filename;

    #[test]
    fn accepts_only_32hex_md() {
        assert!(is_item_filename("0123456789abcdef0123456789abcdef.md"));
        assert!(is_item_filename("0123456789ABCDEF0123456789ABCDEF.md")); // 大写 hex 也算
    }

    #[test]
    fn rejects_wrong_shape() {
        assert!(!is_item_filename("info.json"));
        assert!(!is_item_filename("0123456789abcdef.md")); // 太短
        assert!(!is_item_filename("0123456789abcdef0123456789abcdef.txt")); // 扩展名不对
        assert!(!is_item_filename("zz23456789abcdef0123456789abcdef.md")); // 非 hex
        assert!(!is_item_filename("0123456789abcdef0123456789abcdef")); // 无 .md
    }
}
