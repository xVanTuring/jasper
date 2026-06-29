//! joplin-lite —— 轻量只读 Joplin 客户端（本地 HTTP 服务 + 浏览器 UI）
//!
//! 启动后扫描本地数据目录 → 内存索引 → 提供 HTTP API + 托管前端 SPA。
//!
//! 用法：joplin-lite [数据源]
//!   数据源可以是本地目录路径，或 http(s):// 开头的 WebDAV 地址。
//! 环境变量：
//!   JOPLIN_LITE_SOURCE        数据源（等价于命令行参数）
//!   JOPLIN_LITE_WEBDAV_USER   WebDAV 用户名
//!   JOPLIN_LITE_WEBDAV_PASS   WebDAV 密码
//!   JOPLIN_LITE_HOST          监听地址（默认 127.0.0.1；局域网访问设 0.0.0.0）
//!   JOPLIN_LITE_PORT          端口（默认 27583）

mod api;
mod library;
mod model;
mod parser;
mod serialize;
mod storage;

use anyhow::Result;
use api::AppState;
use library::Library;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use storage::local::LocalStorage;
use storage::webdav::WebDavStorage;
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() -> Result<()> {
    let source = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("JOPLIN_LITE_SOURCE").ok())
        .unwrap_or_else(|| format!("{}/../JopinData", env!("CARGO_MANIFEST_DIR")));
    let host = std::env::var("JOPLIN_LITE_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("JOPLIN_LITE_PORT").unwrap_or_else(|_| "27583".to_string());

    println!("joplin-lite 启动中…");

    let storage: Arc<dyn storage::StorageBackend> =
        if source.starts_with("http://") || source.starts_with("https://") {
            let user = std::env::var("JOPLIN_LITE_WEBDAV_USER").ok();
            let pass = std::env::var("JOPLIN_LITE_WEBDAV_PASS").ok();
            println!("数据源: WebDAV {source}");
            Arc::new(WebDavStorage::new(&source, user.as_deref(), pass.as_deref()))
        } else {
            println!("数据源: 本地目录 {source}");
            Arc::new(LocalStorage::new(&source))
        };

    let (lib, stats) = Library::build(storage.as_ref())?;
    println!(
        "索引完成: 笔记={} 笔记本={} 资源={} 标签={} note_tag={} 其它={} 加密(跳过)={} 错误={}",
        stats.notes,
        stats.folders,
        stats.resources,
        stats.tags,
        stats.note_tags,
        stats.others,
        stats.encrypted,
        stats.errors
    );

    let state = Arc::new(AppState {
        library: RwLock::new(lib),
        storage,
    });

    // 托管前端构建产物（web/dist），SPA 回退到 index.html。开发期前端跑 Vite 即可。
    let web_dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../web/dist");
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
