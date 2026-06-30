//! jasper-core —— Joplin 同步格式的可移植核心：数据模型、条目解析、序列化、内存索引。
//!
//! 不含任何 IO（文件系统 / 网络 / SQLite），纯计算，可同时用于：
//! - 原生服务端（server crate，配合 storage/cache 做拉取与缓存）
//! - 浏览器 WASM demo（wasm crate，配合内存后端）

pub mod library;
pub mod model;
pub mod parser;
pub mod serialize;
