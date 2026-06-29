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

    fn put_resource(&self, resource_id: &str, bytes: &[u8]) -> Result<()> {
        let dir = self.root.join(".resource");
        std::fs::create_dir_all(&dir).with_context(|| format!("创建资源目录失败 {:?}", dir))?;
        // 写临时文件后原子 rename，避免写一半损坏
        let path = dir.join(resource_id);
        let tmp = dir.join(format!(".{resource_id}.tmp"));
        std::fs::write(&tmp, bytes).with_context(|| format!("写入临时资源失败 {:?}", tmp))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("替换资源失败 {:?}", path))?;
        Ok(())
    }

    fn delete_resource(&self, resource_id: &str) -> Result<()> {
        let path = self.root.join(".resource").join(resource_id);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("删除资源失败 {:?}", path)),
        }
    }

    fn put_item(&self, name: &str, content: &str) -> Result<()> {
        // 写临时文件后原子 rename，避免写一半导致文件损坏
        let path = self.root.join(name);
        let tmp = self.root.join(format!(".{name}.tmp"));
        std::fs::write(&tmp, content).with_context(|| format!("写入临时文件失败 {:?}", tmp))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("替换条目失败 {:?}", path))?;
        Ok(())
    }

    fn delete_item(&self, name: &str) -> Result<()> {
        let path = self.root.join(name);
        std::fs::remove_file(&path).with_context(|| format!("删除条目失败 {:?}", path))
    }

    fn init_new(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root).with_context(|| format!("创建目录失败 {:?}", self.root))?;
        std::fs::create_dir_all(self.root.join(".resource")).ok();
        self.put_item("info.json", super::DEFAULT_INFO_JSON)?;
        Ok(())
    }
}
