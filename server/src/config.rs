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
    /// "local" | "webdav" | "plugin"（插件存储 provider，spec 0.2）
    pub source_type: String,
    #[serde(default)]
    pub local_path: String,
    #[serde(default)]
    pub webdav_url: String,
    #[serde(default)]
    pub webdav_user: String,
    #[serde(default)]
    pub webdav_pass: String,
    /// source_type=="plugin" 时：提供 provider 的插件 id。
    #[serde(default)]
    pub plugin_id: String,
    /// source_type=="plugin" 时：插件内的存储贡献 id（[[contributes.storage]].id）。
    #[serde(default)]
    pub plugin_storage: String,
    /// source_type=="plugin" 时：数据源配置（JSON 对象文本，可含 secret，明文——与 webdav_pass 同姿势）。
    #[serde(default)]
    pub plugin_config: String,
    /// apply 时按 config_schema 剔除 secret 字段后的规范化 JSON（键排序）。
    /// 落库持久化，让 source_key() 保持纯函数（无需 manifest 即可算缓存键）。
    #[serde(default)]
    pub plugin_config_key: String,
    /// 只读模式：开启后拒绝一切写操作（应用级开关，随配置持久化）。
    #[serde(default)]
    pub read_only: bool,
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
        Self::init(conn)
    }

    /// 内存库（测试用，不落盘、彼此隔离）。
    #[cfg(test)]
    pub fn in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
            [],
        )?;
        // 插件状态与插件作用域 KV（plugins feature 的路由使用；表无条件建好，成本可忽略）
        conn.execute(
            "CREATE TABLE IF NOT EXISTS plugin_state (
                id TEXT PRIMARY KEY,
                enabled INTEGER NOT NULL,
                granted_caps TEXT NOT NULL DEFAULT ''
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS plugin_settings (
                plugin_id TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                PRIMARY KEY (plugin_id, key)
            )",
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
            plugin_id: get("plugin_id").unwrap_or_default(),
            plugin_storage: get("plugin_storage").unwrap_or_default(),
            plugin_config: get("plugin_config").unwrap_or_default(),
            plugin_config_key: get("plugin_config_key").unwrap_or_default(),
            read_only: get("read_only").map(|v| v == "true" || v == "1").unwrap_or(false),
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
        set("plugin_id", &cfg.plugin_id)?;
        set("plugin_storage", &cfg.plugin_storage)?;
        set("plugin_config", &cfg.plugin_config)?;
        set("plugin_config_key", &cfg.plugin_config_key)?;
        set("read_only", if cfg.read_only { "true" } else { "false" })?;
        Ok(())
    }

    // ---------- 插件状态 / 插件设置（spec §5 / §10）----------

    /// 读插件持久化状态：`(enabled, 已授权能力)`。无记录返回 None。
    pub fn plugin_state(&self, id: &str) -> Option<(bool, Vec<String>)> {
        self.conn
            .query_row(
                "SELECT enabled, granted_caps FROM plugin_state WHERE id=?1",
                [id],
                |r| Ok((r.get::<_, i64>(0)? != 0, r.get::<_, String>(1)?)),
            )
            .ok()
            .map(|(enabled, caps)| {
                let caps = caps.split(',').filter(|s| !s.is_empty()).map(String::from).collect();
                (enabled, caps)
            })
    }

    pub fn set_plugin_state(&self, id: &str, enabled: bool, granted_caps: &[String]) -> Result<()> {
        self.conn.execute(
            "INSERT INTO plugin_state(id,enabled,granted_caps) VALUES(?1,?2,?3)
             ON CONFLICT(id) DO UPDATE SET enabled=?2, granted_caps=?3",
            rusqlite::params![id, enabled as i64, granted_caps.join(",")],
        )?;
        Ok(())
    }

    /// 删除插件的状态与全部设置（卸载时用）。
    pub fn remove_plugin(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM plugin_state WHERE id=?1", [id])?;
        self.conn.execute("DELETE FROM plugin_settings WHERE plugin_id=?1", [id])?;
        Ok(())
    }

    /// 读插件作用域全部设置（value 为 JSON 文本）。
    pub fn plugin_settings(&self, plugin_id: &str) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let Ok(mut stmt) = self.conn.prepare("SELECT key, value FROM plugin_settings WHERE plugin_id=?1")
        else {
            return out;
        };
        let rows = stmt.query_map([plugin_id], |r| Ok((r.get(0)?, r.get(1)?)));
        if let Ok(rows) = rows {
            out.extend(rows.flatten());
        }
        out
    }

    /// 写/删单个插件设置：`value_json = None` 删除该键。
    pub fn set_plugin_setting(&self, plugin_id: &str, key: &str, value_json: Option<&str>) -> Result<()> {
        match value_json {
            Some(v) => {
                self.conn.execute(
                    "INSERT INTO plugin_settings(plugin_id,key,value) VALUES(?1,?2,?3)
                     ON CONFLICT(plugin_id,key) DO UPDATE SET value=?3",
                    rusqlite::params![plugin_id, key, v],
                )?;
            }
            None => {
                self.conn.execute(
                    "DELETE FROM plugin_settings WHERE plugin_id=?1 AND key=?2",
                    rusqlite::params![plugin_id, key],
                )?;
            }
        }
        Ok(())
    }
}

