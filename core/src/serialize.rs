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
/// 用 std 而非 chrono::Utc::now()：省掉 chrono 的 clock/wasmbind feature，
/// 插件沙箱（wasm32-unknown-unknown）才能零 wasm-bindgen 依赖地编译 core。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("系统时钟早于 Unix 纪元")
        .as_millis() as i64
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
/// `is_todo` 为 true 时建为待办（is_todo: 1），否则普通笔记。
pub fn new_note_md(id: &str, parent_id: &str, title: &str, body: &str, is_todo: bool, now: i64) -> String {
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
        format!("is_todo: {}", if is_todo { 1 } else { 0 }),
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

/// 生成一个新笔记本（type_=2）的 `.md` 内容（字段顺序对齐真实 Joplin 笔记本）。
/// 笔记本无正文段，仅 `标题\n\n元数据`。
pub fn new_folder_md(id: &str, parent_id: &str, title: &str, now: i64) -> String {
    let iso = format_iso(now);
    let props = [
        format!("id: {id}"),
        format!("created_time: {iso}"),
        format!("updated_time: {iso}"),
        format!("user_created_time: {iso}"),
        format!("user_updated_time: {iso}"),
        "encryption_cipher_text: ".to_string(),
        "encryption_applied: 0".to_string(),
        format!("parent_id: {parent_id}"),
        "is_shared: 0".to_string(),
        "share_id: ".to_string(),
        "master_key_id: ".to_string(),
        "icon: ".to_string(),
        "user_data: ".to_string(),
        "deleted_time: 0".to_string(),
        "type_: 2".to_string(),
    ];
    format!("{title}\n\n{}", props.join("\n"))
}

/// 新建标签条目（type_=5）。字段集与顺序逐字对齐 Joplin 真实数据
/// （含空 `parent_id`/`user_data`，**无 `deleted_time`**——标签不进回收站）。
/// title 已由调用方 trim（Joplin `Tag.save` 也 trim；此处不再改动大小写，混合大小写标签受支持）。
pub fn new_tag_md(id: &str, title: &str, now: i64) -> String {
    let iso = format_iso(now);
    let props = [
        format!("id: {id}"),
        format!("created_time: {iso}"),
        format!("updated_time: {iso}"),
        format!("user_created_time: {iso}"),
        format!("user_updated_time: {iso}"),
        "encryption_cipher_text: ".to_string(),
        "encryption_applied: 0".to_string(),
        "is_shared: 0".to_string(),
        "parent_id: ".to_string(),
        "user_data: ".to_string(),
        "type_: 5".to_string(),
    ];
    format!("{title}\n\n{}", props.join("\n"))
}

/// 新建 note_tag 关联条目（type_=6，纯元数据、无标题无空行）。字段集/顺序对齐 Joplin。
pub fn new_note_tag_md(id: &str, note_id: &str, tag_id: &str, now: i64) -> String {
    let iso = format_iso(now);
    let props = [
        format!("id: {id}"),
        format!("note_id: {note_id}"),
        format!("tag_id: {tag_id}"),
        format!("created_time: {iso}"),
        format!("updated_time: {iso}"),
        format!("user_created_time: {iso}"),
        format!("user_updated_time: {iso}"),
        "encryption_cipher_text: ".to_string(),
        "encryption_applied: 0".to_string(),
        "is_shared: 0".to_string(),
        "type_: 6".to_string(),
    ];
    props.join("\n")
}

/// 把现有笔记本移动到新的父笔记本（改 parent_id），刷新更新时间，其余元数据原样保留。
/// 笔记本无正文段（仅标题），故标题逐字保留、仅重写元数据块。
pub fn move_folder_md(original: &str, new_parent_id: &str, now: i64) -> Result<String> {
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
    let head = lines[..sep].join("\n");
    Ok(format!("{head}\n\n{}", new_meta.join("\n")))
}

/// 重命名笔记本：只改标题并刷新更新时间，parent_id 等元数据逐字保留。
/// 笔记本无正文段（仅 `标题\n\n元数据`），故整段标题以新标题替换、仅重写元数据块。
pub fn rename_folder_md(original: &str, new_title: &str, now: i64) -> Result<String> {
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
        let out = new_note_md(id, "parentparentparentparentparent12", "标题", "正文内容", false, 1_700_000_000_000);
        let raw = parser::parse_item(&out).unwrap();
        let note = parser::to_note(&raw).unwrap();
        assert_eq!(note.id, id);
        assert_eq!(note.title, "标题");
        assert_eq!(note.body, "正文内容");
        assert_eq!(note.parent_id, "parentparentparentparentparent12");
        assert_eq!(note.markup_language, crate::model::MarkupLanguage::Markdown);
        assert!(!note.is_todo);
    }

    #[test]
    fn new_todo_sets_is_todo() {
        let out = new_note_md("ffffffffffffffffffffffffffffffff", "parentparentparentparentparent12", "待办", "", true, 1_700_000_000_000);
        let note = parser::to_note(&parser::parse_item(&out).unwrap()).unwrap();
        assert!(note.is_todo);
        assert!(!note.todo_completed);
    }

    #[test]
    fn new_folder_is_parseable() {
        let id = "abcabcabcabcabcabcabcabcabcabc12";
        let out = new_folder_md(id, "parentparentparentparentparent12", "我的笔记本", 1_700_000_000_000);
        let raw = parser::parse_item(&out).unwrap();
        assert_eq!(raw.item_type(), crate::model::ItemType::Folder);
        let f = parser::to_folder(&raw).unwrap();
        assert_eq!(f.id, id);
        assert_eq!(f.title, "我的笔记本");
        assert_eq!(f.parent_id, "parentparentparentparentparent12");
    }

    #[test]
    fn new_tag_is_parseable_and_matches_joplin_shape() {
        let id = "1434aa3b57b54f839c8a5fc4025cbf10";
        let out = new_tag_md(id, "emulator", 1_700_000_000_000);
        let raw = parser::parse_item(&out).unwrap();
        assert_eq!(raw.item_type(), crate::model::ItemType::Tag);
        let tag = parser::to_tag(&raw).unwrap();
        assert_eq!(tag.id, id);
        assert_eq!(tag.title, "emulator");
        assert_eq!(tag.parent_id, ""); // 空 parent_id
        // 字段集与顺序须与 Joplin 真实数据一致（含 user_data，无 deleted_time）
        let expected = "emulator\n\nid: 1434aa3b57b54f839c8a5fc4025cbf10\n\
            created_time: 2023-11-14T22:13:20.000Z\nupdated_time: 2023-11-14T22:13:20.000Z\n\
            user_created_time: 2023-11-14T22:13:20.000Z\nuser_updated_time: 2023-11-14T22:13:20.000Z\n\
            encryption_cipher_text: \nencryption_applied: 0\nis_shared: 0\nparent_id: \nuser_data: \ntype_: 5";
        assert_eq!(out, expected);
    }

    #[test]
    fn new_note_tag_is_parseable_and_matches_joplin_shape() {
        let (id, note_id, tag_id) = (
            "5828867dfe7f4ce184f3dff4e0e1e756",
            "8565caad9a4c487996e2e5be171af413",
            "1434aa3b57b54f839c8a5fc4025cbf10",
        );
        let out = new_note_tag_md(id, note_id, tag_id, 1_700_000_000_000);
        let raw = parser::parse_item(&out).unwrap();
        assert_eq!(raw.item_type(), crate::model::ItemType::NoteTag);
        assert!(raw.title.is_none()); // 纯元数据、无标题
        let nt = parser::to_note_tag(&raw).unwrap();
        assert_eq!((nt.id.as_str(), nt.note_id.as_str(), nt.tag_id.as_str()), (id, note_id, tag_id));
        let expected = "id: 5828867dfe7f4ce184f3dff4e0e1e756\n\
            note_id: 8565caad9a4c487996e2e5be171af413\ntag_id: 1434aa3b57b54f839c8a5fc4025cbf10\n\
            created_time: 2023-11-14T22:13:20.000Z\nupdated_time: 2023-11-14T22:13:20.000Z\n\
            user_created_time: 2023-11-14T22:13:20.000Z\nuser_updated_time: 2023-11-14T22:13:20.000Z\n\
            encryption_cipher_text: \nencryption_applied: 0\nis_shared: 0\ntype_: 6";
        assert_eq!(out, expected);
    }

    #[test]
    fn move_folder_changes_parent() {
        let orig = new_folder_md("abcabcabcabcabcabcabcabcabcabc12", "oldoldoldoldoldoldoldoldoldold12", "本子", 1_700_000_000_000);
        let out = move_folder_md(&orig, "newnewnewnewnewnewnewnewnewnew12", 1_700_000_999_000).unwrap();
        let f = parser::to_folder(&parser::parse_item(&out).unwrap()).unwrap();
        assert_eq!(f.parent_id, "newnewnewnewnewnewnewnewnewnew12");
        assert_eq!(f.title, "本子");
        assert_eq!(f.updated_time, 1_700_000_999_000);
    }

    #[test]
    fn rename_folder_changes_title_keeps_parent() {
        let orig = new_folder_md("abcabcabcabcabcabcabcabcabcabc12", "parentparentparentparentparent12", "旧名字", 1_700_000_000_000);
        let out = rename_folder_md(&orig, "新名字", 1_700_000_999_000).unwrap();
        let f = parser::to_folder(&parser::parse_item(&out).unwrap()).unwrap();
        assert_eq!(f.title, "新名字");
        assert_eq!(f.parent_id, "parentparentparentparentparent12"); // 父级逐字保留
        assert_eq!(f.updated_time, 1_700_000_999_000); // 刷新更新时间
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
