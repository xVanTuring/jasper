//! PluginHost：插件注册表 + 生命周期（spec §5）+ dispatch 入口。
//! Send+Sync：Engine/Module 可跨线程共享，每次调用新建 Store（runtime.rs）。

use super::install::{self, InstallError};
use super::manifest::{self, Manifest, Schema};
use super::runtime::{self, CallClass, CallError, HostCtx, PluginLimits};
use crate::config::ConfigStore;
use anyhow::Result;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use wasmi::{Engine, Module};

pub struct LoadedPlugin {
    pub id: String,
    pub dir: PathBuf,
    pub manifest: Option<Manifest>,
    pub error: Option<String>,
    pub enabled: bool,
    pub granted_caps: Vec<String>,
    pub module: Option<Arc<Module>>,
}

/// GET /api/plugins 的条目形状（也是 install/enable 的返回体）。
#[derive(Debug, Clone, Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub has_backend: bool,
    pub capabilities: Vec<String>,
    pub hooks: Vec<String>,
    pub error: Option<String>,
    pub contributes: manifest::Contributes,
    pub settings_schema: Schema,
}

#[derive(thiserror::Error, Debug)]
pub enum HostOpError {
    #[error("插件不存在")]
    NotFound,
    #[error("插件为当前数据源所用，先切换数据源")]
    InUse,
    #[error("{0}")]
    Invalid(String),
    #[error("wasm 加载失败: {0}")]
    Wasm(String),
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

pub struct PluginHost {
    engine: Engine,
    plugins: RwLock<BTreeMap<String, LoadedPlugin>>,
    config: Arc<Mutex<ConfigStore>>,
    pub dir: PathBuf,
    limits: PluginLimits,
    http: ureq::Agent,
}

impl PluginHost {
    /// 默认位置：`<config>/plugins/`。
    pub fn init(config: Arc<Mutex<ConfigStore>>) -> Result<Arc<Self>> {
        let dir = crate::config::config_base_dir()?.join("plugins");
        Self::init_at(dir, config)
    }

    pub fn init_at(dir: PathBuf, config: Arc<Mutex<ConfigStore>>) -> Result<Arc<Self>> {
        Self::init_at_with_limits(dir, config, PluginLimits::default())
    }

    pub fn init_at_with_limits(
        dir: PathBuf,
        config: Arc<Mutex<ConfigStore>>,
        limits: PluginLimits,
    ) -> Result<Arc<Self>> {
        std::fs::create_dir_all(&dir)?;
        let host = Arc::new(Self {
            engine: runtime::new_engine(),
            plugins: RwLock::new(BTreeMap::new()),
            config,
            dir,
            limits,
            http: ureq::AgentBuilder::new().redirects(5).build(),
        });
        host.rescan();
        Ok(host)
    }

    /// 扫描插件目录，重建注册表（保留 config 里的启停状态）。
    pub fn rescan(&self) {
        let mut map = BTreeMap::new();
        for (id, dir, parsed) in install::scan(&self.dir) {
            map.insert(id.clone(), self.load_one(id, dir, parsed));
        }
        *self.plugins.write().unwrap() = map;
    }

    fn load_one(&self, id: String, dir: PathBuf, parsed: Result<Manifest>) -> LoadedPlugin {
        match parsed {
            Err(e) => LoadedPlugin {
                id,
                dir,
                manifest: None,
                error: Some(e.to_string()),
                enabled: false,
                granted_caps: Vec::new(),
                module: None,
            },
            Ok(m) => {
                let state = self.config.lock().unwrap().plugin_state(&id);
                let (mut enabled, granted_caps) = match state {
                    Some((en, caps)) => (en, caps),
                    // 首次发现：零代码自动启用；含 backend 默认禁用（enable=授权，spec §5）
                    None => (m.is_zero_code(), Vec::new()),
                };
                // 能力集扩大后不得沿用旧授权：强制回到禁用态等待重新确认
                if enabled && m.has_backend() {
                    let caps_ok = m.capabilities().iter().all(|c| granted_caps.contains(c));
                    if !caps_ok {
                        enabled = false;
                    }
                }
                let mut error = None;
                let mut module = None;
                if enabled && m.has_backend() {
                    let wasm = dir.join(&m.backend.as_ref().unwrap().wasm);
                    match runtime::compile(&self.engine, &wasm) {
                        Ok(mo) => module = Some(mo),
                        Err(e) => {
                            error = Some(e.to_string());
                            enabled = false;
                        }
                    }
                }
                LoadedPlugin { id, dir, manifest: Some(m), error, enabled, granted_caps, module }
            }
        }
    }

