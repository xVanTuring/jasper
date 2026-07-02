//! ABI 运行时（spec §6）：内存分配、u64 打包、JSON 信封。
//!
//! 插件作者通常不直接用本模块——`register!` 宏会生成
//! `plugin_alloc` / `plugin_free` / `plugin_dispatch` 三个导出并委托到这里。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 插件侧错误（映射为信封里的 `error` 对象）。`code` 取值见 spec §6.4。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginError {
    pub message: String,
    pub code: String,
}

impl PluginError {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self { message: message.into(), code: code.into() }
    }
    pub fn forbidden(msg: impl Into<String>) -> Self {
        Self::new("forbidden", msg)
    }
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new("not_found", msg)
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::new("invalid", msg)
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new("internal", msg)
    }
    pub fn unsupported(msg: impl Into<String>) -> Self {
        Self::new("unsupported", msg)
    }
}

/// 请求信封：`{ "method": string, "params": <json> }`（spec §6.4）。
#[derive(Debug, Deserialize)]
pub struct Req {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

/// 响应信封：`{ ok, result } | { ok, error }`（spec §6.4）。
#[derive(Debug, Serialize, Deserialize)]
pub struct Resp {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PluginError>,
}

impl Resp {
    pub fn ok(result: Value) -> Self {
        Self { ok: true, result: Some(result), error: None }
    }
    pub fn err(e: PluginError) -> Self {
        Self { ok: false, result: None, error: Some(e) }
    }
    pub fn into_result(self) -> Result<Value, PluginError> {
        if self.ok {
            Ok(self.result.unwrap_or(Value::Null))
        } else {
            Err(self.error.unwrap_or_else(|| PluginError::internal("宿主返回错误但缺 error 对象")))
        }
    }
}

/// 打包 (ptr, len) 为 u64：`(ptr << 32) | len`（spec §6.2）。
pub fn pack(ptr: u32, len: u32) -> u64 {
    ((ptr as u64) << 32) | len as u64
}

/// 解包 u64 为 (ptr, len)。
pub fn unpack(packed: u64) -> (u32, u32) {
    ((packed >> 32) as u32, (packed & 0xffff_ffff) as u32)
}

/// 分配 `size` 字节（对齐 1）。size=0 返回 0，不分配。
pub fn alloc(size: usize) -> usize {
    if size == 0 {
        return 0;
    }
    let layout = std::alloc::Layout::from_size_align(size, 1).expect("bad layout");
    unsafe { std::alloc::alloc(layout) as usize }
}

/// 释放 `alloc` 分配的内存。ptr=0 或 size=0 为 no-op。
pub fn free(ptr: usize, size: usize) {
    if ptr == 0 || size == 0 {
        return;
    }
    let layout = std::alloc::Layout::from_size_align(size, 1).expect("bad layout");
    unsafe { std::alloc::dealloc(ptr as *mut u8, layout) }
}

/// `plugin_dispatch` 的通用实现：解析请求信封 → 调业务闭包 → 写回响应。
/// 入参缓冲区归宿主所有（宿主 alloc、宿主 free）；返回值缓冲区由本函数 alloc、宿主读完 free。
///
/// # Safety 说明
/// `ptr/len` 必须指向本线性内存里一段有效的请求字节（由宿主经 `plugin_alloc` 写入）。
pub fn dispatch(ptr: usize, len: usize, f: impl Fn(&str, Value) -> Result<Value, PluginError>) -> u64 {
    let req_bytes = unsafe { std::slice::from_raw_parts(ptr as *const u8, len) };
    let resp = match serde_json::from_slice::<Req>(req_bytes) {
        Ok(req) => match f(&req.method, req.params) {
            Ok(result) => Resp::ok(result),
            Err(e) => Resp::err(e),
        },
        Err(e) => Resp::err(PluginError::invalid(format!("请求 JSON 解析失败: {e}"))),
    };
    let out = serde_json::to_vec(&resp).unwrap_or_else(|_| {
        br#"{"ok":false,"error":{"message":"response serialize failed","code":"internal"}}"#.to_vec()
    });
    write_out(&out)
}

/// 把一段字节写进新分配的插件内存并打包返回（宿主读完负责 plugin_free）。
pub fn write_out(bytes: &[u8]) -> u64 {
    let ptr = alloc(bytes.len());
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len()) };
    pack(ptr as u32, bytes.len() as u32)
}

