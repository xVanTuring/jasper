//! WebDAV 存储插件（spec §3.9/§6.5 参考实现）。
//! 网络全部经宿主代理的 `http.request`（能力 host:http），插件自身不碰 socket。
//! 协议面与内置后端一致：PROPFIND(Depth:1) / GET / PUT / DELETE / MKCOL + Basic Auth。

use base64::Engine as _;
use jasper_plugin_sdk as sdk;
use sdk::host::{http_request, HttpRequest, HttpResponse};
use sdk::rt::PluginError;
use sdk::serde_json::Value;
use sdk::storage::{ItemStat, Storage};
use std::collections::BTreeMap;

/// 全新笔记库的默认 info.json（与宿主 storage::DEFAULT_INFO_JSON 一致）。
const DEFAULT_INFO_JSON: &str = r#"{"version":3,"e2ee":{"value":false,"updatedTime":0},"activeMasterKeyId":{"value":"","updatedTime":0},"masterKeys":[],"ppk":{"value":null,"updatedTime":0},"appMinVersion":"3.0.0"}"#;

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:getlastmodified/><d:resourcetype/></d:prop></d:propfind>"#;

pub struct WebDav {
    base: String, // 规范化后无尾斜杠
    auth: Option<String>,
}

impl WebDav {
    fn request(&self, method: &str, url: &str) -> HttpRequest {
        let mut headers = BTreeMap::new();
        if let Some(a) = &self.auth {
            headers.insert("Authorization".to_string(), a.clone());
        }
        HttpRequest { method: method.into(), url: url.into(), headers, body: None, timeout_ms: None }
    }

    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, PluginError> {
        http_request(req)
    }

    /// 发送并要求 2xx；否则带状态码报错（404 → not_found）。
    fn send_ok(&self, req: &HttpRequest, what: &str) -> Result<HttpResponse, PluginError> {
        let resp = self.send(req)?;
        if resp.is_success() {
            Ok(resp)
        } else if resp.status == 404 {
            Err(PluginError::not_found(format!("{what}: 404")))
        } else {
            Err(PluginError::internal(format!("{what}: HTTP {}", resp.status)))
        }
    }
}

impl Storage for WebDav {
    fn from_config(config: &Value) -> Result<Self, PluginError> {
        let url = config.get("url").and_then(Value::as_str).unwrap_or("").trim().trim_end_matches('/');
        if url.is_empty() {
            return Err(PluginError::invalid("WebDAV URL 为空"));
        }
        let user = config.get("user").and_then(Value::as_str).unwrap_or("");
        let pass = config.get("pass").and_then(Value::as_str).unwrap_or("");
        let auth = (!user.is_empty()).then(|| {
            let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
            format!("Basic {token}")
        });
        Ok(Self { base: url.to_string(), auth })
    }

    fn list_items(&self) -> Result<Vec<ItemStat>, PluginError> {
        let mut req = self.request("PROPFIND", &format!("{}/", self.base));
        req.headers.insert("Depth".into(), "1".into());
        req.headers.insert("Content-Type".into(), "application/xml; charset=utf-8".into());
        req.body = Some(PROPFIND_BODY.as_bytes().to_vec());
        let resp = self.send(&req)?;
        // PROPFIND 正常返回 207 Multi-Status；2xx 也容忍
        if !(resp.is_success() || resp.status == 207) {
            return Err(PluginError::internal(format!("PROPFIND: HTTP {}（检查地址/账号密码）", resp.status)));
        }
        parse_propfind(&resp.body_text())
    }

    fn get_item(&self, name: &str) -> Result<String, PluginError> {
        let resp = self.send_ok(&self.request("GET", &format!("{}/{}", self.base, name)), &format!("GET {name}"))?;
        Ok(resp.body_text())
    }

    fn put_item(&self, name: &str, content: &str) -> Result<(), PluginError> {
        let mut req = self.request("PUT", &format!("{}/{}", self.base, name));
        req.headers.insert("Content-Type".into(), "text/plain; charset=utf-8".into());
        req.body = Some(content.as_bytes().to_vec());
        self.send_ok(&req, &format!("PUT {name}"))?;
        Ok(())
    }

    fn delete_item(&self, name: &str) -> Result<(), PluginError> {
        // 幂等：已不存在（404）视为成功（spec §6.5）
        let resp = self.send(&self.request("DELETE", &format!("{}/{}", self.base, name)))?;
        if resp.is_success() || resp.status == 404 {
            Ok(())
        } else {
            Err(PluginError::internal(format!("DELETE {name}: HTTP {}", resp.status)))
        }
    }

