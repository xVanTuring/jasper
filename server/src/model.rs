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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
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
    pub is_conflict: bool,
    pub source_url: String,
    pub order: i64,
}

#[derive(Debug, Clone)]
pub struct Folder {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub created_time: i64,
    pub updated_time: i64,
    pub icon: String,
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub id: String,
    pub title: String,
    pub mime: String,
    pub file_extension: String,
    pub size: i64,
    pub updated_time: i64,
}

#[derive(Debug, Clone)]
pub struct Tag {
    pub id: String,
    pub parent_id: String,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct NoteTag {
    pub id: String,
    pub note_id: String,
    pub tag_id: String,
}