    fn info_of(p: &LoadedPlugin) -> PluginInfo {
        match &p.manifest {
            Some(m) => PluginInfo {
                id: p.id.clone(),
                name: m.name.clone(),
                version: m.version.clone(),
                api_version: m.api_version.clone(),
                description: m.description.clone(),
                author: m.author.clone(),
                enabled: p.enabled,
                has_backend: m.has_backend(),
                capabilities: m.capabilities().to_vec(),
                hooks: m.hooks().to_vec(),
                error: p.error.clone(),
                contributes: m.contributes.clone(),
                settings_schema: m.settings.schema.clone(),
            },
            None => PluginInfo {
                id: p.id.clone(),
                name: p.id.clone(),
                version: String::new(),
                api_version: String::new(),
                description: String::new(),
                author: String::new(),
                enabled: false,
                has_backend: false,
                capabilities: Vec::new(),
                hooks: Vec::new(),
                error: p.error.clone(),
                contributes: Default::default(),
                settings_schema: Default::default(),
            },
        }
    }

    pub fn list_info(&self) -> Vec<PluginInfo> {
        self.plugins.read().unwrap().values().map(Self::info_of).collect()
    }

    /// 当前数据源是否引用该插件（存储 provider in_use 守护）。
    fn in_use_by_source(&self, id: &str) -> bool {
        self.config
            .lock()
            .unwrap()
            .load()
            .map(|c| c.source_type == "plugin" && c.plugin_id == id)
            .unwrap_or(false)
    }

    /// 安装 zip；返回信息。若旧状态为启用且新 manifest 能力集未超出已授权范围则保持启用，
    /// 否则回到禁用态等待重新授权。
    pub fn install(&self, bytes: &[u8], force: bool) -> Result<PluginInfo, InstallError> {
        let m = install::install_zip(&self.dir, bytes, force)?;
        let id = m.id.clone();
        let dir = self.dir.join(&id);
        // 零代码插件首次安装即启用（写状态，保证重启后稳定）
        if self.config.lock().unwrap().plugin_state(&id).is_none() && m.is_zero_code() {
            let _ = self.config.lock().unwrap().set_plugin_state(&id, true, &[]);
        }
        let loaded = self.load_one(id.clone(), dir, Ok(m));
        let info = Self::info_of(&loaded);
        self.plugins.write().unwrap().insert(id, loaded);
        Ok(info)
    }

    pub fn uninstall(&self, id: &str) -> Result<(), HostOpError> {
        if !self.plugins.read().unwrap().contains_key(id) {
            return Err(HostOpError::NotFound);
        }
        if self.in_use_by_source(id) {
            return Err(HostOpError::InUse);
        }
        install::uninstall(&self.dir, id)?;
        self.plugins.write().unwrap().remove(id);
        let store = self.config.lock().unwrap();
        store.remove_plugin(id).ok();
        Ok(())
    }

    /// 启停。enable = 能力授权动作：granted_caps 更新为 manifest 当前申请的能力集。
    pub fn set_enabled(&self, id: &str, enabled: bool) -> Result<PluginInfo, HostOpError> {
        let mut map = self.plugins.write().unwrap();
        let p = map.get_mut(id).ok_or(HostOpError::NotFound)?;
        let m = p
            .manifest
            .clone()
            .ok_or_else(|| HostOpError::Invalid(p.error.clone().unwrap_or_else(|| "manifest 无效".into())))?;
        if !enabled && self.in_use_by_source(id) {
            return Err(HostOpError::InUse);
        }
        if enabled && m.has_backend() && p.module.is_none() {
            let wasm = p.dir.join(&m.backend.as_ref().unwrap().wasm);
            match runtime::compile(&self.engine, &wasm) {
                Ok(mo) => {
                    p.module = Some(mo);
                    p.error = None;
                }
                Err(e) => {
                    p.error = Some(e.to_string());
                    return Err(HostOpError::Wasm(e.to_string()));
                }
            }
        }
        p.enabled = enabled;
        p.granted_caps = if enabled { m.capabilities().to_vec() } else { p.granted_caps.clone() };
        self.config
            .lock()
            .unwrap()
            .set_plugin_state(id, p.enabled, &p.granted_caps)
            .map_err(HostOpError::Other)?;
        Ok(Self::info_of(p))
    }

