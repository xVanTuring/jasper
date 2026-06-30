//! 写回：把编辑后的笔记重新序列化为 Joplin 的 `.md` 格式。
//!
//! 设计原则：编辑现有笔记时，**保留原文件的元数据块逐字不变**，只替换
//! 标题、正文，并刷新 updated_time / user_updated_time。这样与 Joplin
//! 双向兼容、diff 最小，且无需复刻字段顺序与转义规则。

use anyhow::{anyhow, Result};

/// Unix 毫秒 → Joplin 的 ISO 时间 `YYYY-MM-DDTHH:mm:ss.SSSZ`（UTC）。
pub fn format_iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

/// 当前时间（Unix 毫秒）。
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 生成一个新的 32 位十六进制条目 ID。
pub fn new_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).expect("getrandom 失败");
    let mut s = String::with_capacity(32);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// 把现有笔记的原始内容更新为新的标题/正文，并刷新更新时间，其余元数据原样保留。
pub fn update_note_md(original: &str, new_title: &str, new_body: &str, now: i64) -> Result<String> {
    let lines: Vec<&str> = original.split('\n').collect();

    // 自底向上找到 正文与元数据之间的分隔空行（与解析逻辑一致）
    let mut sep = None;
    let mut i = lines.len();
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if t.is_empty() {
            sep = Some(i);
            break;
        }
        if !t.contains(':') {
            break; // 非法元数据行，停止
        }
    }
    let sep = sep.ok_or_else(|| anyhow!("无法定位元数据块"))?;

    let iso = format_iso(now);
    let new_meta: Vec<String> = lines[sep + 1..]
        .iter()
        .map(|l| {
            let key = l.trim_start();
            if key.starts_with("updated_time:") {
                format!("updated_time: {iso}")
            } else if key.starts_with("user_updated_time:") {
                format!("user_updated_time: {iso}")
            } else {
                (*l).to_string()
            }
        })
        .collect();

    Ok(format!("{new_title}\n\n{new_body}\n\n{}", new_meta.join("\n")))
}

/// 把现有笔记移动到新笔记本：只改 `parent_id` 并刷新更新时间，
/// 标题/正文/其余元数据逐字保留（与 Joplin diff 最小、双向兼容）。
pub fn move_note_md(original: &str, new_parent_id: &str, now: i64) -> Result<String> {
    let lines: Vec<&str> = original.split('\n').collect();

    // 自底向上找正文与元数据之间的分隔空行（与解析/更新逻辑一致）
    let mut sep = None;
    let mut i = lines.len();
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if t.is_empty() {
            sep = Some(i);
            break;
        }
        if !t.contains(':') {
            break;
        }
    }
    let sep = sep.ok_or_else(|| anyhow!("无法定位元数据块"))?;

    let iso = format_iso(now);
    let new_meta: Vec<String> = lines[sep + 1..]
        .iter()
        .map(|l| {
            let key = l.trim_start();
            if key.starts_with("parent_id:") {
                format!("parent_id: {new_parent_id}")
            } else if key.starts_with("updated_time:") {
                format!("updated_time: {iso}")
            } else if key.starts_with("user_updated_time:") {
                format!("user_updated_time: {iso}")
            } else {
                (*l).to_string()
            }
        })
        .collect();

    // 标题/正文段（含其内部空行）逐字保留，仅替换元数据块
    let head = lines[..sep].join("\n");
    Ok(format!("{head}\n\n{}", new_meta.join("\n")))
}

