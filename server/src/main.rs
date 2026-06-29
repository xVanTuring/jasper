//! joplin-lite —— 轻量只读 Joplin 客户端（本地 HTTP 服务 + 浏览器 UI）
//!
//! 启动后扫描本地数据目录 → 内存索引 → 提供 HTTP API + 托管前端 SPA。
//!
//! 用法：joplin-lite [数据源]   （可选，首次也可在浏览器里完成配置）
//!   数据源可以是本地目录路径，或 http(s):// 开头的 WebDAV 地址。
//! 配置持久化在平台配置目录 joplin-lite/config.db；命令行参数仅用于首次引导。
//! 环境变量：
//!   JOPLIN_LITE_SOURCE        引导用数据源（仅当尚无保存配置时生效）
//!   JOPLIN_LITE_WEBDAV_USER   引导用 WebDAV 用户名
//!   JOPLIN_LITE_WEBDAV_PASS   引导用 WebDAV 密码
//!   JOPLIN_LITE_HOST          监听地址（默认 127.0.0.1；局域网/容器设 0.0.0.0）
//!   JOPLIN_LITE_PORT          端口（默认 27583）
//!   JOPLIN_LITE_CONFIG_DIR    配置库目录（默认平台配置目录；容器里挂卷）
//!   JOPLIN_LITE_WEB_DIR       前端静态目录（默认相对源码；容器里指向打包路径）

mod api;
mod cache;
mod config;
mod indexer;
mod storage;

// 纯逻辑（数据模型/解析/序列化/索引）来自 joplin-core；以 crate 路径重导出，
// 让既有的 crate::{model,parser,serialize,library} 引用保持不变。
pub use joplin_core::{library, model, parser, serialize};

use anyhow::Result;
use api::AppState;
use config::{ConfigStore, SourceConfig};
use library::Library;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use storage::StorageBackend;
use tower_http::services::{ServeDir, ServeFile};

// 首次引导：把命令行/环境变量里的数据源转成配置（仅当配置库里还没有时使用）。
fn bootstrap_config() -> Option<SourceConfig> {
    let src = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("JOPLIN_LITE_SOURCE").ok())?;
    if src.starts_with("http://") || src.starts_with("https://") {
        Some(SourceConfig {
            source_type: "webdav".to_string(),
            webdav_url: src,
            webdav_user: std::env::var("JOPLIN_LITE_WEBDAV_USER").unwrap_or_default(),
            webdav_pass: std::env::var("JOPLIN_LITE_WEBDAV_PASS").unwrap_or_default(),
            ..Default::default()
        })
    } else {
        Some(SourceConfig {
            source_type: "local".to_string(),
            local_path: src,
            ..Default::default()
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let host = std::env::var("JOPLIN_LITE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("JOPLIN_LITE_PORT").unwrap_or_else(|_| "27583".to_string());

    println!("joplin-lite 启动中…");

    let config_store = ConfigStore::open()?;
    // 增量缓存库；打开失败则退化为内存缓存（等同禁用缓存，不影响功能）。
    let cache_store = cache::CacheStore::open().unwrap_or_else(|e| {
        eprintln!("缓存库打开失败，本次禁用增量缓存: {e}");
        cache::CacheStore::in_memory().expect("内存缓存初始化失败")
    });
    // 已保存配置优先；否则用命令行/环境变量引导
    let cfg = config_store.load().or_else(bootstrap_config);

    let mut library = Library::default();
    let mut storage_opt: Option<Arc<dyn StorageBackend>> = None;

    if let Some(cfg) = &cfg {
        match config::build_storage(cfg) {
            Ok(storage) => match indexer::build_cached(
                storage.as_ref(),
                &cache_store,
                &config::source_key(cfg),
            ) {
                Ok((lib, stats)) => {
                    println!(
                        "数据源: {} | 索引: 笔记={} 笔记本={} 资源={} 标签={} 错误={} | 缓存命中={} 拉取={}",
                        cfg.source_type, stats.notes, stats.folders, stats.resources, stats.tags,
                        stats.errors, stats.cached, stats.fetched
                    );
                    library = lib;
                    storage_opt = Some(storage);
                    // 引导成功则持久化，省得每次传参
                    if config_store.load().is_none() {
                        config_store.save(cfg).ok();
                    }
                }
                Err(e) => eprintln!("索引失败（进入未配置状态，可在浏览器重新配置）: {e}"),
            },
            Err(e) => eprintln!("数据源无效（进入未配置状态）: {e}"),
        }
    }

    if storage_opt.is_none() {
        println!("尚未配置数据源 —— 请在浏览器中完成首次配置。");
    }

    let state = Arc::new(AppState {
        library: RwLock::new(library),
        storage: RwLock::new(storage_opt),
        config: Mutex::new(config_store),
        cache: cache_store,
    });

    // 托管前端构建产物（web/dist），SPA 回退到 index.html。开发期前端跑 Vite 即可。
    // 路径可用 JOPLIN_LITE_WEB_DIR 覆盖（Docker 等场景下编译期路径不存在）。
    let web_dist = std::env::var("JOPLIN_LITE_WEB_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist"));
    let index = web_dist.join("index.html");
    let serve_dir = ServeDir::new(&web_dist).not_found_service(ServeFile::new(&index));

    let app = api::router(state).fallback_service(serve_dir);

    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("\n  ➜  浏览器打开: http://{}/", display_host(&host, &port));
    println!("  ➜  API 根:     http://{addr}/api/folders\n");
    if !web_dist.exists() {
        println!("（提示：前端尚未构建，web/dist 不存在；可先访问 /api/* 验证后端）\n");
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn display_host(host: &str, port: &str) -> String {
    let h = if host == "0.0.0.0" { "localhost" } else { host };
    format!("{h}:{port}")
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    println!("\n收到退出信号，关闭服务。");
}
