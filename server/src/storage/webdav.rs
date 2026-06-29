//! WebDAV 只读存储后端。
//! 对应 Joplin 的 WebDAV 同步目标（file-api-driver-webdav.js / WebDavApi.ts）。
//!
//! 只读只需三个动作：PROPFIND(Depth:1) 列根目录、GET 取条目、GET 取资源；
//! 认证用 HTTP Basic Auth。用同步的 ureq，避免与 tokio 运行时冲突。

use super::{is_item_filename, ItemStat, StorageBackend};
use anyhow::{anyhow, Context, Result};
use base64::Engine;
use std::io::Read;
use std::time::Duration;

pub struct WebDavStorage {
    base: String, // 规范化后无尾斜杠
    auth: Option<String>,
    agent: ureq::Agent,
}

impl WebDavStorage {
    pub fn new(url: &str, user: Option<&str>, pass: Option<&str>) -> Self {
        let base = url.trim_end_matches('/').to_string();
        let auth = user.map(|u| {
            let token = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", u, pass.unwrap_or("")));
            format!("Basic {token}")
        });
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .build();
        Self { base, auth, agent }
    }

    fn req(&self, method: &str, url: &str) -> ureq::Request {
        let r = self.agent.request(method, url);
        match &self.auth {
            Some(a) => r.set("Authorization", a),
            None => r,
        }
    }
}

const PROPFIND_BODY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<d:propfind xmlns:d="DAV:"><d:prop><d:getlastmodified/><d:resourcetype/></d:prop></d:propfind>"#;

impl StorageBackend for WebDavStorage {
    fn list_items(&self) -> Result<Vec<ItemStat>> {
        let url = format!("{}/", self.base);
        let resp = self
            .req("PROPFIND", &url)
            .set("Depth", "1")
            .set("Content-Type", "application/xml; charset=utf-8")
            .send_string(PROPFIND_BODY)
            .map_err(|e| anyhow!("PROPFIND 失败（检查地址/账号密码）: {e}"))?;
        let xml = resp.into_string().context("读取 PROPFIND 响应失败")?;
        parse_propfind(&xml)
    }

    fn get_item(&self, name: &str) -> Result<String> {
        let url = format!("{}/{}", self.base, name);
        let resp = self
            .req("GET", &url)
            .call()
            .map_err(|e| anyhow!("GET {name} 失败: {e}"))?;
        resp.into_string().with_context(|| format!("读取 {name} 失败"))
    }

    fn get_resource(&self, resource_id: &str) -> Result<Vec<u8>> {
        let url = format!("{}/.resource/{}", self.base, resource_id);
        let resp = self
            .req("GET", &url)
            .call()
            .map_err(|e| anyhow!("GET resource {resource_id} 失败: {e}"))?;
        let mut buf = Vec::new();
        resp.into_reader().read_to_end(&mut buf)?;
        Ok(buf)
    }

    fn put_item(&self, name: &str, content: &str) -> Result<()> {
        let url = format!("{}/{}", self.base, name);
        self.req("PUT", &url)
            .set("Content-Type", "text/plain; charset=utf-8")
            .send_string(content)
            .map_err(|e| anyhow!("PUT {name} 失败: {e}"))?;
        Ok(())
    }

    fn delete_item(&self, name: &str) -> Result<()> {
        let url = format!("{}/{}", self.base, name);
        self.req("DELETE", &url)
            .call()
            .map_err(|e| anyhow!("DELETE {name} 失败: {e}"))?;
        Ok(())
    }
}

/// 解析 PROPFIND 的 multistatus XML，提取根目录下的条目文件及其修改时间。
fn parse_propfind(xml: &str) -> Result<Vec<ItemStat>> {
    let doc = roxmltree::Document::parse(xml).context("解析 PROPFIND XML 失败")?;
    let mut out = Vec::new();
    // 按 local name 匹配，忽略命名空间前缀差异（d: / D: / 默认 ns）
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
        out.push(ItemStat {
            name,
            updated_time: parse_http_date(last_modified),
        });
    }
    Ok(out)
}

/// 从 href 取最后一段并做 percent 解码（文件名通常是 hex.md，无需解码，但稳妥处理）。
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

/// 解析 RFC1123 日期（如 "Tue, 15 Nov 1994 08:12:31 GMT"）为 Unix 毫秒。
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

    #[test]
    fn parses_propfind_response() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response><d:href>/dav/Joplin/</d:href>
    <d:propstat><d:prop><d:resourcetype><d:collection/></d:resourcetype></d:prop></d:propstat>
  </d:response>
  <d:response><d:href>/dav/Joplin/0162e0a8c2ce4c7993b8169d530b06b6.md</d:href>
    <d:propstat><d:prop><d:getlastmodified>Tue, 15 Nov 1994 08:12:31 GMT</d:getlastmodified></d:prop></d:propstat>
  </d:response>
  <d:response><d:href>/dav/Joplin/info.json</d:href>
    <d:propstat><d:prop><d:getlastmodified>Tue, 15 Nov 1994 08:12:31 GMT</d:getlastmodified></d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
        let items = parse_propfind(xml).unwrap();
        assert_eq!(items.len(), 1); // 只有 .md 条目，目录与 info.json 被过滤
        assert_eq!(items[0].name, "0162e0a8c2ce4c7993b8169d530b06b6.md");
        assert!(items[0].updated_time > 0);
    }

    #[test]
    fn decodes_percent_href() {
        assert_eq!(href_filename("/a/b/abc%2Edef"), "abc.def");
        assert_eq!(href_filename("https://h/x/0162e0a8c2ce4c7993b8169d530b06b6.md"),
            "0162e0a8c2ce4c7993b8169d530b06b6.md");
    }
}