    /// 执行插件的 plugin_dispatch（阻塞；调用方负责 spawn_blocking / rayon 上下文）。
    pub fn dispatch(
        &self,
        plugin_id: &str,
        method: &str,
        params: Value,
        class: CallClass,
    ) -> Result<Value, CallError> {
        let (module, caps) = {
            let map = self.plugins.read().unwrap();
            let p = map
                .get(plugin_id)
                .ok_or_else(|| CallError::Fatal(format!("插件不存在: {plugin_id}")))?;
            if !p.enabled {
                return Err(CallError::Fatal(format!("插件未启用: {plugin_id}")));
            }
            let module = p
                .module
                .clone()
                .ok_or_else(|| CallError::Fatal(format!("插件无后端模块: {plugin_id}")))?;
            (module, p.granted_caps.clone())
        };
        let ctx = HostCtx {
            plugin_id: plugin_id.to_string(),
            caps,
            limits: wasmi::StoreLimitsBuilder::new().build(), // call_dispatch 按档位重设
            io_time: std::time::Duration::ZERO,
            config: self.config.clone(),
            http: self.http.clone(),
            http_response_cap: self.limits.http_response_cap,
        };
        runtime::call_dispatch(&self.engine, &module, ctx, method, params, class, &self.limits)
    }

    /// 订阅 before-save 的已启用插件 id（BTreeMap 序 = 加载顺序，spec §8）。
    pub fn before_save_plugins(&self) -> Vec<String> {
        self.plugins
            .read()
            .unwrap()
            .values()
            .filter(|p| p.enabled && p.module.is_some())
            .filter(|p| p.manifest.as_ref().map(|m| m.hooks().contains(&"before-save".to_string())).unwrap_or(false))
            .map(|p| p.id.clone())
            .collect()
    }

    /// 读插件设置：secret 值不回显，仅以 secret_set 标记（spec §10）。
    pub fn settings_values(&self, id: &str) -> Result<(Value, Value), HostOpError> {
        let map = self.plugins.read().unwrap();
        let p = map.get(id).ok_or(HostOpError::NotFound)?;
        let schema = p.manifest.as_ref().map(|m| m.settings.schema.clone()).unwrap_or_default();
        let stored = self.config.lock().unwrap().plugin_settings(id);
        let mut values = serde_json::Map::new();
        let mut secret_set = serde_json::Map::new();
        for (k, v) in stored {
            let is_secret = schema.get(&k).map(|f| f.field_type == "secret").unwrap_or(false);
            if is_secret {
                secret_set.insert(k, Value::Bool(true));
            } else if let Ok(v) = serde_json::from_str::<Value>(&v) {
                values.insert(k, v);
            }
        }
        Ok((Value::Object(values), Value::Object(secret_set)))
    }

    /// 写插件设置：只接受 schema 里声明过的键；secret 空串 = 清除；未提交的键不动。
    pub fn set_settings(&self, id: &str, values: &serde_json::Map<String, Value>) -> Result<(), HostOpError> {
        let schema: Schema = {
            let map = self.plugins.read().unwrap();
            let p = map.get(id).ok_or(HostOpError::NotFound)?;
            p.manifest.as_ref().map(|m| m.settings.schema.clone()).unwrap_or_default()
        };
        let store = self.config.lock().unwrap();
        for (k, v) in values {
            let Some(field) = schema.get(k) else {
                return Err(HostOpError::Invalid(format!("未知设置项: {k}")));
            };
            let clear_secret = field.field_type == "secret" && v.as_str().map(|s| s.is_empty()).unwrap_or(false);
            if clear_secret || v.is_null() {
                store.set_plugin_setting(id, k, None).map_err(HostOpError::Other)?;
            } else {
                let text = serde_json::to_string(v).map_err(|e| HostOpError::Invalid(e.to_string()))?;
                store.set_plugin_setting(id, k, Some(&text)).map_err(HostOpError::Other)?;
            }
        }
        Ok(())
    }

