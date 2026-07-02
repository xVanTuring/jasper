//! S3 兼容对象存储插件（spec §3.9/§6.5）。
//!
//! - 网络经宿主代理的 `http.request`（能力 host:http），签名用 SigV4（纯 Rust sha2/hmac）；
//!   签名所需的当前时间来自宿主 `time.now`（沙箱内无时钟）。
//! - 一律 **path-style**（`{endpoint}/{bucket}/{key}`）：MinIO/自建服务的默认形态，AWS 亦兼容。
//! - 键布局与 Joplin 同步根一致：`{prefix}/<id>.md` + `{prefix}/.resource/<id>`（S3 无目录，天然平铺）。
//! - `DELETE` 在 S3 上对不存在的键也返回 204 → 幂等天然成立（spec §6.5）。

use hmac::{Hmac, Mac};
use jasper_plugin_sdk as sdk;
use sdk::host::{self, http_request, HttpRequest, HttpResponse};
use sdk::rt::PluginError;
use sdk::serde_json::Value;
use sdk::storage::{ItemStat, Storage};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// 全新笔记库的默认 info.json（与宿主 storage::DEFAULT_INFO_JSON 一致）。
const DEFAULT_INFO_JSON: &str = r#"{"version":3,"e2ee":{"value":false,"updatedTime":0},"activeMasterKeyId":{"value":"","updatedTime":0},"masterKeys":[],"ppk":{"value":null,"updatedTime":0},"appMinVersion":"3.0.0"}"#;

pub struct S3 {
    endpoint: String, // 规范化：无尾斜杠，含 scheme（http/https）
    region: String,
    bucket: String,
    prefix: String, // 规范化：无首尾斜杠；空 = bucket 根
    access_key: String,
    secret_key: String,
}

// ---------- 小工具：hex / sha256 / hmac ----------

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC 任意键长");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

/// SigV4 的 URI 编码：未保留字符（A-Za-z0-9 - . _ ~）原样，其余 %XX（大写）；
/// `encode_slash=false` 时保留 `/`（用于对象键路径）。
fn uri_encode(s: &str, encode_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(*b as char),
            b'/' if !encode_slash => out.push('/'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 规范化查询串（编码 + 按键排序）；同时用于签名与实际 URL（两处一致才能过签）。
fn canonical_query(query: &[(String, String)]) -> String {
    let mut pairs: Vec<(String, String)> =
        query.iter().map(|(k, v)| (uri_encode(k, true), uri_encode(v, true))).collect();
    pairs.sort();
    pairs.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&")
}

/// AWS Signature V4（对 headers 全量签名）。返回 Authorization 头的值。
#[allow(clippy::too_many_arguments)]
fn sign_v4(
    method: &str,
    canonical_uri: &str,
    query: &[(String, String)],
    headers: &[(String, String)],
    payload_hash: &str,
    amz_date: &str,   // 如 20150830T123600Z
    scope_date: &str, // 如 20150830
    region: &str,
    service: &str,
    access_key: &str,
    secret_key: &str,
) -> String {
    let mut hs: Vec<(String, String)> =
        headers.iter().map(|(k, v)| (k.to_ascii_lowercase(), v.trim().to_string())).collect();
    hs.sort();
    let canonical_headers: String = hs.iter().map(|(k, v)| format!("{k}:{v}\n")).collect();
    let signed_headers = hs.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>().join(";");
    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{}\n{canonical_headers}\n{signed_headers}\n{payload_hash}",
        canonical_query(query)
    );
    let scope = format!("{scope_date}/{region}/{service}/aws4_request");
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}",
        sha256_hex(canonical_request.as_bytes())
    );
    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), scope_date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex(&hmac_sha256(&k_signing, string_to_sign.as_bytes()));
    format!(
        "AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    )
}

