//! wasmi 沙箱执行（spec §6/§11）。
//!
//! 每次调用新建 Store/实例（状态隔离）；限额两档（Normal/Storage）：
//! - 内存：StoreLimits 拦 memory.grow，超限 trap（不崩宿主）；
//! - fuel：切片执行 + resumable call —— 每耗尽一片回到宿主检查点，
//!   核对 fuel 总预算与 **CPU 墙钟**（elapsed − host_call 内的 IO 时间）后续燃或中止。

use super::host_api;
use crate::config::{AiConfig, ConfigStore};
use crate::library::Library;
use crate::storage::StorageBackend;
use anyhow::{anyhow, Context, Result};
use serde_json::Value;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use wasmi::{Caller, Config, Engine, Extern, Linker, Module, Store, StoreLimits, StoreLimitsBuilder, TypedResumableCall};

/// 调用类别 → 限额档位（spec §11，0.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallClass {
    Normal,
    Storage,
}

#[derive(Debug, Clone)]
pub struct PluginLimits {
    pub normal_memory: usize,
    pub normal_fuel: u64,
    pub normal_cpu: Duration,
    pub storage_memory: usize,
    pub storage_fuel: u64,
    pub storage_cpu: Duration,
    /// fuel 切片：每片耗尽回宿主做一次预算检查。
    pub fuel_slice: u64,
    /// http.request 响应体上限。
    pub http_response_cap: usize,
}

impl Default for PluginLimits {
    fn default() -> Self {
        Self {
            normal_memory: 64 * 1024 * 1024,
            normal_fuel: 1_000_000_000,
            normal_cpu: Duration::from_secs(2),
            storage_memory: 256 * 1024 * 1024,
            storage_fuel: 5_000_000_000,
            storage_cpu: Duration::from_secs(10),
            fuel_slice: 50_000_000,
            http_response_cap: 128 * 1024 * 1024,
        }
    }
}

/// 调用失败的两种形态：插件返回的业务错误（带 spec §6.4 错误码） vs 宿主判定的致命错误。
#[derive(Debug)]
pub enum CallError {
    Plugin { code: String, message: String },
    Fatal(String),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Plugin { code, message } => write!(f, "插件错误[{code}]: {message}"),
            CallError::Fatal(m) => write!(f, "插件调用失败: {m}"),
        }
    }
}

impl std::error::Error for CallError {}

/// notes:* / host:ai 的调用上下文（spec 0.3 §6.5）：仅 command / ui 分发提供，
/// hooks/storage 分发为 None → 对应 host_call 返回 unsupported。
pub struct NotesCtx {
    pub library: Arc<RwLock<Library>>,
    /// dispatch 时的存储快照（数据源未配置 = None → 写入报错）。
    pub storage: Option<Arc<dyn StorageBackend>>,
    pub read_only: bool,
    /// 宿主托管的「写入免确认」开关（按插件）：关 = 提案回传，开 = 直写（跳过 before-save 钩子）。
    pub auto_approve: bool,
    /// tokio 运行时句柄：ai.complete（genai 为 async）在阻塞线程里 block_on 用。
    pub handle: tokio::runtime::Handle,
    /// 宿主级 AI 配置快照。
    pub ai: AiConfig,
    /// 本次调用累积的写提案（WriteProposal JSON）；路由层持 clone、调用后 drain 进 HTTP 响应。
    pub pending: Arc<Mutex<Vec<Value>>>,
    /// 变更事件总线：免确认直写经 persist_note_blocking 广播（SSE /api/events）。
    pub events: crate::events::EventBus,
}

/// 宿主上下文：挂在 Store 数据上，host_call 从这里拿能力/配置/HTTP 出口。
pub struct HostCtx {
    pub plugin_id: String,
    /// 已授权能力（enable 时落库的 granted_caps）。
    pub caps: Vec<String>,
    pub limits: StoreLimits,
    /// host_call 内部的 IO 耗时（不计入 CPU 墙钟）。
    pub io_time: Duration,
    pub config: Arc<Mutex<ConfigStore>>,
    pub http: ureq::Agent,
    pub http_response_cap: usize,
    /// notes/ai 上下文（仅 command/ui 分发，spec 0.3）。
    pub notes: Option<NotesCtx>,
}

impl HostCtx {
    pub fn has_cap(&self, cap: &str) -> bool {
        self.caps.iter().any(|c| c == cap)
    }
}

