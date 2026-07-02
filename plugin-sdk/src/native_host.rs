//! 测试用宿主替身（仅 native + `feature = "native-host"`）：让插件的集成测试
//! 不进 wasm 沙箱也能走 host_call——`http.request` 用 ureq 发真网络请求、
//! `time.now` 用系统时钟、settings 是线程本地 map（用 [`set_setting`] 注入）。
//!
//! ⚠ 只供插件自己的测试：没有能力门控、没有限额、没有沙箱，安全语义与真宿主不同；
//! 各方法的参数/返回形状与宿主 host_api 逐字一致（见 host.rs 的封装）。
//! settings 存储是 thread_local——cargo test 每个测试独立线程，天然隔离；
//! 测试内自行跨线程时注意各线程各有一份。

use crate::rt::PluginError;
use base64::Engine as _;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Read as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

thread_local! {
	static SETTINGS: RefCell<BTreeMap<String, Value>> = RefCell::new(BTreeMap::new());
}

/// 测试注入：预置一个 settings 键值（等价于用户在设置页填好）。
pub fn set_setting(key: &str, value: Value) {
	SETTINGS.with(|s| s.borrow_mut().insert(key.to_string(), value));
}

/// 测试清场：清空本线程的 settings。
pub fn clear_settings() {
	SETTINGS.with(|s| s.borrow_mut().clear());
}

/// `call_host` 的 native 实现入口（rt.rs 在 feature 开启时路由到这里）。
pub fn call(method: &str, params: Value) -> Result<Value, PluginError> {
	match method {
		"log" => {
			let level = params.get("level").and_then(Value::as_str).unwrap_or("info");
			let message = params.get("message").and_then(Value::as_str).unwrap_or("");
			eprintln!("[plugin log/{level}] {message}");
			Ok(Value::Null)
		}
		"time.now" => {
			let ms = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.map_err(|e| PluginError::internal(format!("系统时钟异常: {e}")))?
				.as_millis() as i64;
			Ok(json!({ "unix_ms": ms }))
		}
		"settings.get" => {
			let key = params
				.get("key")
				.and_then(Value::as_str)
				.ok_or_else(|| PluginError::invalid("settings.get 缺 key"))?;
			let value = SETTINGS.with(|s| s.borrow().get(key).cloned()).unwrap_or(Value::Null);
			Ok(json!({ "value": value }))
		}
		"settings.set" => {
			let key = params
				.get("key")
				.and_then(Value::as_str)
				.ok_or_else(|| PluginError::invalid("settings.set 缺 key"))?;
			let value = params.get("value").cloned().unwrap_or(Value::Null);
			set_setting(key, value);
			Ok(json!({}))
		}
		"http.request" => http_request(&params),
		other => Err(PluginError::unsupported(format!("native-host 未实现方法 {other}"))),
	}
}

fn http_request(params: &Value) -> Result<Value, PluginError> {
	let method = params
		.get("method")
		.and_then(Value::as_str)
		.ok_or_else(|| PluginError::invalid("http.request 缺 method"))?;
	let url = params
		.get("url")
		.and_then(Value::as_str)
		.ok_or_else(|| PluginError::invalid("http.request 缺 url"))?;
	let b64 = base64::engine::general_purpose::STANDARD;

	let mut req = ureq::request(method, url);
	if let Some(headers) = params.get("headers").and_then(Value::as_object) {
		for (k, v) in headers {
			if let Some(v) = v.as_str() {
				req = req.set(k, v);
			}
		}
	}
	if let Some(t) = params.get("timeout_ms").and_then(Value::as_u64) {
		req = req.timeout(Duration::from_millis(t));
	}
	let body = match params.get("body_b64").and_then(Value::as_str) {
		Some(s) => Some(
			b64.decode(s)
				.map_err(|e| PluginError::invalid(format!("body_b64 解码失败: {e}")))?,
		),
		None => None,
	};
	let result = match &body {
		Some(bytes) => req.send_bytes(bytes),
		None => req.call(),
	};
	// 与真宿主同语义：拿到响应（含 4xx/5xx）都算 ok 带 status；仅传输层失败报错
	let resp = match result {
		Ok(resp) => resp,
		Err(ureq::Error::Status(_, resp)) => resp,
		Err(e) => return Err(PluginError::internal(format!("http 请求失败: {e}"))),
	};
	let status = resp.status();
	let mut headers = BTreeMap::new();
	for name in resp.headers_names() {
		if let Some(v) = resp.header(&name) {
			headers.insert(name, v.to_string());
		}
	}
	let mut bytes = Vec::new();
	resp.into_reader()
		.read_to_end(&mut bytes)
		.map_err(|e| PluginError::internal(format!("读响应体失败: {e}")))?;
	Ok(json!({ "status": status, "headers": headers, "body_b64": b64.encode(&bytes) }))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn settings_round_trip_through_host_wrappers() {
		clear_settings();
		crate::host::settings_set("k", json!("v")).unwrap();
		assert_eq!(crate::host::settings_get("k").unwrap(), json!("v"));
		assert_eq!(crate::host::settings_get("missing").unwrap(), Value::Null);
	}

	#[test]
	fn now_ms_is_sane() {
		let ms = crate::host::now_ms().unwrap();
		assert!(ms > 1_600_000_000_000, "unix 毫秒应在 2020 年之后: {ms}");
	}

	#[test]
	fn unknown_method_is_unsupported() {
		assert_eq!(call("nope", json!({})).unwrap_err().code, "unsupported");
	}
}
