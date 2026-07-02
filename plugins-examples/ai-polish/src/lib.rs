//! 一键优化：编辑器工具栏命令 → 一次性 AI 调用 → 返回优化后的正文（spec §9.4 note-toolbar 约定）。
//!
//! - AI 参数（格式/端点/密钥/模型/提示词）存在宿主的插件设置里（`settings` 能力；api_key 是 secret，前端不回显）。
//! - 支持两种格式（provider 设置切换）：
//!   - `anthropic`：`POST {api_url}/v1/messages`，`x-api-key` 认证，`system` 字段，响应 `content[].text`。
//!   - `openai`：`POST {api_url}/v1/chat/completions`，`Authorization: Bearer` 认证，system 作为一条消息，响应 `choices[0].message.content`。
//! - 网络经宿主代理（host:http），本插件不碰 socket；宿主的 CPU 墙钟不计网络等待。

use jasper_plugin_sdk as sdk;
use sdk::host::{self, http_request, HttpRequest};
use sdk::rt::PluginError;
use sdk::serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Provider {
    Anthropic,
    OpenAi,
}

impl Provider {
    fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("openai") {
            Provider::OpenAi
        } else {
            Provider::Anthropic
        }
    }
    /// 请求路径后缀（api_url 不带 /v1/...）。
    fn path(self) -> &'static str {
        match self {
            Provider::Anthropic => "/v1/messages",
            Provider::OpenAi => "/v1/chat/completions",
        }
    }
}

struct Settings {
    provider: Provider,
    api_url: String,
    api_key: String,
    model: String,
    system_prompt: String,
    max_tokens: i64,
}

fn setting_str(key: &str, default: &str) -> Result<String, PluginError> {
    let v = host::settings_get(key)?;
    Ok(v.as_str().map(str::to_string).unwrap_or_else(|| default.to_string()))
}

fn load_settings() -> Result<Settings, PluginError> {
    let api_key = setting_str("api_key", "")?;
    if api_key.trim().is_empty() {
        return Err(PluginError::invalid("尚未配置 API Key：请在插件面板的「一键优化」设置里填写"));
    }
    let provider = Provider::parse(&setting_str("provider", "anthropic")?);
    let default_url = match provider {
        Provider::Anthropic => "https://api.anthropic.com",
        Provider::OpenAi => "https://api.openai.com",
    };
    let api_url = setting_str("api_url", default_url)?;
    let api_url = api_url.trim().trim_end_matches('/').to_string();
    if !(api_url.starts_with("http://") || api_url.starts_with("https://")) {
        return Err(PluginError::invalid("API 端点须为 http(s):// URL"));
    }
    let model = setting_str("model", "claude-opus-4-8")?;
    let system_prompt = setting_str(
        "system_prompt",
        "你是文字编辑。优化用户给出的 markdown 笔记正文：修正错别字与病句，使表达更通顺简洁；保持原意、语言、语气与 markdown 结构（标题/列表/代码块/链接/图片引用一律原样保留，代码块内容不改）。只输出优化后的正文，不要任何解释或前后缀。",
    )?;
    let max_tokens = host::settings_get("max_tokens")?.as_i64().unwrap_or(16000).clamp(256, 128_000);
    Ok(Settings { provider, api_url, api_key, model, system_prompt, max_tokens })
}

/// 组装请求体（纯函数，可单测）。按 provider 走不同 schema。
fn build_request_body(s: &Settings, body: &str) -> Value {
    match s.provider {
        Provider::Anthropic => json!({
            "model": s.model,
            "max_tokens": s.max_tokens,
            "system": s.system_prompt,
            "messages": [{ "role": "user", "content": body }],
        }),
        // OpenAI Chat Completions：system 作为首条消息
        Provider::OpenAi => json!({
            "model": s.model,
            "max_tokens": s.max_tokens,
            "messages": [
                { "role": "system", "content": s.system_prompt },
                { "role": "user", "content": body },
            ],
        }),
    }
}