pub fn new_engine() -> Engine {
    let mut cfg = Config::default();
    cfg.consume_fuel(true);
    Engine::new(&cfg)
}

pub fn compile(engine: &Engine, wasm_path: &std::path::Path) -> Result<Arc<Module>> {
    let bytes = std::fs::read(wasm_path).with_context(|| format!("读 {wasm_path:?} 失败"))?;
    let module = Module::new(engine, &bytes).map_err(|e| anyhow!("wasm 编译失败: {e}"))?;
    Ok(Arc::new(module))
}

fn pack_unpack(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, (packed & 0xffff_ffff) as u32)
}

/// host_call import（spec §6.3）：读请求 → host_api 处理 → 经插件 plugin_alloc 写响应。
/// 业务性失败（能力不足等）以 JSON 错误信封返回给插件；只有 ABI 破坏才返回 Err（→ 中止调用）。
fn host_call_impl(mut caller: Caller<'_, HostCtx>, ptr: u32, len: u32) -> Result<u64, wasmi::Error> {
    let mem = caller
        .get_export("memory")
        .and_then(Extern::into_memory)
        .ok_or_else(|| wasmi::Error::new("插件缺 memory 导出"))?;

    let mut req = vec![0u8; len as usize];
    mem.read(&caller, ptr as usize, &mut req)
        .map_err(|e| wasmi::Error::new(format!("读插件内存失败: {e}")))?;

    #[derive(serde::Deserialize)]
    struct HostReq {
        method: String,
        #[serde(default)]
        params: Value,
    }
    let resp: Value = match serde_json::from_slice::<HostReq>(&req) {
        Ok(r) => host_api::handle(caller.data_mut(), &r.method, r.params),
        Err(e) => host_api::err_envelope("invalid", format!("host_call 请求 JSON 解析失败: {e}")),
    };
    let out = serde_json::to_vec(&resp).unwrap_or_else(|_| {
        br#"{"ok":false,"error":{"message":"host response serialize failed","code":"internal"}}"#.to_vec()
    });

    // 再入调用插件的 plugin_alloc 分配响应缓冲（spec §6.3）
    let alloc = caller
        .get_export("plugin_alloc")
        .and_then(Extern::into_func)
        .ok_or_else(|| wasmi::Error::new("插件缺 plugin_alloc 导出"))?
        .typed::<u32, u32>(&caller)
        .map_err(|e| wasmi::Error::new(format!("plugin_alloc 签名不符: {e}")))?;
    let out_ptr = alloc
        .call(&mut caller, out.len() as u32)
        .map_err(|e| wasmi::Error::new(format!("plugin_alloc 调用失败: {e}")))?;
    mem.write(&mut caller, out_ptr as usize, &out)
        .map_err(|e| wasmi::Error::new(format!("写插件内存失败: {e}")))?;
    Ok(((out_ptr as u64) << 32) | out.len() as u64)
}

