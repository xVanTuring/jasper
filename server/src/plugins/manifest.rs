//! manifest.toml 解析与校验（spec §3/§4）。未知字段一律忽略（向前兼容）。

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 宿主支持的插件 API 版本集合（spec §4）。
pub const HOST_API_VERSIONS: &[&str] = &["0.1", "0.2", "0.3", "0.4"];

/// 内置 widget 词汇表（spec §9.2，0.3 冻结）。
pub const WIDGET_TYPES: &[&str] = &["chat", "list", "tree", "form", "markdown", "button"];

/// 设置/存储配置表单的字段定义（spec §10 词汇）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDef {
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
}

pub type Schema = BTreeMap<String, FieldDef>;

const FIELD_TYPES: &[&str] = &["string", "multiline", "secret", "bool", "number", "select"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backend {
    pub wasm: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub hooks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeContribution {
    pub id: String,
    pub name: String,
    pub base: String, // "light" | "dark"
    pub css: String,
}

/// 语言包贡献（spec §3.10，0.4 新增）：插件给应用**增加一门界面语言**。
/// 与主题贡献同构（零代码即可）：`messages` 指向一份 catalog JSON（message key → 译文），
/// 缺失的 key 回落到 `base`（默认 `en`）再回落宿主内置。宿主经资产端点托管该文件、
/// 前端注册进 i18n（语言切换器随之多出一项）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocaleContribution {
    /// 语言代码（如 `fr`、`ja`、`pt-BR`）；语言切换器里存/选的值。
    pub code: String,
    /// 该语言的自称显示名（如 `Français`），切换器直接展示。
    pub name: String,
    /// 缺失键的回落语言（宿主内置之一）；缺省 `en`。
    #[serde(default = "default_locale_base")]
    pub base: String,
    /// catalog JSON 路径（相对包根）：扁平对象 `{ "<msgKey>": "<译文>" }`。
    pub messages: String,
}

fn default_locale_base() -> String {
    "en".to_string()
}

/// 存储 provider 贡献（spec §3.9，0.2 新增）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageContribution {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    pub config_schema: Schema,
}

/// 命令贡献（spec §3.4）。`target = "backend"` 时经 wasm 的 `command` 方法执行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
    pub target: String, // "backend" | "builtin"
}

/// 工具栏贡献（spec §3.6）：把某命令放到指定位置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolbarContribution {
    pub command: String,
    pub location: String, // "note-toolbar" | "topbar"
}

/// 侧边栏面板贡献（spec §3.5，0.3 扩展）：静态 widget（交互经 `command`）
/// 或动态树（`view` → dispatch `ui`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidebarContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub icon: String,
    pub widget: String, // WIDGET_TYPES 之一
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Contributes {
    #[serde(default)]
    pub theme: Vec<ThemeContribution>,
    #[serde(default)]
    pub locale: Vec<LocaleContribution>,
    #[serde(default)]
    pub storage: Vec<StorageContribution>,
    #[serde(default)]
    pub command: Vec<CommandContribution>,
    #[serde(default)]
    pub toolbar: Vec<ToolbarContribution>,
    #[serde(default)]
    pub sidebar: Vec<SidebarContribution>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SettingsSection {
    #[serde(default)]
    pub schema: Schema,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "apiVersion")]
    pub api_version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default, rename = "minHostVersion")]
    pub min_host_version: String,
    #[serde(default)]
    pub backend: Option<Backend>,
    #[serde(default)]
    pub contributes: Contributes,
    #[serde(default)]
    pub settings: SettingsSection,
}

impl Manifest {
    pub fn has_backend(&self) -> bool {
        self.backend.is_some()
    }

    /// 纯零代码插件（无 wasm）：安装后可自动启用（spec §5）。
    pub fn is_zero_code(&self) -> bool {
        self.backend.is_none()
    }

    pub fn capabilities(&self) -> &[String] {
        self.backend.as_ref().map(|b| b.capabilities.as_slice()).unwrap_or(&[])
    }

    pub fn hooks(&self) -> &[String] {
        self.backend.as_ref().map(|b| b.hooks.as_slice()).unwrap_or(&[])
    }
}

/// 解析 + 校验 manifest.toml。
pub fn parse(text: &str) -> Result<Manifest> {
    let m: Manifest = toml::from_str(text).map_err(|e| anyhow::anyhow!("manifest.toml 解析失败: {e}"))?;
    validate(&m)?;
    Ok(m)
}