    fn get_resource(&self, resource_id: &str) -> Result<Vec<u8>, PluginError> {
        let url = format!("{}/.resource/{}", self.base, resource_id);
        let resp = self.send_ok(&self.request("GET", &url), &format!("GET resource {resource_id}"))?;
        Ok(resp.body)
    }

    fn put_resource(&self, resource_id: &str, data: &[u8]) -> Result<(), PluginError> {
        // 确保 .resource/ 存在（已存在时 MKCOL 报 405，忽略）
        let _ = self.send(&self.request("MKCOL", &format!("{}/.resource/", self.base)));
        let mut req = self.request("PUT", &format!("{}/.resource/{}", self.base, resource_id));
        req.headers.insert("Content-Type".into(), "application/octet-stream".into());
        req.body = Some(data.to_vec());
        self.send_ok(&req, &format!("PUT resource {resource_id}"))?;
        Ok(())
    }

    fn delete_resource(&self, resource_id: &str) -> Result<(), PluginError> {
        let url = format!("{}/.resource/{}", self.base, resource_id);
        let resp = self.send(&self.request("DELETE", &url))?;
        if resp.is_success() || resp.status == 404 {
            Ok(())
        } else {
            Err(PluginError::internal(format!("DELETE resource {resource_id}: HTTP {}", resp.status)))
        }
    }

    fn init_new(&self) -> Result<(), PluginError> {
        // 创建根目录与 .resource（已存在则忽略），写默认 info.json
        let _ = self.send(&self.request("MKCOL", &format!("{}/", self.base)));
        let _ = self.send(&self.request("MKCOL", &format!("{}/.resource/", self.base)));
        self.put_item("info.json", DEFAULT_INFO_JSON)
    }
}

sdk::register! { storage: WebDav }

// ---------- PROPFIND 解析（与 server/src/storage/webdav.rs 对齐）----------

/// 条目文件名：32 位 hex + `.md`。
fn is_item_filename(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 35 && name.ends_with(".md") && name[..32].chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_propfind(xml: &str) -> Result<Vec<ItemStat>, PluginError> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| PluginError::internal(format!("解析 PROPFIND XML 失败: {e}")))?;
    let mut out = Vec::new();
    for resp in doc.descendants().filter(|n| n.tag_name().name() == "response") {
        let href = resp
            .descendants()
            .find(|n| n.tag_name().name() == "href")
            .and_then(|n| n.text())
            .unwrap_or("");
        let name = href_filename(href);
        if !is_item_filename(&name) {
            continue;
        }
        let last_modified = resp
            .descendants()
            .find(|n| n.tag_name().name() == "getlastmodified")
            .and_then(|n| n.text())
            .unwrap_or("");
        out.push(ItemStat { name, updated_time: parse_http_date(last_modified) });
    }
    Ok(out)
}

fn href_filename(href: &str) -> String {
    let trimmed = href.trim_end_matches('/');
    let seg = trimmed.rsplit('/').next().unwrap_or("");
    percent_decode(seg)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 3 <= bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// RFC1123（"Tue, 15 Nov 1994 08:12:31 GMT"）→ Unix 毫秒；解析失败返回 0（宿主失去增量缓存但功能正常）。
fn parse_http_date(s: &str) -> i64 {
    if s.is_empty() {
        return 0;
    }
    chrono::NaiveDateTime::parse_from_str(s.trim(), "%a, %d %b %Y %H:%M:%S GMT")
        .map(|dt| dt.and_utc().timestamp_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdk::serde_json::json;

    #[test]
    fn config_builds_auth() {
        let w = WebDav::from_config(&json!({"url": "https://h/dav/", "user": "u", "pass": "p"})).unwrap();
        assert_eq!(w.base, "https://h/dav");
        assert_eq!(w.auth.as_deref(), Some("Basic dTpw")); // base64("u:p")
        assert!(WebDav::from_config(&json!({"url": ""})).is_err());
        // 无账号 → 无 Authorization
        assert!(WebDav::from_config(&json!({"url": "https://h/"})).unwrap().auth.is_none());
    }

    #[test]
    fn parses_propfind_and_dates() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/dav/</d:href></d:response>
  <d:response><d:href>/dav/0162e0a8c2ce4c7993b8169d530b06b6.md</d:href>
    <d:propstat><d:prop><d:getlastmodified>Tue, 15 Nov 1994 08:12:31 GMT</d:getlastmodified></d:prop></d:propstat>
  </d:response>
  <d:response><d:href>/dav/info.json</d:href></d:response>
</d:multistatus>"#;
        let items = parse_propfind(xml).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "0162e0a8c2ce4c7993b8169d530b06b6.md");
        assert!(items[0].updated_time > 0);
        assert_eq!(parse_http_date("garbage"), 0);
    }
}
