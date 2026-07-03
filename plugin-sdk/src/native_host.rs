//! 测试用宿主替身（仅 native + `feature = "native-host"`）：让插件的集成测试
//! 不进 wasm 沙箱也能走 host_call——`http.request` 用 ureq 发真网络请求、
//! `time.now` 用系统时钟、settings 是线程本地 map（用 [`set_setting`] 注入）；
//! `notes.*` 是线程本地内存笔记库（[`put_note`]/[`put_folder`] 注入、
//! [`set_write_pending`] 模拟写确认）、`ai.complete` 回预置文本（[`set_ai_reply`]
//! 固定回复 / [`push_ai_reply`] 逐轮队列；[`last_ai_messages`] 供断言提示词）。
//!
//! ⚠ 只供插件自己的测试：没有能力门控、没有限额、没有沙箱，安全语义与真宿主不同；
//! 各方法的参数/返回形状与宿主 host_api 逐字一致（见 host.rs 的封装）。
//! 全部存储是 thread_local——cargo test 每个测试独立线程，天然隔离；
//! 测试内自行跨线程时注意各线程各有一份。

use crate::rt::PluginError;
use base64::Engine as _;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::io::Read as _;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

thread_local! {
	static SETTINGS: RefCell<BTreeMap<String, Value>> = RefCell::new(BTreeMap::new());
	// system.locale 替身（spec 0.4）：默认 en，用 [`set_locale`] 注入
	static LOCALE: RefCell<String> = RefCell::new(String::from("en"));
	// notes.*/ai.complete 替身状态（spec 0.3）：内存笔记库 + 预置 AI 回复 + 写确认开关
	static NOTES: RefCell<BTreeMap<String, Value>> = RefCell::new(BTreeMap::new());
	static FOLDERS: RefCell<Vec<Value>> = RefCell::new(Vec::new());
	static AI_REPLY: RefCell<Option<String>> = RefCell::new(None);
	static AI_REPLY_QUEUE: RefCell<std::collections::VecDeque<String>> = RefCell::new(std::collections::VecDeque::new());
	static AI_LAST_MESSAGES: RefCell<Option<Value>> = RefCell::new(None);
	static WRITE_PENDING: Cell<bool> = const { Cell::new(false) };
}

/// notes.create 直写模式下替身分配的固定假 id（32hex；真 id 由宿主生成）。
pub const FAKE_CREATED_ID: &str = "0123456789abcdef0123456789abcdef";

/// 测试注入：预置一个 settings 键值（等价于用户在设置页填好）。
pub fn set_setting(key: &str, value: Value) {
	SETTINGS.with(|s| s.borrow_mut().insert(key.to_string(), value));
}

/// 测试清场：清空本线程的 settings。
pub fn clear_settings() {
	SETTINGS.with(|s| s.borrow_mut().clear());
}

/// 测试注入：预置 `system.locale` 返回的 UI 语言（等价于用户选了某界面语言）。
pub fn set_locale(code: &str) {
	LOCALE.with(|l| *l.borrow_mut() = code.to_string());
}

/// 组一个 core Note 形状的 JSON（与宿主 notes.get 返回逐字段一致），供 [`put_note`] 与断言用。
pub fn make_note(id: &str, parent_id: &str, title: &str, body: &str) -> Value {
	json!({
		"id": id,
		"parent_id": parent_id,
		"title": title,
		"body": body,
		"created_time": 1_700_000_000_000_i64,
		"updated_time": 1_700_000_000_000_i64,
		"markup_language": 1,
		"is_todo": false,
		"todo_completed": false,
		"is_conflict": false,
		"source_url": "",
		"order": 0,
	})
}

/// 测试注入：预置一条笔记（core Note 形状 JSON，见 [`make_note`]），键 = note["id"]。
pub fn put_note(note: Value) {
	let id = note.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
	NOTES.with(|n| n.borrow_mut().insert(id, note));
}

/// 测试注入：预置一个笔记本。
pub fn put_folder(id: &str, title: &str, parent_id: &str) {
	FOLDERS.with(|f| {
		f.borrow_mut().push(json!({ "id": id, "title": title, "parent_id": parent_id }));
	});
}

/// 测试清场：清空本线程的笔记与笔记本。
pub fn clear_notes() {
	NOTES.with(|n| n.borrow_mut().clear());
	FOLDERS.with(|f| f.borrow_mut().clear());
}

/// 测试注入：预置 ai.complete 的回复（缺省 "(stub reply)"）。
pub fn set_ai_reply(reply: &str) {
	AI_REPLY.with(|r| *r.borrow_mut() = Some(reply.to_string()));
}

