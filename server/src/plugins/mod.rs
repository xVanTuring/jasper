//! 插件系统宿主（spec 0.2；feature = "plugins"）。
//!
//! 本文件在**两种构建模式下都编译**：feature 关闭时只剩零成本桩——
//! `PluginHost` 是永不构造的占位类型、`api_router()` 返回空路由、
//! `init()` 恒 None，默认构建不引入 wasmi/zip/toml 任何依赖。

use crate::api::AppState;
use crate::config::ConfigStore;
use axum::Router;
use std::sync::{Arc, Mutex};

#[cfg(feature = "plugins")]
mod ai;
#[cfg(feature = "plugins")]
mod hooks;
#[cfg(feature = "plugins")]
mod host;
#[cfg(feature = "plugins")]
mod host_api;
#[cfg(feature = "plugins")]
pub mod install;
#[cfg(feature = "plugins")]
pub mod manifest;
#[cfg(feature = "plugins")]
pub mod routes;
#[cfg(feature = "plugins")]
pub mod runtime;
#[cfg(feature = "plugins")]
pub mod storage;

#[cfg(feature = "plugins")]
pub use host::PluginHost;
#[cfg(feature = "plugins")]
pub use storage::{build_plugin_storage, prepare_plugin_source};

#[cfg(feature = "plugins")]
pub fn init(config: Arc<Mutex<ConfigStore>>) -> Option<Arc<PluginHost>> {
    match PluginHost::init(config) {
        Ok(host) => {
            let infos = host.list_info();
            let enabled = infos.iter().filter(|p| p.enabled).count();
            tracing::info!(
                installed = infos.len(), enabled, dir = ?host.dir,
                "plugin host initialized",
            );
            Some(host)
        }
        Err(e) => {
            tracing::warn!("plugin host init failed (disabling plugins for this run): {e}");
            None
        }
    }
}

#[cfg(feature = "plugins")]
pub fn api_router() -> Router<Arc<AppState>> {
    routes::router()
}

/// before-save 钩子入口：把（改好标题/正文的）笔记交给订阅插件串联改写，
/// 返回最终 `(title, body)`。宿主只取这两个字段 → 元数据逐字保留是结构性的。
/// 任何插件失败都回退到入参值，绝不丢用户数据（spec §8）。
#[cfg(feature = "plugins")]
pub async fn before_save(host: &Option<Arc<PluginHost>>, note: crate::model::Note) -> (String, String) {
    let Some(host) = host else {
        return (note.title, note.body);
    };
    if host.before_save_plugins().is_empty() {
        return (note.title, note.body);
    }
    let fallback = (note.title.clone(), note.body.clone());
    let host = host.clone();
    match tokio::task::spawn_blocking(move || hooks::run_before_save(&host, note)).await {
        Ok(n) => (n.title, n.body),
        Err(_) => fallback,
    }
}

// ---------- feature 关闭时的零成本桩 ----------

/// 占位类型：feature 关闭时 `AppState.plugins` 恒为 None，本类型永不构造。
#[cfg(not(feature = "plugins"))]
pub struct PluginHost;

#[cfg(not(feature = "plugins"))]
pub fn init(_config: Arc<Mutex<ConfigStore>>) -> Option<Arc<PluginHost>> {
    None
}

#[cfg(not(feature = "plugins"))]
pub fn api_router() -> Router<Arc<AppState>> {
    Router::new()
}

/// 直通桩：feature 关闭时保存路径零开销。
#[cfg(not(feature = "plugins"))]
pub async fn before_save(_host: &Option<Arc<PluginHost>>, note: crate::model::Note) -> (String, String) {
    (note.title, note.body)
}

#[cfg(not(feature = "plugins"))]
pub fn prepare_plugin_source(
    _cfg: &mut crate::config::SourceConfig,
    _host: Option<&Arc<PluginHost>>,
) -> Result<(), String> {
    Err("此构建不含插件支持（需 --features plugins）".into())
}

#[cfg(not(feature = "plugins"))]
pub fn build_plugin_storage(
    _cfg: &crate::config::SourceConfig,
    _host: Option<&Arc<PluginHost>>,
) -> anyhow::Result<Arc<dyn crate::storage::StorageBackend>> {
    Err(anyhow::anyhow!("此构建不含插件支持（需 --features plugins）"))
}
