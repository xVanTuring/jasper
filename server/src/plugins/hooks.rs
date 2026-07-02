//! before-save 钩子（spec §8）：保存前把笔记交给订阅插件串联改写。
//! 铁律：**绝不因插件失败丢用户数据**——任一插件出错即跳过它，沿用上一步结果。

use super::runtime::CallClass;
use super::PluginHost;
use crate::model::Note;
use std::sync::Arc;

/// 串联执行（按插件 id 序 = 加载顺序）。阻塞（内部跑 wasm），调用方套 spawn_blocking。
pub fn run_before_save(host: &Arc<PluginHost>, mut note: Note) -> Note {
    for id in host.before_save_plugins() {
        let params = serde_json::json!({ "note": note });
        match host.dispatch(&id, "hook.before_save", params, CallClass::Normal) {
            Ok(result) => {
                let returned = result.get("note").cloned().unwrap_or(serde_json::Value::Null);
                match serde_json::from_value::<Note>(returned) {
                    Ok(n) => note = n,
                    Err(e) => eprintln!("[plugin:{id}] before_save 返回的 note 解析失败，跳过该插件: {e}"),
                }
            }
            Err(e) => eprintln!("[plugin:{id}] before_save 失败，跳过该插件: {e}"),
        }
    }
    note
}

#[cfg(test)]
mod tests {
    use crate::api::{router, AppState};
    use crate::config::ConfigStore;
    use crate::library::Library;
    use crate::plugins::PluginHost;
    use crate::serialize;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::Value;
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex, RwLock};
    use tower::ServiceExt;

    fn examples_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins-examples")
    }

    async fn send(state: Arc<AppState>, method: &str, uri: &str, body: String) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap();
        let resp = router(state).oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    /// 全链路：trim-trailing 插件启用后，PUT /api/notes/{id} 的行尾空白被去除并落盘；
    /// 一个必失败的钩子插件（testbed 不支持 hook.before_save）不影响保存。
    #[tokio::test(flavor = "multi_thread")]
    async fn before_save_trims_and_survives_broken_plugin() {
        let trim_wasm = examples_dir().join("trim-trailing/plugin.wasm");
        let testbed_wasm = examples_dir().join("testbed/plugin.wasm");
        if !trim_wasm.exists() || !testbed_wasm.exists() {
            eprintln!("跳过：示例插件未构建（先跑 plugins-examples/build-wasm.sh）");
            return;
        }

        // 插件目录：trim-trailing 照抄；broken-hook 用 testbed 的 wasm + 订阅 before-save 的 manifest
        // （testbed 不认识 hook.before_save → 每次调用失败 → 钩子链应跳过它）
        let plug_dir = tempfile::tempdir().unwrap();
        let trim_dst = plug_dir.path().join("trim-trailing");
        std::fs::create_dir_all(&trim_dst).unwrap();
        std::fs::copy(examples_dir().join("trim-trailing/manifest.toml"), trim_dst.join("manifest.toml")).unwrap();
        std::fs::copy(&trim_wasm, trim_dst.join("plugin.wasm")).unwrap();
        let broken_dst = plug_dir.path().join("broken-hook");
        std::fs::create_dir_all(&broken_dst).unwrap();
        std::fs::write(
            broken_dst.join("manifest.toml"),
            "id = \"broken-hook\"\nname = \"Broken\"\nversion = \"1\"\napiVersion = \"0.2\"\n[backend]\nwasm = \"plugin.wasm\"\nhooks = [\"before-save\"]\n",
        )
        .unwrap();
        std::fs::copy(&testbed_wasm, broken_dst.join("plugin.wasm")).unwrap();

        // 数据源：临时本地库 + 一篇现成笔记
        let src_dir = tempfile::tempdir().unwrap();
        let note_id = serialize::new_id();
        let note_md = serialize::new_note_md(&note_id, "", "标题", "原正文", false, serialize::now_ms());
        std::fs::write(src_dir.path().join(format!("{note_id}.md")), &note_md).unwrap();

        let config = Arc::new(Mutex::new(ConfigStore::in_memory().unwrap()));
        let host = PluginHost::init_at(plug_dir.path().to_path_buf(), config.clone()).unwrap();
        host.set_enabled("trim-trailing", true).unwrap();
        host.set_enabled("broken-hook", true).unwrap();

        let state = Arc::new(AppState {
            library: RwLock::new(Library::default()),
            storage: RwLock::new(None),
            config,
            cache: crate::cache::CacheStore::in_memory().unwrap(),
            read_only: AtomicBool::new(false),
            plugins: Some(host),
        });

        // 连接数据源（走正式 apply_config 路径）
        let cfg_body = serde_json::json!({
            "source_type": "local",
            "local_path": src_dir.path().to_string_lossy(),
        })
        .to_string();
        let (st, body) = send(state.clone(), "PUT", "/api/config", cfg_body).await;
        assert_eq!(st, StatusCode::OK);
        assert_eq!(body["ok"], true, "{body}");

        // 保存带行尾空白的正文
        let upd = serde_json::json!({ "title": "标题", "body": "行一  \n行二\t\n行三" }).to_string();
        let (st, body) = send(state.clone(), "PUT", &format!("/api/notes/{note_id}"), upd).await;
        assert_eq!(st, StatusCode::OK, "{body}");
        assert_eq!(body["body"], "行一\n行二\n行三", "trim 生效且 broken 插件被跳过");

        // 落盘内容一致 + 元数据逐字保留（id 行还在）
        let on_disk = std::fs::read_to_string(src_dir.path().join(format!("{note_id}.md"))).unwrap();
        assert!(on_disk.contains("行一\n行二\n行三"));
        assert!(on_disk.contains(&format!("id: {note_id}")));
    }
}
