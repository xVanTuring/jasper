//! PluginStorage：把插件的 `storage.*` dispatch（spec §6.5，0.2）适配成 `StorageBackend`。
//! 每次调用经 runtime 新建实例 → rayon par_iter 并发下天然安全（无共享可变状态）。

use super::manifest::Schema;
use super::runtime::CallClass;
use super::PluginHost;
use crate::config::SourceConfig;
use crate::storage::{is_item_filename, ItemStat, StorageBackend};
use anyhow::{anyhow, Result};
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

pub struct PluginStorage {
    host: Arc<PluginHost>,
    plugin_id: String,
    storage_id: String,
    config: Value,
}

impl PluginStorage {
    pub fn new(host: Arc<PluginHost>, plugin_id: String, storage_id: String, config: Value) -> Self {
        Self { host, plugin_id, storage_id, config }
    }

    fn call(&self, op: &str, extra: Value) -> Result<Value> {
        let mut params = json!({ "storage": self.storage_id, "config": self.config });
        if let (Some(obj), Some(ex)) = (params.as_object_mut(), extra.as_object()) {
            for (k, v) in ex {
                obj.insert(k.clone(), v.clone());
            }
        }
        self.host
            .dispatch(&self.plugin_id, &format!("storage.{op}"), params, CallClass::Storage)
            .map_err(|e| anyhow!("插件存储 {}/{} {op}: {e}", self.plugin_id, self.storage_id))
    }

    fn expect_str(v: &Value, key: &str) -> Result<String> {
        v.get(key)
            .and_then(Value::as_str)
            .map(String::from)
            .ok_or_else(|| anyhow!("插件存储响应缺 {key}"))
    }
}

impl StorageBackend for PluginStorage {
    fn list_items(&self) -> Result<Vec<ItemStat>> {
        let r = self.call("list_items", json!({}))?;
        let items = r
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("list_items 响应缺 items"))?;
        Ok(items
            .iter()
            .filter_map(|it| {
                let name = it.get("name")?.as_str()?.to_string();
                // 与内置后端一致：只认 <32hex>.md（插件多报的杂项在此兜底过滤）
                if !is_item_filename(&name) {
                    return None;
                }
                let updated_time = it.get("updated_time").and_then(Value::as_i64).unwrap_or(0);
                Some(ItemStat { name, updated_time })
            })
            .collect())
    }

    fn get_item(&self, name: &str) -> Result<String> {
        let r = self.call("get_item", json!({ "name": name }))?;
        Self::expect_str(&r, "content")
    }

    fn get_resource(&self, resource_id: &str) -> Result<Vec<u8>> {
        let r = self.call("get_resource", json!({ "resource_id": resource_id }))?;
        let b64 = Self::expect_str(&r, "data_b64")?;
        base64::engine::general_purpose::STANDARD
            .decode(b64)
            .map_err(|e| anyhow!("data_b64 解码失败: {e}"))
    }

    fn put_resource(&self, resource_id: &str, bytes: &[u8]) -> Result<()> {
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        self.call("put_resource", json!({ "resource_id": resource_id, "data_b64": b64 }))?;
        Ok(())
    }

    fn delete_resource(&self, resource_id: &str) -> Result<()> {
        self.call("delete_resource", json!({ "resource_id": resource_id }))?;
        Ok(())
    }

    fn put_item(&self, name: &str, content: &str) -> Result<()> {
        self.call("put_item", json!({ "name": name, "content": content }))?;
        Ok(())
    }

    fn delete_item(&self, name: &str) -> Result<()> {
        self.call("delete_item", json!({ "name": name }))?;
        Ok(())
    }

    fn init_new(&self) -> Result<()> {
        self.call("init_new", json!({}))?;
        Ok(())
    }
}

