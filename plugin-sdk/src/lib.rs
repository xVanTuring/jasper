//! # jasper-plugin-sdk
//!
//! Jasper 后端插件（wasm，spec 0.3）的 Rust SDK：封装 ABI（spec §6），
//! 作者只写业务函数/类型，用 [`register!`] 一行接入。
//!
//! ```ignore
//! use jasper_plugin_sdk as sdk;
//! use sdk::core::model::Note;
//!
//! fn before_save(mut note: Note) -> Result<Note, String> {
//!     note.body = note.body.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
//!     Ok(note)
//! }
//!
//! sdk::register! { before_save: before_save }
//! // 或：sdk::register! { storage: MyStorage }（impl sdk::storage::Storage）
//! // 或组合任意子集：sdk::register! { before_save: before_save, command: run, ui: render }
//! ```
//!
//! 插件 crate 须为 `crate-type = ["cdylib"]`，目标 `wasm32-unknown-unknown`。

pub mod host;
#[cfg(all(not(target_arch = "wasm32"), feature = "native-host"))]
pub mod native_host;
pub mod rt;
pub mod storage;

pub use jasper_core as core;
pub use rt::PluginError;
// 宏与作者代码共用同一版本 serde_json
pub use serde_json;

// wasmi 沙箱无熵源：给 getrandom 注册报错实现（仅为编译通过）。
// 插件不该自造 Joplin id——id 由宿主生成；误调 core::serialize::new_id 会 panic 该次调用（宿主可容错）。
#[cfg(target_arch = "wasm32")]
mod rand_shim {
    fn no_entropy(_buf: &mut [u8]) -> Result<(), getrandom::Error> {
        Err(getrandom::Error::UNSUPPORTED)
    }
    getrandom::register_custom_getrandom!(no_entropy);
}

/// 生成 `plugin_alloc` / `plugin_free` / `plugin_dispatch` 三个 ABI 导出（spec §6.1/§6.2）。
/// 可挂载（任意顺序、可组合）：
/// - `before_save`：`fn(Note) -> Result<Note, String>`
/// - `storage`：impl [`storage::Storage`] 的类型
/// - `command`：`fn(&str /* 命令 id */, Value /* args */) -> Result<Value, PluginError>`
/// - `ui`：`fn(&str /* view */, Value /* state */) -> Result<Value, PluginError>`（返回 UiNode 树，spec §9.3）
/// - `editor`：`fn(&str /* phase: before-save|input */, String /* text */) -> Result<String, PluginError>`
///   （编辑期文本变换，`contributes.editor` → `editor.transform`，spec §3.7/§6.5）
#[macro_export]
macro_rules! register {
    ( $($rest:tt)* ) => {
        $crate::__register_accum! { hook = (), storage = (), command = (), ui = (), editor = (); $($rest)* }
    };
}