/// 认证头（按 provider）。
fn auth_headers(s: &Settings) -> BTreeMap<String, String> {
    let mut h = BTreeMap::new();
    h.insert("Content-Type".to_string(), "application/json".to_string());
    match s.provider {
        Provider::Anthropic => {
            h.insert("x-api-key".to_string(), s.api_key.clone());
            h.insert("anthropic-version".to_string(), "2023-06-01".to_string());
        }
        Provider::OpenAi => {
            h.insert("Authorization".to_string(), format!("Bearer {}", s.api_key));
        }
    }
    h
}

/// 顶层错误对象（两家形状一致：`{"error":{message,type}}`）。
fn error_field(resp: &Value) -> Option<PluginError> {
    let err = resp.get("error")?;
    // OpenAI 有时 error 是字符串
    let msg = err
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| err.as_str())
        .unwrap_or("未知错误");
    let kind = err.get("type").and_then(Value::as_str).unwrap_or("error");
    Some(PluginError::internal(format!("AI 端点报错（{kind}）: {msg}")))
}

/// 从响应取正文（纯函数，可单测）。按 provider 解析不同结构。
fn extract_text(provider: Provider, resp: &Value) -> Result<String, PluginError> {
    if let Some(e) = error_field(resp) {
        return Err(e);
    }
    match provider {
        Provider::Anthropic => {
            if resp.get("stop_reason").and_then(Value::as_str) == Some("refusal") {
                return Err(PluginError::internal("AI 拒绝了本次请求（safety refusal），正文未改动"));
            }
            let text: String = resp
                .get("content")
                .and_then(Value::as_array)
                .map(|blocks| {
                    blocks
                        .iter()
                        .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                        .filter_map(|b| b.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            if text.trim().is_empty() {
                return Err(PluginError::internal("AI 返回为空，正文未改动"));
            }
            if resp.get("stop_reason").and_then(Value::as_str) == Some("max_tokens") {
                return Err(PluginError::internal("输出被 max_tokens 截断，正文未改动（可在设置里调大 max_tokens）"));
            }
            Ok(text)
        }
        Provider::OpenAi => {
            let choice = resp
                .get("choices")
                .and_then(Value::as_array)
                .and_then(|c| c.first())
                .ok_or_else(|| PluginError::internal("AI 响应缺 choices，正文未改动"))?;
            let finish = choice.get("finish_reason").and_then(Value::as_str).unwrap_or("");
            if finish == "content_filter" {
                return Err(PluginError::internal("AI 拒绝了本次请求（content_filter），正文未改动"));
            }
            let text = choice
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if text.trim().is_empty() {
                return Err(PluginError::internal("AI 返回为空，正文未改动"));
            }
            if finish == "length" {
                return Err(PluginError::internal("输出被 max_tokens 截断，正文未改动（可在设置里调大 max_tokens）"));
            }
            Ok(text.to_string())
        }
    }
}

fn polish(body: &str) -> Result<Value, PluginError> {
    if body.trim().is_empty() {
        return Err(PluginError::invalid("正文为空，没有可优化的内容"));
    }
    let s = load_settings()?;
    let fmt = match s.provider {
        Provider::Anthropic => "anthropic",
        Provider::OpenAi => "openai",
    };
    host::log(
        "info",
        &format!("polish[{fmt}]: {} 字符 → {}{} ({})", body.chars().count(), s.api_url, s.provider.path(), s.model),
    );

    let req_body = sdk::serde_json::to_vec(&build_request_body(&s, body))
        .map_err(|e| PluginError::internal(format!("请求序列化失败: {e}")))?;
    let resp = http_request(&HttpRequest {
        method: "POST".to_string(),
        url: format!("{}{}", s.api_url, s.provider.path()),
        headers: auth_headers(&s),
        body: Some(req_body),
        // AI 生成可能较慢；宿主上限 120s（网络等待不计插件 CPU 墙钟）
        timeout_ms: Some(120_000),
    })?;

    let parsed: Value = sdk::serde_json::from_slice(&resp.body)
        .map_err(|e| PluginError::internal(format!("AI 响应不是 JSON（HTTP {}）: {e}", resp.status)))?;
    if !resp.is_success() {
        return Err(extract_text(s.provider, &parsed)
            .err()
            .unwrap_or_else(|| PluginError::internal(format!("AI 端点返回 HTTP {}", resp.status))));
    }
    let text = extract_text(s.provider, &parsed)?;
    Ok(json!({ "body": text }))
}

fn run_command(id: &str, args: Value) -> Result<Value, PluginError> {
    match id {
        "polish" => {
            let body = args.get("body").and_then(Value::as_str).unwrap_or("");
            polish(body)
        }
        other => Err(PluginError::unsupported(format!("未知命令: {other}"))),
    }
}

sdk::register! { command: run_command }

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(provider: Provider) -> Settings {
        Settings {
            provider,
            api_url: "https://api.example.com".into(),
            api_key: "sk-test".into(),
            model: "m".into(),
            system_prompt: "润色".into(),
            max_tokens: 16000,
        }
    }

    #[test]
    fn provider_parse_and_path() {
        assert!(matches!(Provider::parse("openai"), Provider::OpenAi));
        assert!(matches!(Provider::parse("OpenAI"), Provider::OpenAi));
        assert!(matches!(Provider::parse("anthropic"), Provider::Anthropic));
        assert!(matches!(Provider::parse("weird"), Provider::Anthropic)); // 默认
        assert_eq!(Provider::OpenAi.path(), "/v1/chat/completions");
        assert_eq!(Provider::Anthropic.path(), "/v1/messages");
    }

    #[test]
    fn anthropic_request_and_headers() {
        let s = settings(Provider::Anthropic);
        let b = build_request_body(&s, "正文");
        assert_eq!(b["system"], "润色");
        assert_eq!(b["messages"][0]["role"], "user");
        assert!(b.get("messages").unwrap().as_array().unwrap().len() == 1);
        let h = auth_headers(&s);
        assert!(h.contains_key("x-api-key"));
        assert!(h.contains_key("anthropic-version"));
        assert!(!h.contains_key("Authorization"));
    }

    #[test]
    fn openai_request_and_headers() {
        let s = settings(Provider::OpenAi);
        let b = build_request_body(&s, "正文");
        assert!(b.get("system").is_none());
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["messages"][0]["content"], "润色");
        assert_eq!(b["messages"][1]["role"], "user");
        assert_eq!(b["messages"][1]["content"], "正文");
        let h = auth_headers(&s);
        assert_eq!(h.get("Authorization").unwrap(), "Bearer sk-test");
        assert!(!h.contains_key("x-api-key"));
    }

    #[test]
    fn anthropic_extract() {
        let ok = json!({ "content": [{ "type": "text", "text": "优化后" }], "stop_reason": "end_turn" });
        assert_eq!(extract_text(Provider::Anthropic, &ok).unwrap(), "优化后");
        let refusal = json!({ "content": [], "stop_reason": "refusal" });
        assert!(extract_text(Provider::Anthropic, &refusal).unwrap_err().message.contains("拒绝"));
        let trunc = json!({ "content": [{ "type": "text", "text": "半" }], "stop_reason": "max_tokens" });
        assert!(extract_text(Provider::Anthropic, &trunc).unwrap_err().message.contains("截断"));
    }

    #[test]
    fn openai_extract() {
        let ok = json!({
            "choices": [{ "message": { "role": "assistant", "content": "优化后" }, "finish_reason": "stop" }]
        });
        assert_eq!(extract_text(Provider::OpenAi, &ok).unwrap(), "优化后");
        let filtered = json!({ "choices": [{ "message": { "content": "" }, "finish_reason": "content_filter" }] });
        assert!(extract_text(Provider::OpenAi, &filtered).unwrap_err().message.contains("拒绝"));
        let trunc = json!({ "choices": [{ "message": { "content": "半" }, "finish_reason": "length" }] });
        assert!(extract_text(Provider::OpenAi, &trunc).unwrap_err().message.contains("截断"));
        let empty = json!({ "choices": [] });
        assert!(extract_text(Provider::OpenAi, &empty).is_err());
    }

    #[test]
    fn error_field_common() {
        let anthropic_err = json!({ "type": "error", "error": { "type": "authentication_error", "message": "bad key" } });
        assert!(extract_text(Provider::Anthropic, &anthropic_err).unwrap_err().message.contains("authentication_error"));
        let openai_err = json!({ "error": { "message": "invalid model", "type": "invalid_request_error" } });
        assert!(extract_text(Provider::OpenAi, &openai_err).unwrap_err().message.contains("invalid model"));
    }
}
