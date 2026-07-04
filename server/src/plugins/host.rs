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
    /// notes:write 的「写入免确认」开关（宿主托管，spec 0.3 §7）。
    pub write_auto_approve: bool,
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

    fn info_of(&self, p: &LoadedPlugin) -> PluginInfo {
        let write_auto_approve = self.config.lock().unwrap().plugin_write_auto_approve(&p.id);
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
                write_auto_approve,
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
                write_auto_approve,
            },
        }
    }

    pub fn list_info(&self) -> Vec<PluginInfo> {
        self.plugins.read().unwrap().values().map(|p| self.info_of(p)).collect()
    }

    /// 单个插件的信息（auto-approve 端点返回体用）。
    pub fn info(&self, id: &str) -> Option<PluginInfo> {
        self.plugins.read().unwrap().get(id).map(|p| self.info_of(p))
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
        let info = self.info_of(&loaded);
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
        Ok(self.info_of(p))
    }

    /// 执行插件的 plugin_dispatch（阻塞；调用方负责 spawn_blocking / rayon 上下文）。
    /// 无 notes/ai 上下文——hooks/storage 调用方走这里（spec 0.3 §6.5）。
    pub fn dispatch(
        &self,
        plugin_id: &str,
        method: &str,
        params: Value,
        class: CallClass,
    ) -> Result<Value, CallError> {
        self.dispatch_with_notes(plugin_id, method, params, class, None)
    }

    /// 同 [`dispatch`]，但携带 notes/ai 调用上下文（command/ui 路由用，spec 0.3）。
    pub fn dispatch_with_notes(
        &self,
        plugin_id: &str,
        method: &str,
        params: Value,
        class: CallClass,
        notes: Option<runtime::NotesCtx>,
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
            notes,
        };
        tracing::debug!(plugin_id, method, ?class, "dispatching plugin call");
        let result = runtime::call_dispatch(&self.engine, &module, ctx, method, params, class, &self.limits);
        if let Err(e) = &result {
            tracing::warn!(plugin_id, method, error = %e, "plugin call failed");
        }
        result
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

    /// ui 端点守卫：view 须在该插件的 contributes.sidebar 里声明（spec §3.5/§9.5）。
    pub fn has_ui_view(&self, plugin_id: &str, view: &str) -> bool {
        let map = self.plugins.read().unwrap();
        let Some(p) = map.get(plugin_id) else { return false };
        p.enabled
            && p.module.is_some()
            && p.manifest
                .as_ref()
                .map(|m| m.contributes.sidebar.iter().any(|s| s.view.as_deref() == Some(view)))
                .unwrap_or(false)
    }

    /// editor.transform 端点守卫：该插件是否为相位 `phase` 声明了 contributes.editor（spec §3.7/§6.5）。
    pub fn has_editor_transform(&self, plugin_id: &str, phase: &str) -> bool {
        let map = self.plugins.read().unwrap();
        let Some(p) = map.get(plugin_id) else { return false };
        p.enabled
            && p.module.is_some()
            && p.manifest
                .as_ref()
                .map(|m| m.contributes.editor.iter().any(|e| e.on == phase))
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
            eprintln!("skipping: {name}/plugin.wasm not built (run plugins-examples/build-wasm.sh first)");
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

    // ---------- notes.*（spec 0.3 §6.5）----------

    use crate::plugins::runtime::NotesCtx;

    /// testbed wasm + 自写 manifest（能力集由测试指定）。
    fn host_with_manifest(manifest: &str) -> Option<(tempfile::TempDir, Arc<PluginHost>)> {
        let src = examples_dir().join("testbed/plugin.wasm");
        if !src.exists() {
            eprintln!("skipping: testbed/plugin.wasm not built (run plugins-examples/build-wasm.sh first)");
            return None;
        }
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("testbed");
        std::fs::create_dir_all(&dst).unwrap();
        std::fs::copy(src, dst.join("plugin.wasm")).unwrap();
        std::fs::write(dst.join("manifest.toml"), manifest).unwrap();
        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host =
            PluginHost::init_at_with_limits(dir.path().to_path_buf(), config, tiny_limits()).unwrap();
        host.set_enabled("testbed", true).unwrap();
        Some((dir, host))
    }

    const NOTES_MANIFEST: &str = "id = \"testbed\"\nname = \"T\"\nversion = \"1\"\napiVersion = \"0.3\"\n[backend]\nwasm = \"plugin.wasm\"\ncapabilities = [\"notes:read\", \"notes:write\"]\n";

    /// Library + 本地临时存储夹具（1 笔记本 + 2 笔记，其一含行尾空格供钩子测试）。
    struct NotesFixture {
        library: Arc<RwLock<crate::library::Library>>,
        _storage_dir: tempfile::TempDir,
        storage_root: PathBuf,
        storage: Arc<dyn crate::storage::StorageBackend>,
        folder_id: String,
        note_id: String,
        rt: tokio::runtime::Runtime,
    }

    fn notes_fixture() -> NotesFixture {
        let folder_id = "f".repeat(32);
        let note_id = "a".repeat(32);
        let note2_id = "b".repeat(32);
        let contents = vec![
            crate::serialize::new_folder_md(&folder_id, "", "收件箱", 1_700_000_000_000),
            crate::serialize::new_note_md(&note_id, &folder_id, "购物清单", "牛奶 面包", false, 1_700_000_000_000),
            crate::serialize::new_note_md(&note2_id, &folder_id, "旅行", "订机票", false, 1_700_000_000_000),
        ];
        let storage_dir = tempfile::tempdir().unwrap();
        // 存储镜像同一批条目（写路径要读原 raw + 落盘断言）
        std::fs::write(storage_dir.path().join(format!("{folder_id}.md")), &contents[0]).unwrap();
        std::fs::write(storage_dir.path().join(format!("{note_id}.md")), &contents[1]).unwrap();
        std::fs::write(storage_dir.path().join(format!("{note2_id}.md")), &contents[2]).unwrap();
        let (lib, _stats) = crate::library::Library::from_contents(contents);
        NotesFixture {
            library: Arc::new(RwLock::new(lib)),
            storage_root: storage_dir.path().to_path_buf(),
            storage: Arc::new(crate::storage::local::LocalStorage::new(storage_dir.path())),
            _storage_dir: storage_dir,
            folder_id,
            note_id,
            rt: tokio::runtime::Runtime::new().unwrap(),
        }
    }

    fn ctx_of(fx: &NotesFixture, read_only: bool, auto_approve: bool) -> (NotesCtx, Arc<Mutex<Vec<serde_json::Value>>>) {
        let pending = Arc::new(Mutex::new(Vec::new()));
        (
            NotesCtx {
                library: fx.library.clone(),
                storage: Some(fx.storage.clone()),
                read_only,
                auto_approve,
                handle: fx.rt.handle().clone(),
                ai: Default::default(),
                pending: pending.clone(),
                events: crate::events::EventBus::new(),
            },
            pending,
        )
    }

    fn cmd(id: &str, args: serde_json::Value) -> Value {
        json!({ "id": id, "args": args })
    }

    #[test]
    fn notes_without_capability_is_forbidden() {
        // manifest 未申请 notes:* → forbidden（即便给了 ctx）
        let Some((_dir, host)) = host_with_manifest(
            "id = \"testbed\"\nname = \"T\"\nversion = \"1\"\napiVersion = \"0.3\"\n[backend]\nwasm = \"plugin.wasm\"\n",
        ) else {
            return;
        };
        let fx = notes_fixture();
        let (ctx, _) = ctx_of(&fx, false, false);
        let err = host
            .dispatch_with_notes("testbed", "command", cmd("read-note", json!({"id": fx.note_id})), CallClass::Normal, Some(ctx))
            .unwrap_err();
        match err {
            CallError::Plugin { code, .. } => assert_eq!(code, "forbidden"),
            other => panic!("期望 forbidden，得到 {other}"),
        }
    }

    #[test]
    fn notes_outside_command_ui_context_is_unsupported() {
        // 有能力但无 ctx（hooks/storage 路径）→ unsupported（spec §6.5）
        let Some((_dir, host)) = host_with_manifest(NOTES_MANIFEST) else { return };
        let fx = notes_fixture();
        let err = host
            .dispatch("testbed", "command", cmd("read-note", json!({"id": fx.note_id})), CallClass::Normal)
            .unwrap_err();
        match err {
            CallError::Plugin { code, .. } => assert_eq!(code, "unsupported"),
            other => panic!("期望 unsupported，得到 {other}"),
        }
    }

    #[test]
    fn notes_read_search_and_folders() {
        let Some((_dir, host)) = host_with_manifest(NOTES_MANIFEST) else { return };
        let fx = notes_fixture();

        let (ctx, _) = ctx_of(&fx, false, false);
        let r = host
            .dispatch_with_notes("testbed", "command", cmd("read-note", json!({"id": fx.note_id})), CallClass::Normal, Some(ctx))
            .unwrap();
        assert_eq!(r["note"]["title"], "购物清单");
        assert_eq!(r["note"]["parent_id"], fx.folder_id.as_str());

        let (ctx, _) = ctx_of(&fx, false, false);
        let r = host
            .dispatch_with_notes("testbed", "command", cmd("search-notes", json!({"q": "机票"})), CallClass::Normal, Some(ctx))
            .unwrap();
        let hits = r["notes"].as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["title"], "旅行");

        let (ctx, _) = ctx_of(&fx, false, false);
        let r = host
            .dispatch_with_notes("testbed", "command", cmd("list-folders", json!({})), CallClass::Normal, Some(ctx))
            .unwrap();
        let folders = r["folders"].as_array().unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0]["title"], "收件箱");

        // 不存在的笔记 → not_found
        let (ctx, _) = ctx_of(&fx, false, false);
        let err = host
            .dispatch_with_notes("testbed", "command", cmd("read-note", json!({"id": "9".repeat(32)})), CallClass::Normal, Some(ctx))
            .unwrap_err();
        match err {
            CallError::Plugin { code, .. } => assert_eq!(code, "not_found"),
            other => panic!("期望 not_found，得到 {other}"),
        }
    }

    #[test]
    fn notes_write_pending_by_default() {
        let Some((_dir, host)) = host_with_manifest(NOTES_MANIFEST) else { return };
        let fx = notes_fixture();
        let disk_before =
            std::fs::read_to_string(fx.storage_root.join(format!("{}.md", fx.note_id))).unwrap();

        let (ctx, pending) = ctx_of(&fx, false, false);
        let r = host
            .dispatch_with_notes(
                "testbed",
                "command",
                cmd("write-note", json!({"id": fx.note_id, "body": "改写后的正文"})),
                CallClass::Normal,
                Some(ctx),
            )
            .unwrap();
        assert_eq!(r["pending"], true);
        assert_eq!(r["note"]["body"], "改写后的正文");

        // 提案在累积器里，形状齐全
        let proposals = pending.lock().unwrap();
        assert_eq!(proposals.len(), 1);
        assert_eq!(proposals[0]["action"], "update");
        assert_eq!(proposals[0]["plugin_id"], "testbed");
        assert_eq!(proposals[0]["note"]["body"], "改写后的正文");
        assert_eq!(proposals[0]["original"]["body"], "牛奶 面包");

        // 盘与索引都未动
        let disk_after =
            std::fs::read_to_string(fx.storage_root.join(format!("{}.md", fx.note_id))).unwrap();
        assert_eq!(disk_before, disk_after);
        assert_eq!(fx.library.read().unwrap().note(&fx.note_id).unwrap().body, "牛奶 面包");
    }

    #[test]
    fn notes_write_auto_approve_writes_and_skips_hooks() {
        // testbed（写）+ trim-trailing（before-save 钩子）同装：
        // 插件直写路径必须跳过钩子 → 行尾空格存活（spec §6.5 防重入）
        let Some((dir, host)) = host_with_manifest(NOTES_MANIFEST) else { return };
        if install_example(dir.path(), "trim-trailing").is_none() {
            return;
        }
        host.rescan();
        host.set_enabled("testbed", true).unwrap();
        host.set_enabled("trim-trailing", true).unwrap();
        assert!(host.before_save_plugins().contains(&"trim-trailing".to_string()), "钩子插件应已挂上");

        let fx = notes_fixture();
        let (ctx, pending) = ctx_of(&fx, false, true); // 免确认
        let r = host
            .dispatch_with_notes(
                "testbed",
                "command",
                cmd("write-note", json!({"id": fx.note_id, "body": "行尾有空格  \n第二行\t"})),
                CallClass::Normal,
                Some(ctx),
            )
            .unwrap();
        assert_eq!(r["pending"], false);
        assert!(pending.lock().unwrap().is_empty());

        // 落盘 + 索引已更新，且行尾空白保留（未过 trim-trailing）
        let disk =
            std::fs::read_to_string(fx.storage_root.join(format!("{}.md", fx.note_id))).unwrap();
        assert!(disk.contains("行尾有空格  \n第二行\t"), "直写应跳过 before-save 钩子: {disk:?}");
        assert_eq!(fx.library.read().unwrap().note(&fx.note_id).unwrap().body, "行尾有空格  \n第二行\t");
        // 其余元数据逐字保留（id/parent 不变）
        assert_eq!(fx.library.read().unwrap().note(&fx.note_id).unwrap().parent_id, fx.folder_id);
    }

    #[test]
    fn notes_create_validates_parent_and_writes() {
        let Some((_dir, host)) = host_with_manifest(NOTES_MANIFEST) else { return };
        let fx = notes_fixture();

        // parent 不存在 → invalid
        let (ctx, _) = ctx_of(&fx, false, true);
        let err = host
            .dispatch_with_notes(
                "testbed",
                "command",
                cmd("make-note", json!({"parent_id": "9".repeat(32), "title": "x"})),
                CallClass::Normal,
                Some(ctx),
            )
            .unwrap_err();
        match err {
            CallError::Plugin { code, .. } => assert_eq!(code, "invalid"),
            other => panic!("期望 invalid，得到 {other}"),
        }

        // pending：id 为空串、不落盘
        let (ctx, pending) = ctx_of(&fx, false, false);
        let r = host
            .dispatch_with_notes(
                "testbed",
                "command",
                cmd("make-note", json!({"parent_id": fx.folder_id, "title": "新笔记", "body": "内容"})),
                CallClass::Normal,
                Some(ctx),
            )
            .unwrap();
        assert_eq!(r["pending"], true);
        assert_eq!(r["note"]["id"], "");
        assert_eq!(pending.lock().unwrap()[0]["action"], "create");
        assert!(pending.lock().unwrap()[0]["original"].is_null());

        // 免确认：生成 id、落盘 + 进索引
        let (ctx, _) = ctx_of(&fx, false, true);
        let r = host
            .dispatch_with_notes(
                "testbed",
                "command",
                cmd("make-note", json!({"parent_id": fx.folder_id, "title": "直写笔记", "body": "内容"})),
                CallClass::Normal,
                Some(ctx),
            )
            .unwrap();
        assert_eq!(r["pending"], false);
        let new_id = r["note"]["id"].as_str().unwrap().to_string();
        assert_eq!(new_id.len(), 32);
        assert!(fx.storage_root.join(format!("{new_id}.md")).exists());
        assert_eq!(fx.library.read().unwrap().note(&new_id).unwrap().title, "直写笔记");
    }

    #[test]
    fn notes_write_read_only_is_forbidden() {
        let Some((_dir, host)) = host_with_manifest(NOTES_MANIFEST) else { return };
        let fx = notes_fixture();
        let (ctx, pending) = ctx_of(&fx, true, true); // 只读优先于免确认
        let err = host
            .dispatch_with_notes(
                "testbed",
                "command",
                cmd("write-note", json!({"id": fx.note_id, "body": "x"})),
                CallClass::Normal,
                Some(ctx),
            )
            .unwrap_err();
        match err {
            CallError::Plugin { code, .. } => assert_eq!(code, "forbidden"),
            other => panic!("期望 forbidden，得到 {other}"),
        }
        assert!(pending.lock().unwrap().is_empty());
    }

    // ---------- ai.complete（spec 0.3 §6.5，genai）----------

    const AI_MANIFEST: &str = "id = \"testbed\"\nname = \"T\"\nversion = \"1\"\napiVersion = \"0.3\"\n[backend]\nwasm = \"plugin.wasm\"\ncapabilities = [\"host:ai\"]\n";

    fn ctx_with_ai(fx: &NotesFixture, ai: crate::config::AiConfig) -> NotesCtx {
        let (mut ctx, _) = ctx_of(fx, false, false);
        ctx.ai = ai;
        ctx
    }

    /// 极简 HTTP stub：对每个连接回固定 JSON 后关闭；服务 `hits` 个连接后退出。
    /// 顺带豁免本机代理（开发机常设 HTTP_PROXY；reqwest 默认吃环境代理，
    /// 127.0.0.1 进代理会 502——与 e2e playwright 配置里的 NO_PROXY 同一坑）。
    fn spawn_ai_stub(body: &'static str, hits: usize) -> String {
        std::env::set_var("NO_PROXY", "127.0.0.1,localhost");
        std::env::set_var("no_proxy", "127.0.0.1,localhost");
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(hits) {
                let Ok(mut s) = stream else { continue };
                s.set_read_timeout(Some(Duration::from_millis(500))).ok();
                let mut req = Vec::new();
                let mut buf = [0u8; 65536];
                loop {
                    match s.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            req.extend_from_slice(&buf[..n]);
                            if let Some(pos) = req.windows(4).position(|w| w == b"\r\n\r\n") {
                                let head = String::from_utf8_lossy(&req[..pos]).to_lowercase();
                                let cl = head
                                    .split("content-length:")
                                    .nth(1)
                                    .and_then(|s| s.split(['\r', '\n']).next())
                                    .and_then(|s| s.trim().parse::<usize>().ok())
                                    .unwrap_or(0);
                                if req.len() >= pos + 4 + cl {
                                    break;
                                }
                            }
                        }
                    }
                }
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = s.write_all(resp.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn ai_complete_via_openai_stub() {
        let Some((_dir, host)) = host_with_manifest(AI_MANIFEST) else { return };
        let fx = notes_fixture();
        let base = spawn_ai_stub(
            r#"{"id":"chatcmpl-x","object":"chat.completion","created":0,"model":"gpt-test","choices":[{"index":0,"message":{"role":"assistant","content":"stubbed-reply"},"finish_reason":"stop"}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#,
            4,
        );
        let ctx = ctx_with_ai(
            &fx,
            crate::config::AiConfig {
                provider: "openai".into(),
                base_url: format!("{base}/v1/"),
                api_key: "sk-test".into(),
                model: "gpt-test".into(),
            },
        );
        let r = host
            .dispatch_with_notes("testbed", "command", cmd("ai-echo", json!({"prompt": "hi"})), CallClass::Normal, Some(ctx))
            .unwrap();
        assert_eq!(r["content"], "stubbed-reply");
    }

    #[test]
    fn ai_complete_via_anthropic_stub() {
        let Some((_dir, host)) = host_with_manifest(AI_MANIFEST) else { return };
        let fx = notes_fixture();
        let base = spawn_ai_stub(
            r#"{"id":"msg_x","type":"message","role":"assistant","model":"claude-test","content":[{"type":"text","text":"anthropic-reply"}],"stop_reason":"end_turn","usage":{"input_tokens":1,"output_tokens":1}}"#,
            4,
        );
        let ctx = ctx_with_ai(
            &fx,
            crate::config::AiConfig {
                provider: "anthropic".into(),
                base_url: format!("{base}/v1/"),
                api_key: "sk-ant-test".into(),
                model: "claude-test".into(),
            },
        );
        let r = host
            .dispatch_with_notes("testbed", "command", cmd("ai-echo", json!({"prompt": "hi"})), CallClass::Normal, Some(ctx))
            .unwrap();
        assert_eq!(r["content"], "anthropic-reply");
    }

    #[test]
    fn ai_unconfigured_is_internal_with_hint() {
        let Some((_dir, host)) = host_with_manifest(AI_MANIFEST) else { return };
        let fx = notes_fixture();
        let (ctx, _) = ctx_of(&fx, false, false); // AiConfig 默认 = 未配置
        let err = host
            .dispatch_with_notes("testbed", "command", cmd("ai-echo", json!({"prompt": "hi"})), CallClass::Normal, Some(ctx))
            .unwrap_err();
        match err {
            CallError::Plugin { code, message } => {
                assert_eq!(code, "internal");
                assert!(message.contains("设置"), "message 应指引去设置页: {message}");
            }
            other => panic!("期望 internal，得到 {other}"),
        }
    }

    #[test]
    fn ai_without_capability_is_forbidden() {
        let Some((_dir, host)) = host_with_manifest(NOTES_MANIFEST) else { return };
        let fx = notes_fixture();
        let (ctx, _) = ctx_of(&fx, false, false);
        let err = host
            .dispatch_with_notes("testbed", "command", cmd("ai-echo", json!({"prompt": "hi"})), CallClass::Normal, Some(ctx))
            .unwrap_err();
        match err {
            CallError::Plugin { code, .. } => assert_eq!(code, "forbidden"),
            other => panic!("期望 forbidden，得到 {other}"),
        }
    }

    #[test]
    fn ui_dispatch_returns_tree_and_reaches_notes() {
        let Some((_dir, host)) = host_with_manifest(NOTES_MANIFEST) else { return };
        let fx = notes_fixture();

        // 静态树
        let (ctx, _) = ctx_of(&fx, false, false);
        let r = host
            .dispatch_with_notes("testbed", "ui", json!({"view": "main", "state": null}), CallClass::Normal, Some(ctx))
            .unwrap();
        assert_eq!(r["type"], "markdown");
        assert_eq!(r["children"][0]["type"], "button");

        // ui 上下文里 notes.search 可用（spec §6.5）
        let (ctx, _) = ctx_of(&fx, false, false);
        let r = host
            .dispatch_with_notes("testbed", "ui", json!({"view": "notes", "state": {"q": "购物"}}), CallClass::Normal, Some(ctx))
            .unwrap();
        assert_eq!(r["type"], "list");
        assert_eq!(r["props"]["items"][0]["title"], "购物清单");
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