// 累积器：按键收集五个可选槽位，与书写顺序无关。
#[doc(hidden)]
#[macro_export]
macro_rules! __register_accum {
    ( hook = ($($h:path)?), storage = ($($s:ty)?), command = ($($c:path)?), ui = ($($u:path)?), editor = ($($e:path)?); before_save: $f:path $(, $($rest:tt)*)? ) => {
        $crate::__register_accum! { hook = ($f), storage = ($($s)?), command = ($($c)?), ui = ($($u)?), editor = ($($e)?); $($($rest)*)? }
    };
    ( hook = ($($h:path)?), storage = ($($s:ty)?), command = ($($c:path)?), ui = ($($u:path)?), editor = ($($e:path)?); storage: $t:ty $(, $($rest:tt)*)? ) => {
        $crate::__register_accum! { hook = ($($h)?), storage = ($t), command = ($($c)?), ui = ($($u)?), editor = ($($e)?); $($($rest)*)? }
    };
    ( hook = ($($h:path)?), storage = ($($s:ty)?), command = ($($c:path)?), ui = ($($u:path)?), editor = ($($e:path)?); command: $f:path $(, $($rest:tt)*)? ) => {
        $crate::__register_accum! { hook = ($($h)?), storage = ($($s)?), command = ($f), ui = ($($u)?), editor = ($($e)?); $($($rest)*)? }
    };
    ( hook = ($($h:path)?), storage = ($($s:ty)?), command = ($($c:path)?), ui = ($($u:path)?), editor = ($($e:path)?); ui: $f:path $(, $($rest:tt)*)? ) => {
        $crate::__register_accum! { hook = ($($h)?), storage = ($($s)?), command = ($($c)?), ui = ($f), editor = ($($e)?); $($($rest)*)? }
    };
    ( hook = ($($h:path)?), storage = ($($s:ty)?), command = ($($c:path)?), ui = ($($u:path)?), editor = ($($e:path)?); editor: $f:path $(, $($rest:tt)*)? ) => {
        $crate::__register_accum! { hook = ($($h)?), storage = ($($s)?), command = ($($c)?), ui = ($($u)?), editor = ($f); $($($rest)*)? }
    };
    ( hook = ($($h:path)?), storage = ($($s:ty)?), command = ($($c:path)?), ui = ($($u:path)?), editor = ($($e:path)?); ) => {
        $crate::__register_dispatch! { hook = ($($h)?), storage = ($($s)?), command = ($($c)?), ui = ($($u)?), editor = ($($e)?) }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __register_dispatch {
    ( hook = ($($hook:path)?), storage = ($($st:ty)?), command = ($($cmd:path)?), ui = ($($ui:path)?), editor = ($($editor:path)?) ) => {
        // 业务路由：storage.* 方法族优先，其余按 method 匹配。native 下仅供测试。
        #[allow(dead_code)]
        fn __jasper_dispatch(
            method: &str,
            params: $crate::serde_json::Value,
        ) -> ::std::result::Result<$crate::serde_json::Value, $crate::PluginError> {
            $(
                if let Some(r) = $crate::storage::dispatch_storage::<$st>(method, &params) {
                    return r;
                }
            )?
            match method {
                "metadata" => Ok($crate::serde_json::json!({ "ok": true })),
                $(
                    "hook.before_save" => {
                        let note: $crate::core::model::Note = $crate::serde_json::from_value(
                            params.get("note").cloned().unwrap_or($crate::serde_json::Value::Null),
                        )
                        .map_err(|e| $crate::PluginError::invalid(format!("note 解析失败: {e}")))?;
                        let hook: fn(
                            $crate::core::model::Note,
                        ) -> ::std::result::Result<$crate::core::model::Note, String> = $hook;
                        let out = hook(note).map_err($crate::PluginError::internal)?;
                        Ok($crate::serde_json::json!({ "note": out }))
                    }
                )?
                $(
                    "command" => {
                        let id = params
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let args = params.get("args").cloned().unwrap_or($crate::serde_json::Value::Null);
                        let run: fn(
                            &str,
                            $crate::serde_json::Value,
                        ) -> ::std::result::Result<$crate::serde_json::Value, $crate::PluginError> = $cmd;
                        run(&id, args)
                    }
                )?
                $(
                    "ui" => {
                        let view = params
                            .get("view")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let state = params.get("state").cloned().unwrap_or($crate::serde_json::Value::Null);
                        let render: fn(
                            &str,
                            $crate::serde_json::Value,
                        ) -> ::std::result::Result<$crate::serde_json::Value, $crate::PluginError> = $ui;
                        render(&view, state)
                    }
                )?
                $(
                    "editor.transform" => {
                        let phase = params
                            .get("phase")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let text = params
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let transform: fn(
                            &str,
                            String,
                        ) -> ::std::result::Result<String, $crate::PluginError> = $editor;
                        let out = transform(&phase, text)?;
                        Ok($crate::serde_json::json!({ "text": out }))
                    }
                )?
                other => Err($crate::PluginError::unsupported(format!("未知方法: {other}"))),
            }
        }

        #[cfg(target_arch = "wasm32")]
        mod __jasper_plugin_abi {
            #[no_mangle]
            pub extern "C" fn plugin_alloc(size: u32) -> u32 {
                $crate::rt::alloc(size as usize) as u32
            }
            #[no_mangle]
            pub extern "C" fn plugin_free(ptr: u32, size: u32) {
                $crate::rt::free(ptr as usize, size as usize)
            }
            #[no_mangle]
            pub extern "C" fn plugin_dispatch(ptr: u32, len: u32) -> u64 {
                $crate::rt::dispatch(ptr as usize, len as usize, super::__jasper_dispatch)
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use crate::core::model::{MarkupLanguage, Note};
    use serde_json::json;

    fn trim_hook(mut note: Note) -> Result<Note, String> {
        note.body = note.body.lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
        Ok(note)
    }

    crate::register! { before_save: trim_hook }

    fn sample_note() -> Note {
        Note {
            id: "a".repeat(32),
            parent_id: String::new(),
            title: "t".into(),
            body: "x  \ny\t".into(),
            created_time: 0,
            updated_time: 0,
            markup_language: MarkupLanguage::Markdown,
            is_todo: false,
            todo_completed: false,
            is_conflict: false,
            source_url: String::new(),
            order: 0,
        }
    }

    #[test]
    fn dispatch_metadata_and_hook() {
        let r = __jasper_dispatch("metadata", json!(null)).unwrap();
        assert_eq!(r, json!({ "ok": true }));

        let r = __jasper_dispatch("hook.before_save", json!({ "note": sample_note() })).unwrap();
        assert_eq!(r["note"]["body"], "x\ny");

        let e = __jasper_dispatch("nope", json!(null)).unwrap_err();
        assert_eq!(e.code, "unsupported");
    }
}

#[cfg(test)]
mod ui_slot_tests {
    use crate::PluginError;
    use serde_json::{json, Value};

    fn run(id: &str, args: Value) -> Result<Value, PluginError> {
        Ok(json!({ "echoed": { "id": id, "args": args } }))
    }

    fn render(view: &str, state: Value) -> Result<Value, PluginError> {
        Ok(json!({
            "type": "markdown",
            "props": { "source": format!("view={view}") },
            "children": [ { "type": "button", "props": { "label": "go", "command": "x", "state": state } } ],
        }))
    }

    crate::register! { command: run, ui: render }

    #[test]
    fn dispatch_routes_ui_and_command() {
        let r = __jasper_dispatch("ui", json!({ "view": "main", "state": { "n": 1 } })).unwrap();
        assert_eq!(r["type"], "markdown");
        assert_eq!(r["props"]["source"], "view=main");
        assert_eq!(r["children"][0]["props"]["state"]["n"], 1);

        let r = __jasper_dispatch("command", json!({ "id": "c1", "args": { "a": 2 } })).unwrap();
        assert_eq!(r["echoed"]["id"], "c1");

        // 未挂 before_save → 该方法落到 unsupported
        let e = __jasper_dispatch("hook.before_save", json!({ "note": null })).unwrap_err();
        assert_eq!(e.code, "unsupported");
    }
}

#[cfg(test)]
mod editor_slot_tests {
    use crate::PluginError;
    use serde_json::json;

    // 编辑期变换：把文本连同相位打成可观测的结果
    fn transform(phase: &str, text: String) -> Result<String, PluginError> {
        Ok(format!("[{phase}] {}", text.to_uppercase()))
    }

    crate::register! { editor: transform }

    #[test]
    fn dispatch_routes_editor_transform() {
        let r = __jasper_dispatch("editor.transform", json!({ "phase": "input", "text": "hi there" })).unwrap();
        assert_eq!(r["text"], "[input] HI THERE");

        // 未挂 command → 落到 unsupported（证明只注册了 editor 槽）
        let e = __jasper_dispatch("command", json!({ "id": "x" })).unwrap_err();
        assert_eq!(e.code, "unsupported");
    }
}
