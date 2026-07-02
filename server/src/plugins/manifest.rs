//! manifest.toml 解析与校验（spec §3/§4）。未知字段一律忽略（向前兼容）。

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 宿主支持的插件 API 版本集合（spec §4）。
pub const HOST_API_VERSIONS: &[&str] = &["0.1", "0.2"];

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

/// 存储 provider 贡献（spec §3.9，0.2 新增）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageContribution {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: String,
    pub config_schema: Schema,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Contributes {
    #[serde(default)]
    pub theme: Vec<ThemeContribution>,
    #[serde(default)]
    pub storage: Vec<StorageContribution>,
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
    for s in &m.contributes.storage {
        if !is_valid_id(&s.id) {
            bail!("存储贡献 id 非法: {:?}", s.id);
        }
        if s.name.trim().is_empty() {
            bail!("存储贡献 {} 缺 name", s.id);
        }
        validate_schema(&s.config_schema, &format!("存储贡献 {}", s.id))?;
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
        // minHostVersion 高于宿主
        assert!(parse(
            "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"\nminHostVersion = \"999.0.0\""
        )
        .is_err());
        // select 缺 options
        assert!(parse(
            "id = \"a\"\nname = \"x\"\nversion = \"1\"\napiVersion = \"0.2\"\n[settings.schema]\nm = { type = \"select\" }"
        )
        .is_err());
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