/// 根据配置构造存储后端。`plugins`：插件宿主（"plugin" 数据源用；feature 关/未初始化传 None 即报错）。
pub fn build_storage(
    cfg: &SourceConfig,
    plugins: Option<&Arc<crate::plugins::PluginHost>>,
) -> Result<Arc<dyn StorageBackend>> {
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
        "plugin" => crate::plugins::build_plugin_storage(cfg, plugins),
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

/// 数据源的稳定标识，用于隔离不同数据源的增量缓存（不含密码/secret）。
pub fn source_key(cfg: &SourceConfig) -> String {
    match cfg.source_type.as_str() {
        "local" => format!("local:{}", cfg.local_path.trim()),
        "webdav" => format!("webdav:{}|{}", cfg.webdav_url.trim(), cfg.webdav_user),
        // plugin_config_key = apply 时按 schema 剔除 secret 后的规范化 JSON（见 plugins::prepare_plugin_source）
        "plugin" => format!("plugin:{}/{}|{}", cfg.plugin_id, cfg.plugin_storage, cfg.plugin_config_key),
        other => format!("{other}:"),
    }
}

#[cfg(test)]
mod tests {
    use super::{source_key, ConfigStore, SourceConfig};

    #[test]
    fn read_only_round_trips() {
        let store = ConfigStore::in_memory().unwrap();
        let base = SourceConfig {
            source_type: "local".into(),
            local_path: "/notes".into(),
            ..Default::default()
        };
        // 默认（未设）为 false
        assert!(!base.read_only);
        // 保存 true → 读回 true
        store.save(&SourceConfig { read_only: true, ..base.clone() }).unwrap();
        assert!(store.load().unwrap().read_only);
        // 覆盖为 false → 读回 false
        store.save(&SourceConfig { read_only: false, ..base.clone() }).unwrap();
        assert!(!store.load().unwrap().read_only);
    }

    #[test]
    fn source_key_local_trims_path() {
        let cfg = SourceConfig {
            source_type: "local".into(),
            local_path: "  /data/notes  ".into(),
            ..Default::default()
        };
        assert_eq!(source_key(&cfg), "local:/data/notes");
    }

    #[test]
    fn source_key_webdav_excludes_password() {
        let cfg = SourceConfig {
            source_type: "webdav".into(),
            webdav_url: " https://dav.example/ ".into(),
            webdav_user: "joplin".into(),
            webdav_pass: "secret".into(),
            ..Default::default()
        };
        let key = source_key(&cfg);
        assert_eq!(key, "webdav:https://dav.example/|joplin");
        assert!(!key.contains("secret")); // 缓存 key 不含密码
    }

    #[test]
    fn source_key_unknown_type() {
        let cfg = SourceConfig {
            source_type: "demo".into(),
            ..Default::default()
        };
        assert_eq!(source_key(&cfg), "demo:");
    }

    #[test]
    fn source_key_plugin_uses_config_key_not_secrets() {
        let cfg = SourceConfig {
            source_type: "plugin".into(),
            plugin_id: "webdav-storage".into(),
            plugin_storage: "webdav".into(),
            // apply 时算好的键（secret 已剔除）；原始 plugin_config 含密码但不参与
            plugin_config: r#"{"pass":"s3cret","url":"https://x/"}"#.into(),
            plugin_config_key: r#"{"url":"https://x/"}"#.into(),
            ..Default::default()
        };
        let key = source_key(&cfg);
        assert_eq!(key, r#"plugin:webdav-storage/webdav|{"url":"https://x/"}"#);
        assert!(!key.contains("s3cret"));
    }

    #[test]
    fn plugin_state_and_settings_round_trip() {
        let store = ConfigStore::in_memory().unwrap();
        // 状态
        assert!(store.plugin_state("x").is_none());
        store.set_plugin_state("x", true, &["host:http".into(), "settings".into()]).unwrap();
        let (enabled, caps) = store.plugin_state("x").unwrap();
        assert!(enabled);
        assert_eq!(caps, ["host:http", "settings"]);
        // 设置 KV（value 为 JSON 文本）
        store.set_plugin_setting("x", "k", Some("{\"a\":1}")).unwrap();
        assert_eq!(store.plugin_settings("x"), [("k".to_string(), "{\"a\":1}".to_string())]);
        store.set_plugin_setting("x", "k", None).unwrap();
        assert!(store.plugin_settings("x").is_empty());
        // 卸载清理
        store.set_plugin_setting("x", "k2", Some("1")).unwrap();
        store.remove_plugin("x").unwrap();
        assert!(store.plugin_state("x").is_none());
        assert!(store.plugin_settings("x").is_empty());
    }
}