/// 执行一次 `plugin_dispatch`（spec §6.2）。`ctx` 每次调用新建（状态隔离）。
pub fn call_dispatch(
    engine: &Engine,
    module: &Module,
    mut ctx: HostCtx,
    method: &str,
    params: Value,
    class: CallClass,
    limits: &PluginLimits,
) -> Result<Value, CallError> {
    let (mem_max, fuel_budget, cpu_budget) = match class {
        CallClass::Normal => (limits.normal_memory, limits.normal_fuel, limits.normal_cpu),
        CallClass::Storage => (limits.storage_memory, limits.storage_fuel, limits.storage_cpu),
    };
    ctx.limits = StoreLimitsBuilder::new().memory_size(mem_max).build();

    let fatal = |m: String| CallError::Fatal(m);

    let mut store = Store::new(engine, ctx);
    store.limiter(|c| &mut c.limits);
    let slice = limits.fuel_slice.min(fuel_budget).max(1);
    store.set_fuel(slice).map_err(|e| fatal(format!("set_fuel: {e}")))?;

    let mut linker: Linker<HostCtx> = Linker::new(engine);
    linker
        .func_wrap("joplin", "host_call", host_call_impl)
        .map_err(|e| fatal(format!("注册 host_call 失败: {e}")))?;
    let instance = linker
        .instantiate_and_start(&mut store, module)
        .map_err(|e| fatal(format!("实例化失败: {e}")))?;

    let mem = instance
        .get_memory(&store, "memory")
        .ok_or_else(|| fatal("插件缺 memory 导出".into()))?;
    let alloc = instance
        .get_typed_func::<u32, u32>(&store, "plugin_alloc")
        .map_err(|e| fatal(format!("缺 plugin_alloc: {e}")))?;
    let free = instance
        .get_typed_func::<(u32, u32), ()>(&store, "plugin_free")
        .map_err(|e| fatal(format!("缺 plugin_free: {e}")))?;
    let dispatch = instance
        .get_typed_func::<(u32, u32), u64>(&store, "plugin_dispatch")
        .map_err(|e| fatal(format!("缺 plugin_dispatch: {e}")))?;

    // 写请求（请求缓冲归宿主所有：宿主 alloc、读完响应后宿主 free）
    let req_bytes = serde_json::to_vec(&serde_json::json!({ "method": method, "params": params }))
        .map_err(|e| fatal(format!("请求序列化失败: {e}")))?;
    let req_len = req_bytes.len() as u32;
    let req_ptr = alloc.call(&mut store, req_len).map_err(|e| fatal(format!("plugin_alloc 失败: {e}")))?;
    mem.write(&mut store, req_ptr as usize, &req_bytes)
        .map_err(|e| fatal(format!("写请求失败: {e}")))?;

    // 切片执行 + 检查点（fuel 总额 / CPU 墙钟 = elapsed − io_time）
    let started = Instant::now();
    let mut spent: u64 = slice.saturating_sub(store.get_fuel().unwrap_or(0));
    store.set_fuel(slice).map_err(|e| fatal(format!("set_fuel: {e}")))?;
    let mut call = dispatch
        .call_resumable(&mut store, (req_ptr, req_len))
        .map_err(|e| fatal(format!("plugin_dispatch 失败: {e}")))?;
    let packed = loop {
        match call {
            TypedResumableCall::Finished(v) => break v,
            TypedResumableCall::OutOfFuel(oof) => {
                spent = spent.saturating_add(slice.saturating_sub(store.get_fuel().unwrap_or(0)));
                if spent >= fuel_budget {
                    return Err(fatal(format!("fuel 超限（预算 {fuel_budget}）")));
                }
                let cpu = started.elapsed().saturating_sub(store.data().io_time);
                if cpu >= cpu_budget {
                    return Err(fatal(format!("CPU 墙钟超限（预算 {cpu_budget:?}，已用 {cpu:?}）")));
                }
                let next = slice.max(oof.required_fuel()).min(fuel_budget.saturating_sub(spent).max(1));
                store.set_fuel(next).map_err(|e| fatal(format!("set_fuel: {e}")))?;
                call = oof.resume(&mut store).map_err(|e| fatal(format!("续跑失败: {e}")))?;
            }
            // host_call 返回 Err = ABI 破坏（业务错误早已按信封返回），不续跑
            TypedResumableCall::HostTrap(t) => {
                return Err(fatal(format!("host_call 致命错误: {:?}", t.host_error())));
            }
        }
    };

    // 读响应 → 释放响应与请求缓冲
    let (out_ptr, out_len) = pack_unpack(packed);
    let mut out = vec![0u8; out_len as usize];
    mem.read(&store, out_ptr as usize, &mut out)
        .map_err(|e| fatal(format!("读响应失败（越界指针？）: {e}")))?;
    store.set_fuel(slice).ok();
    let _ = free.call(&mut store, (out_ptr, out_len));
    let _ = free.call(&mut store, (req_ptr, req_len));

    // 解析信封（spec §6.4）
    #[derive(serde::Deserialize)]
    struct ErrObj {
        message: String,
        #[serde(default)]
        code: String,
    }
    #[derive(serde::Deserialize)]
    struct Resp {
        ok: bool,
        #[serde(default)]
        result: Option<Value>,
        #[serde(default)]
        error: Option<ErrObj>,
    }
    let resp: Resp = serde_json::from_slice(&out)
        .map_err(|e| fatal(format!("响应不是合法 JSON 信封: {e}")))?;
    if resp.ok {
        Ok(resp.result.unwrap_or(Value::Null))
    } else {
        let e = resp.error.map(|e| (e.code, e.message)).unwrap_or_default();
        Err(CallError::Plugin {
            code: if e.0.is_empty() { "internal".into() } else { e.0 },
            message: e.1,
        })
    }
}