/// 生成一篇新笔记的 `.md` 内容（字段顺序对齐真实 Joplin 笔记）。
pub fn new_note_md(id: &str, parent_id: &str, title: &str, body: &str, now: i64) -> String {
    let iso = format_iso(now);
    let props = [
        format!("id: {id}"),
        format!("parent_id: {parent_id}"),
        format!("created_time: {iso}"),
        format!("updated_time: {iso}"),
        "is_conflict: 0".to_string(),
        "latitude: 0.00000000".to_string(),
        "longitude: 0.00000000".to_string(),
        "altitude: 0.0000".to_string(),
        "author: ".to_string(),
        "source_url: ".to_string(),
        "is_todo: 0".to_string(),
        "todo_due: 0".to_string(),
        "todo_completed: 0".to_string(),
        "source: jasper".to_string(),
        "source_application: net.cozic.jasper".to_string(),
        "application_data: ".to_string(),
        "order: 0".to_string(),
        format!("user_created_time: {iso}"),
        format!("user_updated_time: {iso}"),
        "encryption_cipher_text: ".to_string(),
        "encryption_applied: 0".to_string(),
        "markup_language: 1".to_string(),
        "is_shared: 0".to_string(),
        "share_id: ".to_string(),
        "conflict_original_id: ".to_string(),
        "master_key_id: ".to_string(),
        "user_data: ".to_string(),
        "deleted_time: 0".to_string(),
        "type_: 1".to_string(),
    ];
    format!("{title}\n\n{body}\n\n{}", props.join("\n"))
}

/// 生成一个新资源（type_=4）的元数据 `.md` 内容。
/// 字段集/顺序/默认值对齐真实 Joplin 资源（含 OCR 字段：ocr_driver_id 默认 1、ocr_status 0）。
/// 资源条目无正文段，仅 `标题\n\n元数据`。
pub fn new_resource_md(
    id: &str,
    title: &str,
    mime: &str,
    file_extension: &str,
    size: i64,
    now: i64,
) -> String {
    let iso = format_iso(now);
    let props = [
        format!("id: {id}"),
        format!("mime: {mime}"),
        "filename: ".to_string(),
        format!("created_time: {iso}"),
        format!("updated_time: {iso}"),
        format!("user_created_time: {iso}"),
        format!("user_updated_time: {iso}"),
        format!("file_extension: {file_extension}"),
        "encryption_cipher_text: ".to_string(),
        "encryption_applied: 0".to_string(),
        "encryption_blob_encrypted: 0".to_string(),
        format!("size: {size}"),
        "is_shared: 0".to_string(),
        "share_id: ".to_string(),
        "master_key_id: ".to_string(),
        "user_data: ".to_string(),
        format!("blob_updated_time: {now}"),
        "ocr_text: ".to_string(),
        "ocr_details: ".to_string(),
        "ocr_status: 0".to_string(),
        "ocr_error: ".to_string(),
        "ocr_driver_id: 1".to_string(),
        "type_: 4".to_string(),
    ];
    format!("{title}\n\n{}", props.join("\n"))
}

