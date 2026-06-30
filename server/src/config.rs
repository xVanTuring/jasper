//! 数据源配置：持久化到本地 SQLite（key-value），并据此构造存储后端。
//!
//! 配置库位置：平台配置目录下 `jasper/config.db`
//! （macOS: ~/Library/Application Support/jasper/config.db）。

use crate::storage::local::LocalStorage;
use crate::storage::webdav::WebDavStorage;
use crate::storage::StorageBackend;
use anyhow::{anyhow, Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct SourceConfig {
    /// "local" | "webdav"
    pub source_type: String,
    #[serde(default)]
    pub local_path: String,
    #[serde(default)]
    pub webdav_url: String,
    #[serde(default)]
    pub webdav_user: String,
    #[serde(default)]
    pub webdav_pass: String,
}

pub struct ConfigStore {
    conn: Connection,
}

impl ConfigStore {
    pub fn open() -> Result<Self> {
        let path = config_db_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(&path).with_context(|| format!("打开配置库失败 {path:?}"))?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )?;
        Ok(Self { conn })
    }

    pub fn load(&self) -> Option<SourceConfig> {
        let get = |k: &str| -> Option<String> {
            self.conn
                .query_row("SELECT value FROM settings WHERE key=?1", [k], |r| r.get(0))
                .ok()
        };
        let source_type = get("source_type")?;
        if source_type.is_empty() {
            return None;
        }
        Some(SourceConfig {
            source_type,
            local_path: get("local_path").unwrap_or_default(),
            webdav_url: get("webdav_url").unwrap_or_default(),
            webdav_user: get("webdav_user").unwrap_or_default(),
            webdav_pass: get("webdav_pass").unwrap_or_default(),
        })
    }

    pub fn save(&self, cfg: &SourceConfig) -> Result<()> {
        let set = |k: &str, v: &str| -> Result<()> {
            self.conn.execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=?2",
                rusqlite::params![k, v],
            )?;
            Ok(())
        };
        set("source_type", &cfg.source_type)?;
        set("local_path", &cfg.local_path)?;
        set("webdav_url", &cfg.webdav_url)?;
        set("webdav_user", &cfg.webdav_user)?;
        set("webdav_pass", &cfg.webdav_pass)?;
        Ok(())
    }
}

/// 根据配置构造存储后端。
pub fn build_storage(cfg: &SourceConfig) -> Result<Arc<dyn StorageBackend>> {
    match cfg.source_type.as_str() {
        "local" => {
            if cfg.local_path.trim().is_empty() {
                return Err(anyhow!("本地路径为空"));
            }
            Ok(Arc::new(LocalStorage::new(cfg.local_path.trim())))
        }
        "webdav" => {
            if cfg.webdav_url.trim().is_empty() {
                return Err(anyhow!("WebDAV 地址为空"));
            }
            Ok(Arc::new(WebDavStorage::new(
                cfg.webdav_url.trim(),
                (!cfg.webdav_user.is_empty()).then_some(cfg.webdav_user.as_str()),
                (!cfg.webdav_pass.is_empty()).then_some(cfg.webdav_pass.as_str()),
            )))
        }
        other => Err(anyhow!("未知数据源类型: {other}")),
    }
}

/// 配置/缓存库所在目录（config.db、cache.db 同处）。
/// 允许通过环境变量覆盖（便于测试 / 便携部署 / 容器挂卷）。
pub fn config_base_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("JASPER_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let base = dirs::config_dir().context("无法定位配置目录")?;
    Ok(base.join("jasper"))
}

fn config_db_path() -> Result<PathBuf> {
    Ok(config_base_dir()?.join("config.db"))
}

/// 数据源的稳定标识，用于隔离不同数据源的增量缓存（不含密码）。
pub fn source_key(cfg: &SourceConfig) -> String {
    match cfg.source_type.as_str() {
        "local" => format!("local:{}", cfg.local_path.trim()),
        "webdav" => format!("webdav:{}|{}", cfg.webdav_url.trim(), cfg.webdav_user),
        other => format!("{other}:"),
    }
}