/// 校验并规范化插件数据源配置（PUT /api/config 的 source_type=="plugin" 分支）：
/// - 插件须存在、已启用、含该存储贡献；
/// - `plugin_config` 按 config_schema 做类型/必填/未知键校验；
/// - 写回规范化 JSON（键排序）+ 计算不含 secret 的 `plugin_config_key`（缓存隔离键）。
pub fn prepare_plugin_source(cfg: &mut SourceConfig, host: Option<&Arc<PluginHost>>) -> Result<(), String> {
    let host = host.ok_or("插件宿主不可用")?;
    if cfg.plugin_id.is_empty() || cfg.plugin_storage.is_empty() {
        return Err("缺 plugin_id / plugin_storage".into());
    }
    let contribution = host
        .storage_contribution(&cfg.plugin_id, &cfg.plugin_storage)
        .ok_or_else(|| format!("插件 {} 未启用或无存储贡献 {}", cfg.plugin_id, cfg.plugin_storage))?;
    let schema: &Schema = &contribution.config_schema;

    let raw: BTreeMap<String, Value> = if cfg.plugin_config.trim().is_empty() {
        BTreeMap::new()
    } else {
        serde_json::from_str(&cfg.plugin_config).map_err(|e| format!("plugin_config 不是 JSON 对象: {e}"))?
    };

    for (k, v) in &raw {
        let Some(f) = schema.get(k) else {
            return Err(format!("未知配置项: {k}"));
        };
        let ok = match f.field_type.as_str() {
            "string" | "secret" | "multiline" => v.is_string(),
            "select" => v
                .as_str()
                .map(|s| f.options.as_ref().map(|o| o.iter().any(|x| x == s)).unwrap_or(false))
                .unwrap_or(false),
            "bool" => v.is_boolean(),
            "number" => v.is_number(),
            _ => false,
        };
        if !ok {
            return Err(format!("配置项 {k} 类型不符（应为 {}）", f.field_type));
        }
    }
    for (k, f) in schema {
        if f.required == Some(true) {
            let missing = raw
                .get(k)
                .map(|v| v.as_str().map(|s| s.trim().is_empty()).unwrap_or(false))
                .unwrap_or(true);
            if missing {
                return Err(format!("缺必填配置项: {k}"));
            }
        }
    }

    // BTreeMap 序列化 = 键排序 → 规范化文本
    cfg.plugin_config = serde_json::to_string(&raw).map_err(|e| e.to_string())?;
    let non_secret: BTreeMap<&String, &Value> = raw
        .iter()
        .filter(|(k, _)| schema.get(*k).map(|f| f.field_type != "secret").unwrap_or(true))
        .collect();
    cfg.plugin_config_key = serde_json::to_string(&non_secret).map_err(|e| e.to_string())?;
    Ok(())
}