    /// 取某插件的存储贡献（M6 存储适配用）。
    pub fn storage_contribution(&self, plugin_id: &str, storage_id: &str) -> Option<manifest::StorageContribution> {
        let map = self.plugins.read().unwrap();
        let p = map.get(plugin_id)?;
        if !p.enabled {
            return None;
        }
        p.manifest
            .as_ref()?
            .contributes
            .storage
            .iter()
            .find(|s| s.id == storage_id)
            .cloned()
    }

    /// 该插件是否有已启用的 backend 命令 `cmd`（commands 端点的前置校验）。
    pub fn has_backend_command(&self, plugin_id: &str, cmd: &str) -> bool {
        let map = self.plugins.read().unwrap();
        let Some(p) = map.get(plugin_id) else { return false };
        p.enabled
            && p.module.is_some()
            && p.manifest
                .as_ref()
                .map(|m| m.contributes.command.iter().any(|c| c.id == cmd && c.target == "backend"))
                .unwrap_or(false)
    }

    /// 插件目录（资产托管用）；返回前须确认 enabled。
    pub fn asset_root(&self, plugin_id: &str) -> Option<PathBuf> {
        let map = self.plugins.read().unwrap();
        let p = map.get(plugin_id)?;
        if !p.enabled {
            return None;
        }
        Some(p.dir.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::runtime::CallClass;
    use serde_json::json;
    use std::time::Duration;

    fn examples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples")
    }

    /// 把 plugins-examples/<name> 复制成已安装形态；plugin.wasm 缺失时返回 None（测试跳过）。
    fn install_example(host_dir: &std::path::Path, name: &str) -> Option<()> {
        let src = examples_dir().join(name);
        if !src.join("plugin.wasm").exists() {
            eprintln!("跳过：{name}/plugin.wasm 未构建（先跑 plugins-examples/build-wasm.sh）");
            return None;
        }
        let dst = host_dir.join(name);
        std::fs::create_dir_all(&dst).unwrap();
        for f in ["manifest.toml", "plugin.wasm"] {
            std::fs::copy(src.join(f), dst.join(f)).unwrap();
        }
        Some(())
    }

    fn tiny_limits() -> PluginLimits {
        PluginLimits {
            normal_memory: 16 * 1024 * 1024,
            normal_fuel: 200_000_000,
            normal_cpu: Duration::from_millis(800),
            storage_memory: 16 * 1024 * 1024,
            storage_fuel: 200_000_000,
            storage_cpu: Duration::from_millis(800),
            fuel_slice: 10_000_000,
            http_response_cap: 1024 * 1024,
        }
    }

    fn host_with_testbed() -> Option<(tempfile::TempDir, Arc<PluginHost>)> {
        let dir = tempfile::tempdir().unwrap();
        install_example(dir.path(), "testbed")?;
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host =
            PluginHost::init_at_with_limits(dir.path().to_path_buf(), config, tiny_limits()).unwrap();
        host.set_enabled("testbed", true).unwrap();
        Some((dir, host))
    }

    #[test]
    fn abi_echo_round_trip() {
        let Some((_dir, host)) = host_with_testbed() else { return };
        let r = host
            .dispatch("testbed", "echo", json!({"x": 1, "s": "好"}), CallClass::Normal)
            .unwrap();
        assert_eq!(r["echo"]["x"], 1);
        assert_eq!(r["echo"]["s"], "好");
    }

    #[test]
    fn spin_is_aborted_within_budget() {
        let Some((_dir, host)) = host_with_testbed() else { return };
        let started = std::time::Instant::now();
        let err = host.dispatch("testbed", "spin", json!({}), CallClass::Normal).unwrap_err();
        assert!(matches!(err, CallError::Fatal(_)), "应为致命中止: {err}");
        // 预算 800ms/2e8 fuel，宽容断言：远小于「永远」
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn alloc_bomb_traps_not_crashes() {
        let Some((_dir, host)) = host_with_testbed() else { return };
        let err = host.dispatch("testbed", "alloc_bomb", json!({}), CallClass::Normal).unwrap_err();
        assert!(matches!(err, CallError::Fatal(_)), "应为致命中止: {err}");
        // 宿主还活着：再来一次正常调用
        let r = host.dispatch("testbed", "echo", json!({"ok": true}), CallClass::Normal).unwrap();
        assert_eq!(r["echo"]["ok"], true);
    }

    #[test]
    fn bad_json_is_internal_not_panic() {
        let Some((_dir, host)) = host_with_testbed() else { return };
        let err = host.dispatch("testbed", "bad_json", json!({}), CallClass::Normal).unwrap_err();
        assert!(matches!(err, CallError::Fatal(_)));
    }

    #[test]
    fn http_without_capability_is_forbidden() {
        let Some((_dir, host)) = host_with_testbed() else { return };
        // testbed manifest 未申请 host:http → host_call 返回 forbidden，插件原样透传
        let err = host.dispatch("testbed", "call_http", json!({}), CallClass::Normal).unwrap_err();
        match err {
            CallError::Plugin { code, .. } => assert_eq!(code, "forbidden"),
            other => panic!("期望 forbidden 业务错误，得到 {other}"),
        }
    }

    #[test]
    fn settings_round_trip_via_host_call() {
        // trim-trailing 没申请 settings；testbed 也没有——直接测 settings_values/set_settings 的宿主侧
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("cfg")).unwrap();
        std::fs::write(
            dir.path().join("cfg/manifest.toml"),
            "id = \"cfg\"\nname = \"C\"\nversion = \"1\"\napiVersion = \"0.2\"\n[settings.schema]\napi_key = { type = \"secret\" }\nmodel = { type = \"string\" }\n",
        )
        .unwrap();
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(dir.path().to_path_buf(), config).unwrap();

        let mut vals = serde_json::Map::new();
        vals.insert("api_key".into(), json!("sk-secret"));
        vals.insert("model".into(), json!("m1"));
        host.set_settings("cfg", &vals).unwrap();

        let (values, secret_set) = host.settings_values("cfg").unwrap();
        assert_eq!(values["model"], "m1");
        assert!(values.get("api_key").is_none(), "secret 不回显");
        assert_eq!(secret_set["api_key"], true);

        // 空串清除 secret
        let mut clear = serde_json::Map::new();
        clear.insert("api_key".into(), json!(""));
        host.set_settings("cfg", &clear).unwrap();
        let (_, secret_set) = host.settings_values("cfg").unwrap();
        assert!(secret_set.get("api_key").is_none());

        // 未知键拒绝
        let mut bad = serde_json::Map::new();
        bad.insert("nope".into(), json!(1));
        assert!(matches!(host.set_settings("cfg", &bad), Err(HostOpError::Invalid(_))));
    }

    #[test]
    fn zero_code_auto_enables_and_backend_defaults_disabled() {
        let dir = tempfile::tempdir().unwrap();
        // 零代码（纯清单）
        std::fs::create_dir_all(dir.path().join("theme-x")).unwrap();
        std::fs::write(
            dir.path().join("theme-x/manifest.toml"),
            "id = \"theme-x\"\nname = \"T\"\nversion = \"1\"\napiVersion = \"0.1\"\n",
        )
        .unwrap();
        // 含 backend（wasm 文件都不必存在——默认禁用即不编译）
        std::fs::create_dir_all(dir.path().join("backendy")).unwrap();
        std::fs::write(
            dir.path().join("backendy/manifest.toml"),
            "id = \"backendy\"\nname = \"B\"\nversion = \"1\"\napiVersion = \"0.2\"\n[backend]\nwasm = \"plugin.wasm\"\n",
        )
        .unwrap();
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(dir.path().to_path_buf(), config).unwrap();
        let infos = host.list_info();
        let theme = infos.iter().find(|p| p.id == "theme-x").unwrap();
        let backendy = infos.iter().find(|p| p.id == "backendy").unwrap();
        assert!(theme.enabled, "零代码自动启用");
        assert!(!backendy.enabled, "含 backend 默认禁用等待授权");
        // 启用缺 wasm 的插件 → Wasm 错误
        assert!(matches!(host.set_enabled("backendy", true), Err(HostOpError::Wasm(_))));
    }
}
