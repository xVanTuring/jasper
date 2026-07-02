//! Joplin 条目的数据模型。
//!
//! 字段集来源：joplin/packages/lib/services/database/types.ts
//! type_ 枚举来源：joplin/packages/lib/BaseModel.ts:12-29

use std::collections::HashMap;

/// Joplin 的 type_ 枚举（只读客户端关心的子集）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemType {
    Note = 1,
    Folder = 2,
    Resource = 4,
    Tag = 5,
    NoteTag = 6,
    /// 其它类型（Revision=13、MasterKey=9 等），只读 MVP 跳过。
    Other,
}

impl ItemType {
    pub fn from_i64(v: i64) -> Self {
        match v {
            1 => ItemType::Note,
            2 => ItemType::Folder,
            4 => ItemType::Resource,
            5 => ItemType::Tag,
            6 => ItemType::NoteTag,
            _ => ItemType::Other,
        }
    }
}

/// markup_language：决定笔记正文如何渲染。
/// 来源：joplin/packages/renderer/types.ts:3-7
/// serde 下序列化为整数 1|2（与 Joplin 元数据一致，插件 ABI 依赖此形状）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(into = "i64", from = "i64"))]
pub enum MarkupLanguage {
    Markdown = 1,
    Html = 2,
}

impl MarkupLanguage {
    pub fn from_i64(v: i64) -> Self {
        match v {
            2 => MarkupLanguage::Html,
            _ => MarkupLanguage::Markdown,
        }
    }
}

impl From<i64> for MarkupLanguage {
    fn from(v: i64) -> Self {
        MarkupLanguage::from_i64(v)
    }
}

impl From<MarkupLanguage> for i64 {
    fn from(v: MarkupLanguage) -> Self {
        v as i64
    }
}

/// 解析后的原始条目：标题/正文 + 所有元数据键值对（均为字符串）。
/// 后续按 type_ 转成下面的强类型结构。
#[derive(Debug, Clone)]
pub struct RawItem {
    pub type_: i64,
    pub title: Option<String>,
    pub body: Option<String>,
    pub props: HashMap<String, String>,
}

impl RawItem {
    pub fn item_type(&self) -> ItemType {
        ItemType::from_i64(self.type_)
    }

    pub fn id(&self) -> Option<&str> {
        self.props.get("id").map(|s| s.as_str())
    }

    pub fn prop(&self, key: &str) -> Option<&str> {
        self.props.get(key).map(|s| s.as_str())
    }

    /// 是否为加密条目（明文场景应跳过）。来源：encryption_applied 字段。
    pub fn is_encrypted(&self) -> bool {
        self.prop("encryption_applied").map(|v| v == "1").unwrap_or(false)
    }
}

// 下列 cfg_attr(serde) 让插件 ABI（spec §6.5 数据形状）与宿主共用同一套类型；
// 规范形状之外的字段标 serde(default)，规范形状的 JSON 也能反序列化。
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Note {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub body: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub markup_language: MarkupLanguage,
    pub is_todo: bool,
    pub todo_completed: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub is_conflict: bool,
    #[cfg_attr(feature = "serde", serde(default))]
    pub source_url: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub order: i64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Folder {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub created_time: i64,
    pub updated_time: i64,
    #[cfg_attr(feature = "serde", serde(default))]
    pub icon: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Resource {
    pub id: String,
    pub title: String,
    pub mime: String,
    pub file_extension: String,
    pub size: i64,
    pub updated_time: i64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Tag {
    pub id: String,
    pub parent_id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NoteTag {
    pub id: String,
    pub note_id: String,
    pub tag_id: String,
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;

    #[test]
    fn markup_language_serializes_as_int() {
        assert_eq!(serde_json::to_string(&MarkupLanguage::Markdown).unwrap(), "1");
        assert_eq!(serde_json::to_string(&MarkupLanguage::Html).unwrap(), "2");
        assert_eq!(serde_json::from_str::<MarkupLanguage>("2").unwrap(), MarkupLanguage::Html);
        // 未知值回落 Markdown（与 from_i64 一致）
        assert_eq!(serde_json::from_str::<MarkupLanguage>("99").unwrap(), MarkupLanguage::Markdown);
    }

    #[test]
    fn note_round_trips_and_accepts_spec_shape() {
        let note = Note {
            id: "a".repeat(32),
            parent_id: "b".repeat(32),
            title: "标题".into(),
            body: "正文".into(),
            created_time: 1,
            updated_time: 2,
            markup_language: MarkupLanguage::Html,
            is_todo: true,
            todo_completed: false,
            is_conflict: false,
            source_url: String::new(),
            order: 7,
        };
        let json = serde_json::to_string(&note).unwrap();
        assert!(json.contains("\"markup_language\":2"));
        let back: Note = serde_json::from_str(&json).unwrap();
        assert_eq!(back.title, note.title);
        assert_eq!(back.order, 7);

        // 规范 §6.5 的 Note 形状（无 is_conflict/order）也能反序列化，缺省补零值
        let spec = r#"{"id":"x","parent_id":"","title":"t","body":"b",
            "markup_language":1,"created_time":0,"updated_time":0,
            "is_todo":false,"todo_completed":false,"source_url":""}"#;
        let n: Note = serde_json::from_str(spec).unwrap();
        assert!(!n.is_conflict);
        assert_eq!(n.order, 0);
    }

    #[test]
    fn folder_accepts_missing_icon() {
        let f: Folder = serde_json::from_str(
            r#"{"id":"x","parent_id":"","title":"t","created_time":0,"updated_time":0}"#,
        )
        .unwrap();
        assert_eq!(f.icon, "");
    }
}