/// build_storage 的 "plugin" 分支（config.rs 调用）。
pub fn build_plugin_storage(
    cfg: &SourceConfig,
    host: Option<&Arc<PluginHost>>,
) -> Result<Arc<dyn StorageBackend>> {
    let host = host.ok_or_else(|| anyhow!("插件宿主不可用"))?;
    host.storage_contribution(&cfg.plugin_id, &cfg.plugin_storage)
        .ok_or_else(|| anyhow!("插件 {} 未启用或无存储贡献 {}", cfg.plugin_id, cfg.plugin_storage))?;
    let config: Value = if cfg.plugin_config.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(&cfg.plugin_config)?
    };
    Ok(Arc::new(PluginStorage::new(
        host.clone(),
        cfg.plugin_id.clone(),
        cfg.plugin_storage.clone(),
        config,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigStore;
    use std::sync::Mutex;

    /// 起一个带存储贡献 manifest 的宿主（wasm 用 testbed 的——不会真调 storage.*）。
    fn host_with_storage_manifest() -> Option<(tempfile::TempDir, Arc<PluginHost>)> {
        let wasm = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../plugins-examples/testbed/plugin.wasm");
        if !wasm.exists() {
            eprintln!("跳过：testbed/plugin.wasm 未构建");
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let pdir = dir.path().join("stor");
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(
            pdir.join("manifest.toml"),
            r#"id = "stor"
name = "Stor"
version = "1"
apiVersion = "0.2"
[backend]
wasm = "plugin.wasm"
capabilities = ["host:http"]
[[contributes.storage]]
id = "s1"
name = "S1"
[contributes.storage.config_schema]
url  = { type = "string", required = true }
user = { type = "string" }
pass = { type = "secret" }
mode = { type = "select", options = ["a", "b"] }
"#,
        )
        .unwrap();
        std::fs::copy(&wasm, pdir.join("plugin.wasm")).unwrap();
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(dir.path().to_path_buf(), config).unwrap();
        host.set_enabled("stor", true).unwrap();
        Some((dir, host))
    }

    fn plugin_cfg(config_json: &str) -> SourceConfig {
        SourceConfig {
            source_type: "plugin".into(),
            plugin_id: "stor".into(),
            plugin_storage: "s1".into(),
            plugin_config: config_json.into(),
            ..Default::default()
        }
    }

    #[test]
    fn prepare_validates_and_normalizes() {
        let Some((_dir, host)) = host_with_storage_manifest() else { return };
        let host = Some(&host);

        // 合法：规范化 + config_key 剔除 secret
        let mut cfg = plugin_cfg(r#"{"url":"https://x/","pass":"s3cret","user":"u"}"#);
        prepare_plugin_source(&mut cfg, host).unwrap();
        assert_eq!(cfg.plugin_config, r#"{"pass":"s3cret","url":"https://x/","user":"u"}"#);
        assert!(!cfg.plugin_config_key.contains("s3cret"), "缓存键不含 secret");
        assert!(cfg.plugin_config_key.contains("https://x/"));

        // 缺必填
        let mut cfg = plugin_cfg(r#"{"user":"u"}"#);
        assert!(prepare_plugin_source(&mut cfg, host).unwrap_err().contains("url"));
        // 未知键
        let mut cfg = plugin_cfg(r#"{"url":"x","nope":1}"#);
        assert!(prepare_plugin_source(&mut cfg, host).unwrap_err().contains("nope"));
        // 类型不符
        let mut cfg = plugin_cfg(r#"{"url":123}"#);
        assert!(prepare_plugin_source(&mut cfg, host).is_err());
        // select 不在 options
        let mut cfg = plugin_cfg(r#"{"url":"x","mode":"z"}"#);
        assert!(prepare_plugin_source(&mut cfg, host).is_err());
        // 贡献不存在
        let mut cfg = plugin_cfg(r#"{"url":"x"}"#);
        cfg.plugin_storage = "nope".into();
        assert!(prepare_plugin_source(&mut cfg, host).is_err());
    }

    #[test]
    fn dispatch_unsupported_storage_method_errors_cleanly() {
        // testbed 不实现 storage.* → PluginStorage 各方法应返回错误而非 panic
        let Some((_dir, host)) = host_with_storage_manifest() else { return };
        let cfg = plugin_cfg(r#"{"url":"x"}"#);
        let storage = build_plugin_storage(&cfg, Some(&host)).unwrap();
        let err = storage.list_items().unwrap_err().to_string();
        assert!(err.contains("unsupported") || err.contains("未知"), "{err}");
    }

    /// 集成测试：webdav 插件 vs hacdias/webdav 容器（docker-compose.dev.yml，joplin/joplin）。
    /// 覆盖 8 方法往返、与内置 WebDavStorage 的等价对照、build_cached 的 rayon 并发与增量命中。
    /// 未设 JASPER_TEST_WEBDAV_URL 或未构建 plugin.wasm 时自动跳过（CI 安全）。
    #[test]
    fn webdav_plugin_round_trip_against_container() {
        let Ok(base) = std::env::var("JASPER_TEST_WEBDAV_URL") else {
            eprintln!("跳过：未设 JASPER_TEST_WEBDAV_URL（docker compose -f docker-compose.dev.yml up -d）");
            return;
        };
        let examples = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples/webdav-storage");
        if !examples.join("plugin.wasm").exists() {
            eprintln!("跳过：webdav-storage/plugin.wasm 未构建（先跑 plugins-examples/build-wasm.sh）");
            return;
        }
        let user = std::env::var("JASPER_WEBDAV_USER").unwrap_or_else(|_| "joplin".into());
        let pass = std::env::var("JASPER_WEBDAV_PASS").unwrap_or_else(|_| "joplin".into());
        // 唯一子目录：测试彼此隔离、可重复跑
        let unique = format!(
            "plugin-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
        );
        let sub_url = format!("{}/{}", base.trim_end_matches('/'), unique);

        // 装好并启用插件
        let plug_dir = tempfile::tempdir().unwrap();
        let dst = plug_dir.path().join("webdav-storage");
        std::fs::create_dir_all(&dst).unwrap();
        for f in ["manifest.toml", "plugin.wasm"] {
            std::fs::copy(examples.join(f), dst.join(f)).unwrap();
        }
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(plug_dir.path().to_path_buf(), config).unwrap();
        host.set_enabled("webdav-storage", true).unwrap();

        let mut cfg = SourceConfig {
            source_type: "plugin".into(),
            plugin_id: "webdav-storage".into(),
            plugin_storage: "webdav".into(),
            plugin_config: serde_json::json!({ "url": sub_url, "user": user, "pass": pass }).to_string(),
            ..Default::default()
        };
        prepare_plugin_source(&mut cfg, Some(&host)).unwrap();
        let storage = build_plugin_storage(&cfg, Some(&host)).unwrap();

        // init_new + 条目读写
        storage.init_new().unwrap();
        let note_id = crate::serialize::new_id();
        let name = format!("{note_id}.md");
        let content = crate::serialize::new_note_md(&note_id, "", "插件笔记", "正文 via plugin", false, crate::serialize::now_ms());
        storage.put_item(&name, &content).unwrap();
        let items = storage.list_items().unwrap();
        let it = items.iter().find(|i| i.name == name).expect("列表应包含新条目");
        assert!(it.updated_time > 0, "PROPFIND 应带真实 mtime");
        assert_eq!(storage.get_item(&name).unwrap(), content);

        // 资源读写 + 删除幂等
        let res_id = crate::serialize::new_id();
        let bytes: Vec<u8> = (0..=255u8).collect();
        storage.put_resource(&res_id, &bytes).unwrap();
        assert_eq!(storage.get_resource(&res_id).unwrap(), bytes);
        storage.delete_resource(&res_id).unwrap();
        storage.delete_resource(&res_id).unwrap(); // 已删除再删 = 成功

        // 等价对照：内置 WebDavStorage 在同一子树上看到一致的世界
        let builtin = crate::storage::webdav::WebDavStorage::new(&sub_url, Some(&user), Some(&pass));
        let names = |v: Vec<crate::storage::ItemStat>| {
            let mut n: Vec<String> = v.into_iter().map(|i| i.name).collect();
            n.sort();
            n
        };
        assert_eq!(names(storage.list_items().unwrap()), names(builtin.list_items().unwrap()));
        assert_eq!(builtin.get_item(&name).unwrap(), storage.get_item(&name).unwrap());

        // build_cached：rayon 并发拉取 + 二跑全量缓存命中
        for i in 0..10 {
            let id = crate::serialize::new_id();
            let md = crate::serialize::new_note_md(&id, "", &format!("批量 {i}"), "b", false, crate::serialize::now_ms());
            storage.put_item(&format!("{id}.md"), &md).unwrap();
        }
        let cache = crate::cache::CacheStore::in_memory().unwrap();
        let key = crate::config::source_key(&cfg);
        let (lib1, s1) = crate::indexer::build_cached(storage.as_ref(), &cache, &key).unwrap();
        assert_eq!(lib1.notes.len(), 11);
        assert!(s1.fetched >= 11, "首跑应实际拉取: {s1:?}");
        let (_lib2, s2) = crate::indexer::build_cached(storage.as_ref(), &cache, &key).unwrap();
        assert_eq!(s2.fetched, 0, "二跑应全量缓存命中: {s2:?}");
        assert_eq!(s2.cached, s1.fetched + s1.cached);

        // 清理并验证 delete_item 幂等
        for it in storage.list_items().unwrap() {
            storage.delete_item(&it.name).unwrap();
        }
        storage.delete_item(&name).unwrap(); // 再删已删的 = 成功
        assert!(storage.list_items().unwrap().is_empty());
    }

    /// 集成测试：s3-storage 插件 vs MinIO 容器（docker-compose.dev.yml，minioadmin/minioadmin）。
    /// init_new 尽力 CreateBucket → 8 方法往返 → build_cached 增量。
    /// 未设 JASPER_TEST_S3_URL 或未构建 plugin.wasm 时自动跳过（CI 安全）。
    #[test]
    fn s3_plugin_round_trip_against_minio() {
        let Ok(endpoint) = std::env::var("JASPER_TEST_S3_URL") else {
            eprintln!("跳过：未设 JASPER_TEST_S3_URL（docker compose -f docker-compose.dev.yml up -d 后设 http://127.0.0.1:9000）");
            return;
        };
        let examples =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples/s3-storage");
        if !examples.join("plugin.wasm").exists() {
            eprintln!("跳过：s3-storage/plugin.wasm 未构建（先跑 plugins-examples/build-wasm.sh）");
            return;
        }
        let access_key = std::env::var("JASPER_TEST_S3_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".into());
        let secret_key = std::env::var("JASPER_TEST_S3_SECRET_KEY").unwrap_or_else(|_| "minioadmin".into());
        // 桶名唯一（S3 桶名规则：小写+数字+连字符），前缀再叠一层验证键前缀逻辑
        let unique = format!(
            "jasper-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
        );

        let plug_dir = tempfile::tempdir().unwrap();
        let dst = plug_dir.path().join("s3-storage");
        std::fs::create_dir_all(&dst).unwrap();
        for f in ["manifest.toml", "plugin.wasm"] {
            std::fs::copy(examples.join(f), dst.join(f)).unwrap();
        }
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(plug_dir.path().to_path_buf(), config).unwrap();
        host.set_enabled("s3-storage", true).unwrap();

        let mut cfg = SourceConfig {
            source_type: "plugin".into(),
            plugin_id: "s3-storage".into(),
            plugin_storage: "s3".into(),
            plugin_config: serde_json::json!({
                "endpoint": endpoint.trim_end_matches('/'),
                "region": "us-east-1",
                "bucket": unique,
                "prefix": "notes/joplin",
                "access_key": access_key,
                "secret_key": secret_key,
            })
            .to_string(),
            ..Default::default()
        };
        prepare_plugin_source(&mut cfg, Some(&host)).unwrap();
        let storage = build_plugin_storage(&cfg, Some(&host)).unwrap();

        // init_new：建桶（MinIO 上尽力）+ 写 info.json
        storage.init_new().unwrap();

        // 条目读写 + 列表带真实 mtime
        let note_id = crate::serialize::new_id();
        let name = format!("{note_id}.md");
        let content =
            crate::serialize::new_note_md(&note_id, "", "S3 笔记", "正文 via s3 plugin", false, crate::serialize::now_ms());
        storage.put_item(&name, &content).unwrap();
        let items = storage.list_items().unwrap();
        let it = items.iter().find(|i| i.name == name).expect("列表应包含新条目");
        assert!(it.updated_time > 0, "ListObjectsV2 应带真实 LastModified");
        assert_eq!(storage.get_item(&name).unwrap(), content);

        // 资源读写 + 删除幂等
        let res_id = crate::serialize::new_id();
        let bytes: Vec<u8> = (0..=255u8).cycle().take(70_000).collect(); // 跨 base64 分块边界的体积
        storage.put_resource(&res_id, &bytes).unwrap();
        assert_eq!(storage.get_resource(&res_id).unwrap(), bytes);
        storage.delete_resource(&res_id).unwrap();
        storage.delete_resource(&res_id).unwrap();

        // build_cached：首拉 + 二跑全量命中
        for i in 0..5 {
            let id = crate::serialize::new_id();
            let md = crate::serialize::new_note_md(&id, "", &format!("批量 {i}"), "b", false, crate::serialize::now_ms());
            storage.put_item(&format!("{id}.md"), &md).unwrap();
        }
        let cache = crate::cache::CacheStore::in_memory().unwrap();
        let key = crate::config::source_key(&cfg);
        // 注意不能断言「不含 secret 值」——MinIO 默认凭据 access==secret，会误伤；按字段断言
        let key_fields: serde_json::Value = serde_json::from_str(&cfg.plugin_config_key).unwrap();
        assert!(key_fields.get("secret_key").is_none(), "缓存键不得含 secret_key 字段");
        assert!(key_fields.get("bucket").is_some(), "非 secret 字段应进缓存键");
        let (lib1, s1) = crate::indexer::build_cached(storage.as_ref(), &cache, &key).unwrap();
        assert_eq!(lib1.notes.len(), 6);
        let (_lib2, s2) = crate::indexer::build_cached(storage.as_ref(), &cache, &key).unwrap();
        assert_eq!(s2.fetched, 0, "二跑应全量缓存命中: {s2:?}");
        assert_eq!(s2.cached, s1.fetched + s1.cached);

        // 清理条目（桶留着——删桶需先清空全部键含 info.json，测试桶名唯一不碍事）
        for it in storage.list_items().unwrap() {
            storage.delete_item(&it.name).unwrap();
        }
        assert!(storage.list_items().unwrap().is_empty());
    }
}