/// 更新资源条目的标题并刷新更新时间，其余元数据原样保留。
/// 资源条目“正文段”仅一行标题，故直接以新标题替换。
pub fn update_resource_md(original: &str, new_title: &str, now: i64) -> Result<String> {
    let lines: Vec<&str> = original.split('\n').collect();
    let mut sep = None;
    let mut i = lines.len();
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if t.is_empty() {
            sep = Some(i);
            break;
        }
        if !t.contains(':') {
            break;
        }
    }
    let sep = sep.ok_or_else(|| anyhow!("无法定位元数据块"))?;
    let iso = format_iso(now);
    let new_meta: Vec<String> = lines[sep + 1..]
        .iter()
        .map(|l| {
            let key = l.trim_start();
            if key.starts_with("updated_time:") {
                format!("updated_time: {iso}")
            } else if key.starts_with("user_updated_time:") {
                format!("user_updated_time: {iso}")
            } else {
                (*l).to_string()
            }
        })
        .collect();
    Ok(format!("{new_title}\n\n{}", new_meta.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;

    const SAMPLE: &str = "原标题\n\n原正文第一行\n\n原正文第二行\n\nid: a1b2c3d4e5f60718293a4b5c6d7e8f90\nparent_id: 0f1e2d3c4b5a69788796a5b4c3d2e1f0\ncreated_time: 2024-01-01T00:00:00.000Z\nupdated_time: 2024-01-01T00:00:00.000Z\nsource_url: https://x.com\nmarkup_language: 1\nuser_updated_time: 2024-01-01T00:00:00.000Z\ntype_: 1";

    #[test]
    fn update_preserves_metadata_changes_body_and_time() {
        let out = update_note_md(SAMPLE, "新标题", "新正文", 1_700_000_000_000).unwrap();
        let raw = parser::parse_item(&out).unwrap();
        let note = parser::to_note(&raw).unwrap();
        assert_eq!(note.title, "新标题");
        assert_eq!(note.body, "新正文");
        // 其余元数据保留
        assert_eq!(note.parent_id, "0f1e2d3c4b5a69788796a5b4c3d2e1f0");
        assert_eq!(raw.prop("source_url"), Some("https://x.com"));
        assert_eq!(raw.prop("id"), Some("a1b2c3d4e5f60718293a4b5c6d7e8f90"));
        // created_time 不变，updated_time 已刷新
        assert_eq!(note.created_time, 1_704_067_200_000); // 2024-01-01
        assert_eq!(note.updated_time, 1_700_000_000_000);
    }

    #[test]
    fn move_changes_parent_keeps_rest() {
        let out = move_note_md(SAMPLE, "ffffffffffffffffffffffffffffffff", 1_700_000_000_000).unwrap();
        let raw = parser::parse_item(&out).unwrap();
        let note = parser::to_note(&raw).unwrap();
        // parent 改了，标题/正文/其余元数据保留
        assert_eq!(note.parent_id, "ffffffffffffffffffffffffffffffff");
        assert_eq!(note.title, "原标题");
        assert_eq!(note.body, "原正文第一行\n\n原正文第二行");
        assert_eq!(raw.prop("source_url"), Some("https://x.com"));
        assert_eq!(raw.prop("id"), Some("a1b2c3d4e5f60718293a4b5c6d7e8f90"));
        // created_time 不变，updated_time 已刷新
        assert_eq!(note.created_time, 1_704_067_200_000);
        assert_eq!(note.updated_time, 1_700_000_000_000);
    }

    #[test]
    fn new_note_is_parseable() {
        let id = "ffffffffffffffffffffffffffffffff";
        let out = new_note_md(id, "parentparentparentparentparent12", "标题", "正文内容", 1_700_000_000_000);
        let raw = parser::parse_item(&out).unwrap();
        let note = parser::to_note(&raw).unwrap();
        assert_eq!(note.id, id);
        assert_eq!(note.title, "标题");
        assert_eq!(note.body, "正文内容");
        assert_eq!(note.parent_id, "parentparentparentparentparent12");
        assert_eq!(note.markup_language, crate::model::MarkupLanguage::Markdown);
    }

    #[test]
    fn new_resource_is_parseable() {
        let id = "abcdef0123456789abcdef0123456789";
        let out = new_resource_md(id, "photo.png", "image/png", "png", 12345, 1_700_000_000_000);
        let raw = parser::parse_item(&out).unwrap();
        assert_eq!(raw.item_type(), crate::model::ItemType::Resource);
        let r = parser::to_resource(&raw).unwrap();
        assert_eq!(r.id, id);
        assert_eq!(r.title, "photo.png");
        assert_eq!(r.mime, "image/png");
        assert_eq!(r.file_extension, "png");
        assert_eq!(r.size, 12345);
        // 资源条目无正文
        assert!(raw.body.is_none());
        assert_eq!(raw.prop("ocr_driver_id"), Some("1"));
    }

    #[test]
    fn rename_resource_keeps_metadata() {
        let id = "abcdef0123456789abcdef0123456789";
        let orig = new_resource_md(id, "old.png", "image/png", "png", 99, 1_700_000_000_000);
        let out = update_resource_md(&orig, "新名字.png", 1_700_000_999_000).unwrap();
        let raw = parser::parse_item(&out).unwrap();
        let r = parser::to_resource(&raw).unwrap();
        assert_eq!(r.title, "新名字.png");
        assert_eq!(r.id, id); // id 不变
        assert_eq!(r.mime, "image/png");
        assert_eq!(r.size, 99);
        assert_eq!(r.updated_time, 1_700_000_999_000); // 已刷新
    }

    #[test]
    fn new_id_is_32_hex() {
        let id = new_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(new_id(), new_id());
    }
}