impl S3 {
    /// 对象键（含前缀）。`name` 形如 `<32hex>.md` 或 `.resource/<id>`。
    fn key_of(&self, name: &str) -> String {
        if self.prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", self.prefix, name)
        }
    }

    /// list 用的键前缀（尾随 `/`；空前缀 = 空串）。
    fn list_prefix(&self) -> String {
        if self.prefix.is_empty() {
            String::new()
        } else {
            format!("{}/", self.prefix)
        }
    }

    /// 发一个 SigV4 签名请求。`key` 为对象键（空 = bucket 本身）。
    fn request(
        &self,
        method: &str,
        key: &str,
        query: &[(String, String)],
        body: Option<&[u8]>,
    ) -> Result<HttpResponse, PluginError> {
        let now = host::now_ms()?;
        let dt = chrono::DateTime::from_timestamp_millis(now)
            .ok_or_else(|| PluginError::internal("time.now 返回非法时间"))?;
        let amz_date = dt.format("%Y%m%dT%H%M%SZ").to_string();
        let scope_date = dt.format("%Y%m%d").to_string();
        let payload_hash = sha256_hex(body.unwrap_or(&[]));

        // path-style：/{bucket}/{key}（各段 SigV4 编码，键内 `/` 保留）
        let canonical_uri = if key.is_empty() {
            format!("/{}", uri_encode(&self.bucket, false))
        } else {
            format!("/{}/{}", uri_encode(&self.bucket, false), uri_encode(key, false))
        };
        // Host 由宿主按 URL 生成；签名用同一来源（endpoint 去掉 scheme）
        let host_header = self.endpoint.split("://").nth(1).unwrap_or(&self.endpoint).to_string();
        let signed = vec![
            ("host".to_string(), host_header),
            ("x-amz-content-sha256".to_string(), payload_hash.clone()),
            ("x-amz-date".to_string(), amz_date.clone()),
        ];
        let auth = sign_v4(
            method,
            &canonical_uri,
            query,
            &signed,
            &payload_hash,
            &amz_date,
            &scope_date,
            &self.region,
            "s3",
            &self.access_key,
            &self.secret_key,
        );

        let qs = canonical_query(query);
        let url = if qs.is_empty() {
            format!("{}{}", self.endpoint, canonical_uri)
        } else {
            format!("{}{}?{}", self.endpoint, canonical_uri, qs)
        };
        let mut headers = BTreeMap::new();
        headers.insert("x-amz-content-sha256".to_string(), payload_hash);
        headers.insert("x-amz-date".to_string(), amz_date);
        headers.insert("Authorization".to_string(), auth);
        http_request(&HttpRequest {
            method: method.to_string(),
            url,
            headers,
            body: body.map(|b| b.to_vec()),
            timeout_ms: None,
        })
    }

    /// 要求 2xx；404 → not_found，其余带状态码与响应片段（S3 错误是 XML，截断附上便于排查）。
    fn expect_ok(resp: HttpResponse, what: &str) -> Result<HttpResponse, PluginError> {
        if resp.is_success() {
            Ok(resp)
        } else if resp.status == 404 {
            Err(PluginError::not_found(format!("{what}: 404")))
        } else {
            let excerpt: String = resp.body_text().chars().take(200).collect();
            Err(PluginError::internal(format!("{what}: HTTP {} {excerpt}", resp.status)))
        }
    }
}

impl Storage for S3 {
    fn from_config(config: &Value) -> Result<Self, PluginError> {
        let get = |k: &str| config.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
        let endpoint = get("endpoint").trim_end_matches('/').to_string();
        if !(endpoint.starts_with("http://") || endpoint.starts_with("https://")) {
            return Err(PluginError::invalid("endpoint 须为 http(s):// URL"));
        }
        let bucket = get("bucket");
        let access_key = get("access_key");
        let secret_key = config.get("secret_key").and_then(Value::as_str).unwrap_or("").to_string();
        if bucket.is_empty() || access_key.is_empty() || secret_key.is_empty() {
            return Err(PluginError::invalid("bucket / access_key / secret_key 不能为空"));
        }
        let region = {
            let r = get("region");
            if r.is_empty() { "us-east-1".to_string() } else { r }
        };
        let prefix = get("prefix").trim_matches('/').to_string();
        Ok(Self { endpoint, region, bucket, prefix, access_key, secret_key })
    }

