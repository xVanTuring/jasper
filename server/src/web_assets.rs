//! 内嵌前端（单文件打包）。仅在 `embed` feature 下编译。
//!
//! 编译期把 `../web/dist`（相对本 crate 的 Cargo.toml）整个塞进二进制，
//! 运行时由 `handler` 当作 axum fallback 直接吐出，不再读磁盘。
//! SPA 语义：任何未命中的路径回退到 index.html（与 ServeDir 的 not_found_service 一致）。

use axum::{
	http::{header, StatusCode, Uri},
	response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../web/dist"]
struct Assets;

pub async fn handler(uri: Uri) -> Response {
	let path = uri.path().trim_start_matches('/');
	serve(path)
		.or_else(|| serve("index.html"))
		.unwrap_or_else(missing)
}

fn serve(path: &str) -> Option<Response> {
	let file = Assets::get(path)?;
	let mime = file.metadata.mimetype();
	Some(([(header::CONTENT_TYPE, mime)], file.data.into_owned()).into_response())
}

fn missing() -> Response {
	(
		StatusCode::NOT_FOUND,
		"前端资源未嵌入：构建时 web/dist 为空（请先 `cd web && pnpm build`）",
	)
		.into_response()
}