/// 插件/贡献 id 规则：`^[a-z0-9][a-z0-9-]*$`（spec §3.1）。
pub fn is_valid_id(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_lowercase() | b[0].is_ascii_digit()
        && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

/// 语言代码规则（BCP-47 子集）：字母开头，其后字母/数字/`-`（如 `fr`、`pt-BR`、`zh-Hans`）。
pub fn is_valid_locale_code(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b[0].is_ascii_alphabetic()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'-')
}

/// 包内相对路径规则：正斜杠、非空、不出根（spec §2）。
pub fn rel_path_ok(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// 版本比较：点分段数字优先，非数字段按字符串（避免引入 semver 依赖）。
pub fn cmp_version(a: &str, b: &str) -> std::cmp::Ordering {
    let split = |s: &str| -> Vec<String> { s.split(['.', '-', '+']).map(String::from).collect() };
    let (pa, pb) = (split(a), split(b));
    for i in 0..pa.len().max(pb.len()) {
        let (x, y) = (pa.get(i).map(String::as_str).unwrap_or("0"), pb.get(i).map(String::as_str).unwrap_or("0"));
        let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(nx), Ok(ny)) => nx.cmp(&ny),
            _ => x.cmp(y),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    std::cmp::Ordering::Equal
}

fn validate_schema(sch: &Schema, ctx: &str) -> Result<()> {
    for (key, f) in sch {
        if !FIELD_TYPES.contains(&f.field_type.as_str()) {
            bail!("{ctx} 字段 {key} 类型未知: {}", f.field_type);
        }
        if f.field_type == "select" && f.options.as_ref().map(|o| o.is_empty()).unwrap_or(true) {
            bail!("{ctx} 字段 {key}: select 必须给 options");
        }
    }
    Ok(())
}

fn validate(m: &Manifest) -> Result<()> {
    if !is_valid_id(&m.id) {
        bail!("非法插件 id: {:?}（须匹配 ^[a-z0-9][a-z0-9-]*$）", m.id);
    }
    if m.name.trim().is_empty() || m.version.trim().is_empty() {
        bail!("name/version 不能为空");
    }
    // apiVersion：MAJOR 必须在宿主支持集（目前只有 0）；MINOR 更高可加载（宿主打日志警告）
    let (major, _minor) = m
        .api_version
        .split_once('.')
        .and_then(|(a, b)| Some((a.parse::<u32>().ok()?, b.parse::<u32>().ok()?)))
        .ok_or_else(|| anyhow::anyhow!("apiVersion 非法: {:?}（须为 MAJOR.MINOR）", m.api_version))?;
    if !HOST_API_VERSIONS.iter().any(|v| v.split('.').next() == Some(&major.to_string())) {
        bail!("apiVersion {} 的 MAJOR 不受宿主支持（支持: {:?}）", m.api_version, HOST_API_VERSIONS);
    }
    // minHostVersion 大于当前宿主版本 → 拒绝（spec §4）
    if !m.min_host_version.trim().is_empty()
        && cmp_version(m.min_host_version.trim(), env!("CARGO_PKG_VERSION")) == std::cmp::Ordering::Greater
    {
        bail!("插件要求宿主 ≥ {}（当前 {}）", m.min_host_version, env!("CARGO_PKG_VERSION"));
    }
    if let Some(b) = &m.backend {
        if !rel_path_ok(&b.wasm) {
            bail!("backend.wasm 路径非法: {:?}", b.wasm);
        }
    }
    if !m.contributes.storage.is_empty() && m.backend.is_none() {
        bail!("contributes.storage 需要 [backend]（storage.* 由 wasm 实现）");
    }
    for t in &m.contributes.theme {
        if !is_valid_id(&t.id) {
            bail!("主题 id 非法: {:?}", t.id);
        }
        if t.base != "light" && t.base != "dark" {
            bail!("主题 {} 的 base 须为 light|dark", t.id);
        }
        if !rel_path_ok(&t.css) {
            bail!("主题 {} 的 css 路径非法: {:?}", t.id, t.css);
        }
    }
    let mut locale_codes = std::collections::HashSet::new();
    for l in &m.contributes.locale {
        if !is_valid_locale_code(&l.code) {
            bail!("语言代码非法: {:?}（如 fr / ja / pt-BR）", l.code);
        }
        if !locale_codes.insert(l.code.as_str()) {
            bail!("语言代码重复: {}", l.code);
        }
        if l.name.trim().is_empty() {
            bail!("语言 {} 缺 name", l.code);
        }
        if l.base != "en" && l.base != "zh" {
            bail!("语言 {} 的 base 须为 en|zh", l.code);
        }
        if !rel_path_ok(&l.messages) {
            bail!("语言 {} 的 messages 路径非法: {:?}", l.code, l.messages);
        }
    }
    for s in &m.contributes.storage {
        if !is_valid_id(&s.id) {
            bail!("存储贡献 id 非法: {:?}", s.id);
        }
        if s.name.trim().is_empty() {
            bail!("存储贡献 {} 缺 name", s.id);
        }
        validate_schema(&s.config_schema, &format!("存储贡献 {}", s.id))?;
    }
    let mut cmd_ids = std::collections::HashSet::new();
    for c in &m.contributes.command {
        // 命令 id 允许点分（如 ai.summarize）：小写字母数字开头，其后可含 . 与 -
        let ok = c.id.as_bytes().first().map(|b| b.is_ascii_lowercase() || b.is_ascii_digit()).unwrap_or(false)
            && c.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-');
        if !ok {
            bail!("命令 id 非法: {:?}", c.id);
        }
        if !cmd_ids.insert(c.id.as_str()) {
            bail!("命令 id 重复: {}", c.id);
        }
        if c.title.trim().is_empty() {
            bail!("命令 {} 缺 title", c.id);
        }
        if c.target != "backend" && c.target != "builtin" {
            bail!("命令 {} 的 target 须为 backend|builtin", c.id);
        }
        if c.target == "backend" && m.backend.is_none() {
            bail!("backend 命令 {} 需要 [backend]", c.id);
        }
    }
    for tb in &m.contributes.toolbar {
        if !cmd_ids.contains(tb.command.as_str()) {
            bail!("toolbar 引用了未声明的命令: {}", tb.command);
        }
        if tb.location != "note-toolbar" && tb.location != "topbar" {
            bail!("toolbar.location 须为 note-toolbar|topbar");
        }
    }
    // backend 命令 id 集合：sidebar.command 只能指向它们（spec §3.5，0.3）
    let backend_cmds: std::collections::HashSet<&str> = m
        .contributes
        .command
        .iter()
        .filter(|c| c.target == "backend")
        .map(|c| c.id.as_str())
        .collect();
    let mut sidebar_ids = std::collections::HashSet::new();
    for sb in &m.contributes.sidebar {
        if !is_valid_id(&sb.id) {
            bail!("sidebar id 非法: {:?}", sb.id);
        }
        if !sidebar_ids.insert(sb.id.as_str()) {
            bail!("sidebar id 重复: {}", sb.id);
        }
        if sb.title.trim().is_empty() {
            bail!("sidebar {} 缺 title", sb.id);
        }
        if !WIDGET_TYPES.contains(&sb.widget.as_str()) {
            bail!("sidebar {} 的 widget 未知: {:?}（支持: {:?}）", sb.id, sb.widget, WIDGET_TYPES);
        }
        if (sb.command.is_some() || sb.view.is_some()) && m.backend.is_none() {
            bail!("sidebar {} 声明了 command/view，需要 [backend]", sb.id);
        }
        if let Some(cmd) = &sb.command {
            if !backend_cmds.contains(cmd.as_str()) {
                bail!("sidebar {} 引用了未声明的 backend 命令: {}", sb.id, cmd);
            }
        }
        if let Some(view) = &sb.view {
            if view.trim().is_empty() {
                bail!("sidebar {} 的 view 不能为空串", sb.id);
            }
        }
        // 静态 chat 的交互只能走 command（spec §3.5）
        if sb.widget == "chat" && sb.view.is_none() && sb.command.is_none() {
            bail!("sidebar {}：widget=chat 且无 view 时必须给 command", sb.id);
        }
    }
    validate_schema(&m.settings.schema, "settings.schema")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backend_manifest() {
        let m = parse(
            r#"
id = "trim-trailing"
name = "去行尾空白"
version = "0.1.0"
apiVersion = "0.2"
future_field = "ignored"

[backend]
wasm = "plugin.wasm"
capabilities = []
hooks = ["before-save"]
"#,
        )
        .unwrap();
        assert_eq!(m.id, "trim-trailing");
        assert!(m.has_backend());
        assert_eq!(m.hooks(), ["before-save"]);
    }

    #[test]
    fn parses_storage_manifest() {
        let m = parse(
            r#"
id = "webdav-storage"
name = "WebDAV (插件)"
version = "1.0.0"
apiVersion = "0.2"

[backend]
wasm = "plugin.wasm"
capabilities = ["host:http"]

[[contributes.storage]]
id = "webdav"
name = "WebDAV (插件)"

[contributes.storage.config_schema]
url  = { type = "string", label = "WebDAV URL", required = true }
user = { type = "string" }
pass = { type = "secret" }
"#,
        )
        .unwrap();
        let s = &m.contributes.storage[0];
        assert_eq!(s.id, "webdav");
        assert_eq!(s.config_schema["pass"].field_type, "secret");
        assert_eq!(s.config_schema["url"].required, Some(true));
    }

    #[test]
    fn parses_locale_manifest() {
        let m = parse(
            r#"
id = "lang-fr"
name = "Français"
version = "1.0.0"
apiVersion = "0.4"

[[contributes.locale]]
code = "fr"
name = "Français"
messages = "assets/locales/fr.json"

[[contributes.locale]]
code = "pt-BR"
name = "Português (Brasil)"
base = "en"
messages = "assets/locales/pt-BR.json"
"#,
        )
        .unwrap();
        assert!(m.is_zero_code()); // 纯语言包无 backend → 零代码自动启用
        let ls = &m.contributes.locale;
        assert_eq!(ls.len(), 2);
        assert_eq!(ls[0].code, "fr");
        assert_eq!(ls[0].base, "en"); // 缺省
        assert_eq!(ls[1].code, "pt-BR");

        let base = "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.4\"\n";
        // 非法 code
        assert!(parse(&format!(
            "{base}[[contributes.locale]]\ncode = \"1fr\"\nname = \"F\"\nmessages = \"x.json\"\n"
        ))
        .is_err());
        // 缺 name
        assert!(parse(&format!(
            "{base}[[contributes.locale]]\ncode = \"fr\"\nname = \"\"\nmessages = \"x.json\"\n"
        ))
        .is_err());
        // base 非内置
        assert!(parse(&format!(
            "{base}[[contributes.locale]]\ncode = \"fr\"\nname = \"F\"\nbase = \"de\"\nmessages = \"x.json\"\n"
        ))
        .is_err());
        // messages 路径逃逸
        assert!(parse(&format!(
            "{base}[[contributes.locale]]\ncode = \"fr\"\nname = \"F\"\nmessages = \"../x.json\"\n"
        ))
        .is_err());
        // code 重复
        assert!(parse(&format!(
            "{base}[[contributes.locale]]\ncode = \"fr\"\nname = \"A\"\nmessages = \"a.json\"\n[[contributes.locale]]\ncode = \"fr\"\nname = \"B\"\nmessages = \"b.json\"\n"
        ))
        .is_err());
    }

    #[test]
    fn rejects_bad_shapes() {
        // 非法 id
        assert!(parse("id = \"Bad_Id\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"").is_err());
        // 路径逃逸
        assert!(parse(
            "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"\n[backend]\nwasm = \"../evil.wasm\""
        )
        .is_err());
        // storage 但无 backend
        assert!(parse(
            "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"\n[[contributes.storage]]\nid = \"s\"\nname = \"S\"\n[contributes.storage.config_schema]\nu = { type = \"string\" }"
        )
        .is_err());
        // MAJOR 不支持
        assert!(parse("id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"1.0\"").is_err());
        // minHostVersion 高于宿主（宿主版本是日期式 YYYY.M.D，「未来」要选到日期够不着的量级）
        assert!(parse(
            "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"\nminHostVersion = \"999999.0.0\""
        )
        .is_err());
        // select 缺 options
        assert!(parse(
            "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"\n[settings.schema]\nm = { type = \"select\" }"
        )
        .is_err());
    }

    #[test]
    fn parses_command_toolbar_manifest() {
        let m = parse(
            r#"
id = "ai-polish"
name = "一键优化"
version = "0.1.0"
apiVersion = "0.2"

[backend]
wasm = "plugin.wasm"
capabilities = ["settings", "host:http"]

[[contributes.command]]
id = "polish"
title = "一键优化"
target = "backend"
icon = "rich"

[[contributes.toolbar]]
command = "polish"
location = "note-toolbar"
"#,
        )
        .unwrap();
        assert_eq!(m.contributes.command[0].id, "polish");
        assert_eq!(m.contributes.toolbar[0].location, "note-toolbar");

        // toolbar 引用不存在的命令
        assert!(parse(
            "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"\n[[contributes.toolbar]]\ncommand = \"nope\"\nlocation = \"topbar\"\n"
        )
        .is_err());
        // backend 命令但无 [backend]
        assert!(parse(
            "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"\n[[contributes.command]]\nid = \"c\"\ntitle = \"C\"\ntarget = \"backend\"\n"
        )
        .is_err());
    }

    #[test]
    fn parses_sidebar_manifest_static_and_dynamic() {
        let m = parse(
            r#"
id = "ai-chat"
name = "AI 对话"
version = "0.1.0"
apiVersion = "0.3"

[backend]
wasm = "plugin.wasm"
capabilities = ["notes:read", "host:ai"]

[[contributes.command]]
id = "chat"
title = "发送"
target = "backend"

[[contributes.sidebar]]
id = "chat-panel"
title = "AI 对话"
icon = "chat"
widget = "chat"
command = "chat"

[[contributes.sidebar]]
id = "tools"
title = "工具箱"
widget = "markdown"
view = "main"
"#,
        )
        .unwrap();
        let sb = &m.contributes.sidebar;
        assert_eq!(sb.len(), 2);
        assert_eq!(sb[0].command.as_deref(), Some("chat"));
        assert!(sb[0].view.is_none());
        assert_eq!(sb[1].view.as_deref(), Some("main"));

        let base = "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.3\"\n";
        let backend = "[backend]\nwasm = \"plugin.wasm\"\n";
        // widget 未知
        assert!(parse(&format!(
            "{base}{backend}[[contributes.sidebar]]\nid = \"s\"\ntitle = \"S\"\nwidget = \"iframe\"\n"
        ))
        .is_err());
        // chat 无 view 时缺 command
        assert!(parse(&format!(
            "{base}{backend}[[contributes.sidebar]]\nid = \"s\"\ntitle = \"S\"\nwidget = \"chat\"\n"
        ))
        .is_err());
        // command 引用未声明命令
        assert!(parse(&format!(
            "{base}{backend}[[contributes.sidebar]]\nid = \"s\"\ntitle = \"S\"\nwidget = \"list\"\ncommand = \"nope\"\n"
        ))
        .is_err());
        // command/view 需要 [backend]
        assert!(parse(&format!(
            "{base}[[contributes.sidebar]]\nid = \"s\"\ntitle = \"S\"\nwidget = \"markdown\"\nview = \"v\"\n"
        ))
        .is_err());
        // 纯展示 sidebar（无 command/view、非 chat）不需要 backend
        assert!(parse(&format!(
            "{base}[[contributes.sidebar]]\nid = \"s\"\ntitle = \"S\"\nwidget = \"markdown\"\n"
        ))
        .is_ok());
        // sidebar id 重复
        assert!(parse(&format!(
            "{base}{backend}[[contributes.sidebar]]\nid = \"s\"\ntitle = \"A\"\nwidget = \"markdown\"\n[[contributes.sidebar]]\nid = \"s\"\ntitle = \"B\"\nwidget = \"markdown\"\n"
        ))
        .is_err());
    }

    #[test]
    fn api_version_0_3_accepted() {
        assert!(parse("id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.3\"").is_ok());
    }

    #[test]
    fn version_compare() {
        use std::cmp::Ordering::*;
        assert_eq!(cmp_version("1.2.0", "1.10.0"), Less);
        assert_eq!(cmp_version("1.2", "1.2.0"), Equal);
        assert_eq!(cmp_version("2.0.0", "1.9.9"), Greater);
    }

    #[test]
    fn path_rules() {
        assert!(rel_path_ok("assets/themes/x.css"));
        assert!(!rel_path_ok("/abs.css"));
        assert!(!rel_path_ok("a/../b.css"));
        assert!(!rel_path_ok("a\\b.css"));
        assert!(!rel_path_ok(""));
    }
}