    fn list_items(&self) -> Result<Vec<ItemStat>, PluginError> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut query = vec![
                ("list-type".to_string(), "2".to_string()),
                ("prefix".to_string(), self.list_prefix()),
            ];
            if let Some(t) = &token {
                query.push(("continuation-token".to_string(), t.clone()));
            }
            let resp = Self::expect_ok(self.request("GET", "", &query, None)?, "ListObjectsV2")?;
            let page = parse_list(&resp.body_text(), &self.list_prefix())?;
            out.extend(page.items);
            match page.next_token {
                Some(t) => token = Some(t),
                None => break,
            }
        }
        Ok(out)
    }

    fn get_item(&self, name: &str) -> Result<String, PluginError> {
        let resp =
            Self::expect_ok(self.request("GET", &self.key_of(name), &[], None)?, &format!("GET {name}"))?;
        Ok(resp.body_text())
    }

    fn put_item(&self, name: &str, content: &str) -> Result<(), PluginError> {
        Self::expect_ok(
            self.request("PUT", &self.key_of(name), &[], Some(content.as_bytes()))?,
            &format!("PUT {name}"),
        )?;
        Ok(())
    }

    fn delete_item(&self, name: &str) -> Result<(), PluginError> {
        // S3 DELETE 对不存在的键也返回 204：幂等
        let resp = self.request("DELETE", &self.key_of(name), &[], None)?;
        if resp.is_success() || resp.status == 404 {
            Ok(())
        } else {
            Err(PluginError::internal(format!("DELETE {name}: HTTP {}", resp.status)))
        }
    }

    fn get_resource(&self, resource_id: &str) -> Result<Vec<u8>, PluginError> {
        let key = self.key_of(&format!(".resource/{resource_id}"));
        let resp = Self::expect_ok(self.request("GET", &key, &[], None)?, &format!("GET resource {resource_id}"))?;
        Ok(resp.body)
    }

    fn put_resource(&self, resource_id: &str, data: &[u8]) -> Result<(), PluginError> {
        let key = self.key_of(&format!(".resource/{resource_id}"));
        Self::expect_ok(self.request("PUT", &key, &[], Some(data))?, &format!("PUT resource {resource_id}"))?;
        Ok(())
    }

    fn delete_resource(&self, resource_id: &str) -> Result<(), PluginError> {
        let key = self.key_of(&format!(".resource/{resource_id}"));
        let resp = self.request("DELETE", &key, &[], None)?;
        if resp.is_success() || resp.status == 404 {
            Ok(())
        } else {
            Err(PluginError::internal(format!("DELETE resource {resource_id}: HTTP {}", resp.status)))
        }
    }

    fn init_new(&self) -> Result<(), PluginError> {
        // 尽力建桶（无权限/已存在则忽略——桶通常由管理员预建）；us-east-1 之外需 LocationConstraint
        let body = if self.region == "us-east-1" {
            None
        } else {
            Some(format!(
                "<CreateBucketConfiguration xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\"><LocationConstraint>{}</LocationConstraint></CreateBucketConfiguration>",
                self.region
            ))
        };
        let _ = self.request("PUT", "", &[], body.as_deref().map(str::as_bytes));
        // info.json 必须写成功（这也是连通性验证）
        self.put_item("info.json", DEFAULT_INFO_JSON)
    }
}

sdk::register! { storage: S3 }

// ---------- ListObjectsV2 XML 解析 ----------

struct ListPage {
    items: Vec<ItemStat>,
    next_token: Option<String>,
}

/// 条目文件名：32 位 hex + `.md`。
fn is_item_filename(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() == 35 && name.ends_with(".md") && name[..32].chars().all(|c| c.is_ascii_hexdigit())
}

fn parse_list(xml: &str, strip_prefix: &str) -> Result<ListPage, PluginError> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| PluginError::internal(format!("解析 ListObjectsV2 XML 失败: {e}")))?;
    let mut items = Vec::new();
    for c in doc.descendants().filter(|n| n.tag_name().name() == "Contents") {
        let key = c
            .children()
            .find(|n| n.tag_name().name() == "Key")
            .and_then(|n| n.text())
            .unwrap_or("");
        // 只认前缀根下的 <32hex>.md（.resource/ 下的键含 '/'，自然被排除）
        let Some(rest) = key.strip_prefix(strip_prefix) else { continue };
        if !is_item_filename(rest) {
            continue;
        }
        let mtime = c
            .children()
            .find(|n| n.tag_name().name() == "LastModified")
            .and_then(|n| n.text())
            .map(parse_iso_ms)
            .unwrap_or(0);
        items.push(ItemStat { name: rest.to_string(), updated_time: mtime });
    }
    let truncated = doc
        .descendants()
        .find(|n| n.tag_name().name() == "IsTruncated")
        .and_then(|n| n.text())
        .map(|t| t == "true")
        .unwrap_or(false);
    let next_token = if truncated {
        doc.descendants()
            .find(|n| n.tag_name().name() == "NextContinuationToken")
            .and_then(|n| n.text())
            .map(String::from)
    } else {
        None
    };
    Ok(ListPage { items, next_token })
}