/// 插件 → 宿主调用（spec §6.3）。仅 wasm 环境可用；native 下直接报错（供单测桩用），
/// 除非开 `native-host` feature——则路由到本地宿主替身（见 native_host.rs，仅测试用）。
pub fn call_host(method: &str, params: Value) -> Result<Value, PluginError> {
    #[cfg(all(not(target_arch = "wasm32"), feature = "native-host"))]
    {
        crate::native_host::call(method, params)
    }
    #[cfg(not(all(not(target_arch = "wasm32"), feature = "native-host")))]
    {
        let req = serde_json::to_vec(&serde_json::json!({ "method": method, "params": params }))
            .map_err(|e| PluginError::internal(format!("host 请求序列化失败: {e}")))?;
        let packed = raw_host_call(&req)?;
        let (ptr, len) = unpack(packed);
        let bytes =
            unsafe { std::slice::from_raw_parts(ptr as usize as *const u8, len as usize) }.to_vec();
        // 响应缓冲区由宿主经 plugin_alloc 写入，插件读完释放（spec §6.3）
        free(ptr as usize, len as usize);
        let resp: Resp = serde_json::from_slice(&bytes)
            .map_err(|e| PluginError::internal(format!("host 响应 JSON 解析失败: {e}")))?;
        resp.into_result()
    }
}

#[cfg(target_arch = "wasm32")]
fn raw_host_call(req: &[u8]) -> Result<u64, PluginError> {
    #[link(wasm_import_module = "joplin")]
    extern "C" {
        fn host_call(ptr: u32, len: u32) -> u64;
    }
    Ok(unsafe { host_call(req.as_ptr() as u32, req.len() as u32) })
}

#[cfg(not(target_arch = "wasm32"))]
fn raw_host_call(_req: &[u8]) -> Result<u64, PluginError> {
    Err(PluginError::internal("host_call 仅在 wasm 环境可用"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pack_unpack_round_trip() {
        let packed = pack(0xdead_beef, 42);
        assert_eq!(unpack(packed), (0xdead_beef, 42));
        assert_eq!(unpack(pack(0, 0)), (0, 0));
        assert_eq!(unpack(pack(u32::MAX, u32::MAX)), (u32::MAX, u32::MAX));
    }

    #[test]
    fn envelope_shapes() {
        let ok = serde_json::to_value(Resp::ok(json!({"x": 1}))).unwrap();
        assert_eq!(ok, json!({"ok": true, "result": {"x": 1}}));
        let err = serde_json::to_value(Resp::err(PluginError::forbidden("nope"))).unwrap();
        assert_eq!(err, json!({"ok": false, "error": {"message": "nope", "code": "forbidden"}}));
    }

    #[test]
    fn req_params_default_null() {
        let req: Req = serde_json::from_str(r#"{"method":"metadata"}"#).unwrap();
        assert_eq!(req.method, "metadata");
        assert!(req.params.is_null());
    }

    #[test]
    fn into_result_maps_error_code() {
        let resp: Resp = serde_json::from_str(
            r#"{"ok":false,"error":{"message":"m","code":"not_found"}}"#,
        )
        .unwrap();
        let err = resp.into_result().unwrap_err();
        assert_eq!(err.code, "not_found");
    }

    #[test]
    #[cfg(not(feature = "native-host"))]
    fn native_host_call_errors_cleanly() {
        let err = call_host("log", json!({})).unwrap_err();
        assert_eq!(err.code, "internal");
    }
}