/// 测试注入：入队一条 ai.complete 回复（FIFO，先于 [`set_ai_reply`] 的固定回复消费）。
/// 工具循环/多轮对话类插件用它模拟逐轮不同的模型输出；队列耗尽后回落固定回复。
pub fn push_ai_reply(reply: &str) {
	AI_REPLY_QUEUE.with(|q| q.borrow_mut().push_back(reply.to_string()));
}

/// 最近一次 ai.complete 收到的 messages（断言插件组装的提示词/回喂内容用）。
pub fn last_ai_messages() -> Option<Value> {
	AI_LAST_MESSAGES.with(|m| m.borrow().clone())
}

/// 测试注入：写确认开关。true = 模拟宿主「需确认」——upsert/create 不改内存库、
/// 返回 pending:true（宿主语义见 spec §6.5）；默认 false = 直写。
pub fn set_write_pending(pending: bool) {
	WRITE_PENDING.with(|w| w.set(pending));
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
		"system.locale" => Ok(json!({ "locale": LOCALE.with(|l| l.borrow().clone()) })),
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
		"notes.get" => {
			let id = params
				.get("id")
				.and_then(Value::as_str)
				.ok_or_else(|| PluginError::invalid("notes.get 缺 id"))?;
			NOTES
				.with(|n| n.borrow().get(id).cloned())
				.map(|note| json!({ "note": note }))
				.ok_or_else(|| PluginError::not_found(format!("笔记不存在: {id}")))
		}
		"notes.search" => {
			let q = params
				.get("query")
				.and_then(Value::as_str)
				.ok_or_else(|| PluginError::invalid("notes.search 缺 query"))?
				.to_lowercase();
			let limit = params
				.get("limit")
				.and_then(Value::as_u64)
				.unwrap_or(20)
				.clamp(1, 100) as usize;
			let notes: Vec<Value> = NOTES.with(|n| {
				n.borrow()
					.values()
					.filter(|note| {
						let title = note.get("title").and_then(Value::as_str).unwrap_or("");
						let body = note.get("body").and_then(Value::as_str).unwrap_or("");
						title.to_lowercase().contains(&q) || body.to_lowercase().contains(&q)
					})
					.take(limit)
					.map(|note| {
						json!({
							"id": note["id"],
							"title": note["title"],
							"parent_id": note["parent_id"],
						})
					})
					.collect()
			});
			Ok(json!({ "notes": notes }))
		}
		"notes.list_folders" => {
			let mut folders = FOLDERS.with(|f| f.borrow().clone());
			folders.sort_by(|a, b| {
				let key = |v: &Value| {
					(
						v.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
						v.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
					)
				};
				key(a).cmp(&key(b))
			});
			Ok(json!({ "folders": folders }))
		}
		"notes.upsert" => {
			let id = params
				.get("id")
				.and_then(Value::as_str)
				.ok_or_else(|| PluginError::invalid("notes.upsert 缺 id"))?;
			let mut note = NOTES
				.with(|n| n.borrow().get(id).cloned())
				.ok_or_else(|| PluginError::not_found(format!("笔记不存在: {id}")))?;
			if let Some(t) = params.get("title").and_then(Value::as_str) {
				note["title"] = json!(t);
			}
			if let Some(b) = params.get("body").and_then(Value::as_str) {
				note["body"] = json!(b);
			}
			let pending = WRITE_PENDING.with(Cell::get);
			if !pending {
				put_note(note.clone());
			}
			Ok(json!({ "note": note, "pending": pending }))
		}
		"notes.create" => {
			let parent_id = params
				.get("parent_id")
				.and_then(Value::as_str)
				.ok_or_else(|| PluginError::invalid("notes.create 缺 parent_id"))?;
			let parent_ok = FOLDERS.with(|f| {
				f.borrow().iter().any(|v| v.get("id").and_then(Value::as_str) == Some(parent_id))
			});
			if !parent_ok {
				return Err(PluginError::invalid(format!("笔记本不存在: {parent_id}")));
			}
			let title = params.get("title").and_then(Value::as_str).unwrap_or("");
			let body = params.get("body").and_then(Value::as_str).unwrap_or("");
			let pending = WRITE_PENDING.with(Cell::get);
			// pending 提案的 id 为空串（真 id 由宿主在批准时生成，spec §6.5）
			let note = make_note(if pending { "" } else { FAKE_CREATED_ID }, parent_id, title, body);
			if !pending {
				put_note(note.clone());
			}
			Ok(json!({ "note": note, "pending": pending }))
		}
		"ai.complete" => {
			let ok = params
				.get("messages")
				.and_then(Value::as_array)
				.map(|m| !m.is_empty())
				.unwrap_or(false);
			if !ok {
				return Err(PluginError::invalid("ai.complete 需要非空 messages"));
			}
			AI_LAST_MESSAGES.with(|m| *m.borrow_mut() = params.get("messages").cloned());
			let reply = AI_REPLY_QUEUE
				.with(|q| q.borrow_mut().pop_front())
				.or_else(|| AI_REPLY.with(|r| r.borrow().clone()))
				.unwrap_or_else(|| "(stub reply)".to_string());
			Ok(json!({ "content": reply }))
		}
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
	fn system_locale_through_host_wrapper() {
		assert_eq!(crate::host::system_locale().unwrap(), "en"); // 默认
		set_locale("fr");
		assert_eq!(crate::host::system_locale().unwrap(), "fr");
		set_locale("en"); // 复位，避免污染同线程其它用例
	}

	#[test]
	fn unknown_method_is_unsupported() {
		assert_eq!(call("nope", json!({})).unwrap_err().code, "unsupported");
	}

	#[test]
	fn notes_read_paths_through_host_wrappers() {
		clear_notes();
		put_folder("f".repeat(32).as_str(), "收件箱", "");
		put_note(make_note(&"a".repeat(32), &"f".repeat(32), "购物清单", "牛奶 面包"));
		put_note(make_note(&"b".repeat(32), &"f".repeat(32), "旅行", "机票"));

		let note = crate::host::notes_get(&"a".repeat(32)).unwrap();
		assert_eq!(note.title, "购物清单");
		assert_eq!(crate::host::notes_get(&"9".repeat(32)).unwrap_err().code, "not_found");

		let hits = crate::host::notes_search("牛奶", None).unwrap();
		assert_eq!(hits.len(), 1);
		assert_eq!(hits[0].title, "购物清单");

		let folders = crate::host::notes_list_folders().unwrap();
		assert_eq!(folders.len(), 1);
		assert_eq!(folders[0].title, "收件箱");
	}

	#[test]
	fn notes_write_respects_pending_switch() {
		clear_notes();
		put_folder(&"f".repeat(32), "收件箱", "");
		put_note(make_note(&"a".repeat(32), &"f".repeat(32), "t", "old"));

		// 直写（默认）：内存库更新、pending=false
		set_write_pending(false);
		let r = crate::host::notes_upsert(&"a".repeat(32), None, Some("new")).unwrap();
		assert!(!r.pending);
		assert_eq!(crate::host::notes_get(&"a".repeat(32)).unwrap().body, "new");

		let created = crate::host::notes_create(&"f".repeat(32), Some("新笔记"), None).unwrap();
		assert!(!created.pending);
		assert_eq!(created.note.id, FAKE_CREATED_ID);

		// 提案模式：不落库、pending=true、create 无 id
		set_write_pending(true);
		let r = crate::host::notes_upsert(&"a".repeat(32), None, Some("newer")).unwrap();
		assert!(r.pending);
		assert_eq!(crate::host::notes_get(&"a".repeat(32)).unwrap().body, "new");
		let created = crate::host::notes_create(&"f".repeat(32), Some("x"), None).unwrap();
		assert!(created.pending);
		assert_eq!(created.note.id, "");
		set_write_pending(false);

		// parent 不存在 → invalid
		assert_eq!(crate::host::notes_create(&"9".repeat(32), None, None).unwrap_err().code, "invalid");
	}

	#[test]
	fn ai_complete_returns_injected_reply() {
		set_ai_reply("你好，这是替身回复");
		let msgs = [crate::host::Message::user("hi")];
		assert_eq!(crate::host::ai_complete(&msgs, None).unwrap(), "你好，这是替身回复");
		assert_eq!(
			crate::host::ai_complete(&[], None).unwrap_err().code,
			"invalid",
			"空 messages 应报 invalid"
		);
	}

	#[test]
	fn ai_reply_queue_pops_fifo_then_falls_back() {
		set_ai_reply("固定回复");
		push_ai_reply("第一轮");
		push_ai_reply("第二轮");
		let msgs = [crate::host::Message::user("hi")];
		assert_eq!(crate::host::ai_complete(&msgs, None).unwrap(), "第一轮");
		assert_eq!(crate::host::ai_complete(&msgs, None).unwrap(), "第二轮");
		assert_eq!(crate::host::ai_complete(&msgs, None).unwrap(), "固定回复", "队列耗尽回落固定回复");
		let recorded = last_ai_messages().expect("应记录最近一次 messages");
		assert_eq!(recorded[0]["content"], "hi");
	}
}
