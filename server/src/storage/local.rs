//! 本地文件夹存储后端。
//! 对应 Joplin 的「文件系统」同步目标（file-api-driver-local.ts）。

use super::{is_item_filename, ItemStat, StorageBackend};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

fn mtime_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

impl StorageBackend for LocalStorage {
    fn list_items(&self) -> Result<Vec<ItemStat>> {
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&self.root)
            .with_context(|| format!("无法读取数据目录 {:?}", self.root))?
        {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !is_item_filename(&name) {
                continue;
            }
            out.push(ItemStat {
                updated_time: mtime_ms(&entry.path()),
                name,
            });
        }
        Ok(out)
    }

    fn get_item(&self, name: &str) -> Result<String> {
        let path = self.root.join(name);
        std::fs::read_to_string(&path).with_context(|| format!("读取条目失败 {:?}", path))
    }

    fn get_resource(&self, resource_id: &str) -> Result<Vec<u8>> {
        let path = self.root.join(".resource").join(resource_id);
        std::fs::read(&path).with_context(|| format!("读取资源失败 {:?}", path))
    }
}