/// ISO8601（如 "2009-10-12T17:50:30.000Z"）→ Unix 毫秒；失败返回 0（宿主失去增量缓存但功能正常）。
fn parse_iso_ms(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s.trim()).map(|dt| dt.timestamp_millis()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sdk::serde_json::json;

    /// AWS 官方 SigV4 测试向量（GET iam ListUsers，20150830T123600Z）——已知答案测试。
    /// 来源：AWS General Reference "Signature Version 4 test suite" 示例。
    #[test]
    fn sigv4_known_answer() {
        let auth = sign_v4(
            "GET",
            "/",
            &[
                ("Action".to_string(), "ListUsers".to_string()),
                ("Version".to_string(), "2010-05-08".to_string()),
            ],
            &[
                ("content-type".to_string(), "application/x-www-form-urlencoded; charset=utf-8".to_string()),
                ("host".to_string(), "iam.amazonaws.com".to_string()),
                ("x-amz-date".to_string(), "20150830T123600Z".to_string()),
            ],
            &sha256_hex(b""),
            "20150830T123600Z",
            "20150830",
            "us-east-1",
            "iam",
            "AKIDEXAMPLE",
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
        );
        assert!(
            auth.ends_with("Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"),
            "签名不匹配官方向量: {auth}"
        );
        assert!(auth.contains("SignedHeaders=content-type;host;x-amz-date"));
        assert!(auth.contains("Credential=AKIDEXAMPLE/20150830/us-east-1/iam/aws4_request"));
    }

    #[test]
    fn uri_and_query_encoding() {
        assert_eq!(uri_encode("a b/c~d", false), "a%20b/c~d");
        assert_eq!(uri_encode("a b/c", true), "a%20b%2Fc");
        // 按键排序 + 值编码
        let q = canonical_query(&[
            ("prefix".to_string(), "notes/joplin/".to_string()),
            ("list-type".to_string(), "2".to_string()),
        ]);
        assert_eq!(q, "list-type=2&prefix=notes%2Fjoplin%2F");
    }

    #[test]
    fn parses_list_page_and_pagination() {
        let xml = r#"<?xml version="1.0"?>
<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">
  <IsTruncated>true</IsTruncated>
  <NextContinuationToken>tok==</NextContinuationToken>
  <Contents>
    <Key>notes/0162e0a8c2ce4c7993b8169d530b06b6.md</Key>
    <LastModified>2009-10-12T17:50:30.000Z</LastModified>
  </Contents>
  <Contents>
    <Key>notes/info.json</Key>
    <LastModified>2009-10-12T17:50:30.000Z</LastModified>
  </Contents>
  <Contents>
    <Key>notes/.resource/0162e0a8c2ce4c7993b8169d530b06b6</Key>
    <LastModified>2009-10-12T17:50:30.000Z</LastModified>
  </Contents>
</ListBucketResult>"#;
        let page = parse_list(xml, "notes/").unwrap();
        assert_eq!(page.items.len(), 1, "info.json 与 .resource/ 应被过滤");
        assert_eq!(page.items[0].name, "0162e0a8c2ce4c7993b8169d530b06b6.md");
        assert!(page.items[0].updated_time > 0);
        assert_eq!(page.next_token.as_deref(), Some("tok=="));

        // 未截断 → 无下一页
        let done = parse_list(&xml.replace("true", "false"), "notes/").unwrap();
        assert!(done.next_token.is_none());
    }

    #[test]
    fn config_normalization() {
        let s3 = S3::from_config(&json!({
            "endpoint": "http://127.0.0.1:9000/",
            "bucket": "jasper",
            "prefix": "/notes/joplin/",
            "access_key": "ak",
            "secret_key": "sk",
        }))
        .unwrap();
        assert_eq!(s3.endpoint, "http://127.0.0.1:9000");
        assert_eq!(s3.region, "us-east-1"); // 缺省
        assert_eq!(s3.prefix, "notes/joplin");
        assert_eq!(s3.key_of("x.md"), "notes/joplin/x.md");
        assert_eq!(s3.list_prefix(), "notes/joplin/");

        // 空前缀
        let root = S3::from_config(&json!({
            "endpoint": "https://s3.example.com", "bucket": "b", "access_key": "a", "secret_key": "s",
        }))
        .unwrap();
        assert_eq!(root.key_of("x.md"), "x.md");
        assert_eq!(root.list_prefix(), "");

        // 缺必填
        assert!(S3::from_config(&json!({ "endpoint": "http://x", "bucket": "", "access_key": "a", "secret_key": "s" })).is_err());
        assert!(S3::from_config(&json!({ "endpoint": "ftp://x", "bucket": "b", "access_key": "a", "secret_key": "s" })).is_err());
    }
}
