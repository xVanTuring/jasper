//! 存储 provider（spec §3.9 / §6.5 `storage.*`，0.2 新增）。
//!
//! 插件实现 [`Storage`] trait，经 `register! { storage: MyType }` 接入；
//! 宿主在每次调用里传入该数据源的 config——插件应视自己为无状态（每次调用新实例）。
//! 本版一个 `register!` 只挂一个 Storage 类型：即使 manifest 声明多个
//! `[[contributes.storage]]`，也由同一类型按 config 自行区分（params.storage 不参与路由）。

use crate::rt::PluginError;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 条目文件元信息（对齐宿主 `storage::ItemStat`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemStat {
    /// 文件名，形如 `<32hex>.md`
    pub name: String,
    /// mtime（Unix 毫秒）。尽力返回真实值；0 合法但宿主失去增量缓存。
    pub updated_time: i64,
}

/// 存储后端 trait，8 方法镜像宿主 `StorageBackend`（语义约定见 spec §6.5）：
/// `delete_*` 幂等（不存在视为成功）；`init_new` 建根 + `.resource/` + 默认 info.json。
pub trait Storage: Sized {
    /// 从数据源配置（与 manifest `config_schema` 对齐的对象）构造。
    fn from_config(config: &Value) -> Result<Self, PluginError>;

    fn list_items(&self) -> Result<Vec<ItemStat>, PluginError>;
    fn get_item(&self, name: &str) -> Result<String, PluginError>;
    fn put_item(&self, name: &str, content: &str) -> Result<(), PluginError>;
    fn delete_item(&self, name: &str) -> Result<(), PluginError>;
    fn get_resource(&self, resource_id: &str) -> Result<Vec<u8>, PluginError>;
    fn put_resource(&self, resource_id: &str, data: &[u8]) -> Result<(), PluginError>;
    fn delete_resource(&self, resource_id: &str) -> Result<(), PluginError>;
    fn init_new(&self) -> Result<(), PluginError>;
}

fn str_param<'a>(params: &'a Value, key: &str) -> Result<&'a str, PluginError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::invalid(format!("缺参数 {key}")))
}

/// `storage.*` 方法路由（`register!` 生成的 dispatch 调用）。
/// 非 storage. 前缀返回 None，让调用方继续尝试其它方法族。
pub fn dispatch_storage<S: Storage>(method: &str, params: &Value) -> Option<Result<Value, PluginError>> {
    let op = method.strip_prefix("storage.")?;
    Some(run::<S>(op, params))
}

fn run<S: Storage>(op: &str, params: &Value) -> Result<Value, PluginError> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let config = params.get("config").cloned().unwrap_or(Value::Null);
    let s = S::from_config(&config)?;
    match op {
        "list_items" => {
            let items = s.list_items()?;
            Ok(json!({ "items": items }))
        }
        "get_item" => {
            let content = s.get_item(str_param(params, "name")?)?;
            Ok(json!({ "content": content }))
        }
        "put_item" => {
            s.put_item(str_param(params, "name")?, str_param(params, "content")?)?;
            Ok(json!({}))
        }
        "delete_item" => {
            s.delete_item(str_param(params, "name")?)?;
            Ok(json!({}))
        }
        "get_resource" => {
            let data = s.get_resource(str_param(params, "resource_id")?)?;
            Ok(json!({ "data_b64": b64.encode(data) }))
        }
        "put_resource" => {
            let data = b64
                .decode(str_param(params, "data_b64")?)
                .map_err(|e| PluginError::invalid(format!("data_b64 解码失败: {e}")))?;
            s.put_resource(str_param(params, "resource_id")?, &data)?;
            Ok(json!({}))
        }
        "delete_resource" => {
            s.delete_resource(str_param(params, "resource_id")?)?;
            Ok(json!({}))
        }
        "init_new" => {
            s.init_new()?;
            Ok(json!({}))
        }
        other => Err(PluginError::unsupported(format!("未知 storage 方法: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    // 纯内存假存储：验证路由与 base64 编解码（native 可跑，不涉 host_call）
    struct Mem;
    thread_local! {
        static LAST: RefCell<Option<(String, Vec<u8>)>> = const { RefCell::new(None) };
    }

    impl Storage for Mem {
        fn from_config(config: &Value) -> Result<Self, PluginError> {
            if config.get("fail").is_some() {
                return Err(PluginError::invalid("bad config"));
            }
            Ok(Mem)
        }
        fn list_items(&self) -> Result<Vec<ItemStat>, PluginError> {
            Ok(vec![ItemStat { name: "a".repeat(32) + ".md", updated_time: 42 }])
        }
        fn get_item(&self, name: &str) -> Result<String, PluginError> {
            Ok(format!("content of {name}"))
        }
        fn put_item(&self, _: &str, _: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn delete_item(&self, _: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn get_resource(&self, _: &str) -> Result<Vec<u8>, PluginError> {
            Ok(vec![0xde, 0xad, 0xbe, 0xef])
        }
        fn put_resource(&self, id: &str, data: &[u8]) -> Result<(), PluginError> {
            LAST.with(|l| *l.borrow_mut() = Some((id.to_string(), data.to_vec())));
            Ok(())
        }
        fn delete_resource(&self, _: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn init_new(&self) -> Result<(), PluginError> {
            Ok(())
        }
    }

    #[test]
    fn routes_and_encodes() {
        let r = dispatch_storage::<Mem>("storage.list_items", &json!({"config": {}}))
            .unwrap()
            .unwrap();
        assert_eq!(r["items"][0]["updated_time"], 42);

        let r = dispatch_storage::<Mem>("storage.get_resource", &json!({"config": {}, "resource_id": "x"}))
            .unwrap()
            .unwrap();
        assert_eq!(r["data_b64"], "3q2+7w=="); // base64(de ad be ef)

        dispatch_storage::<Mem>(
            "storage.put_resource",
            &json!({"config": {}, "resource_id": "y", "data_b64": "3q2+7w=="}),
        )
        .unwrap()
        .unwrap();
        LAST.with(|l| {
            let (id, data) = l.borrow().clone().unwrap();
            assert_eq!(id, "y");
            assert_eq!(data, vec![0xde, 0xad, 0xbe, 0xef]);
        });
    }

    #[test]
    fn non_storage_method_passes_through() {
        assert!(dispatch_storage::<Mem>("command", &json!({})).is_none());
    }

    #[test]
    fn bad_config_and_bad_b64_are_invalid() {
        let e = dispatch_storage::<Mem>("storage.list_items", &json!({"config": {"fail": 1}}))
            .unwrap()
            .unwrap_err();
        assert_eq!(e.code, "invalid");
        let e = dispatch_storage::<Mem>(
            "storage.put_resource",
            &json!({"config": {}, "resource_id": "y", "data_b64": "!!!"}),
        )
        .unwrap()
        .unwrap_err();
        assert_eq!(e.code, "invalid");
    }
}
