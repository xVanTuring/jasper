//! `ai.complete` 的宿主实现（spec 0.3 §6.5，能力 host:ai）。
//!
//! 密钥/端点在宿主（ConfigStore 的 AiConfig），插件永不可见。genai 承担协议差异：
//! provider="anthropic" → /v1/messages；provider="openai" → chat-completions 兼容协议
//! （base_url 可指 Ollama/DeepSeek/各类中转）。genai 是 async（reqwest）——本函数在
//! spawn_blocking 线程上跑，经 NotesCtx.handle block_on；网络等待计入 io_time（CPU 墙钟豁免）。

use super::runtime::NotesCtx;
use genai::adapter::AdapterKind;
use genai::chat::{ChatMessage, ChatOptions, ChatRequest};
use genai::resolver::{AuthData, Endpoint};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

type HostResult = Result<Value, (String, String)>;

fn invalid(msg: impl Into<String>) -> (String, String) {
    ("invalid".to_string(), msg.into())
}

fn internal(msg: impl Into<String>) -> (String, String) {
    ("internal".to_string(), msg.into())
}

/// 解析 messages（非空，role ∈ system|user|assistant）。
fn parse_messages(params: &Value) -> Result<Vec<ChatMessage>, (String, String)> {
    let arr = params
        .get("messages")
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| invalid("ai.complete 需要非空 messages"))?;
    let mut out = Vec::with_capacity(arr.len());
    for m in arr {
        let role = m.get("role").and_then(Value::as_str).unwrap_or("");
        let content = m
            .get("content")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("message 缺 content"))?
            .to_string();
        out.push(match role {
            "system" => ChatMessage::system(content),
            "user" => ChatMessage::user(content),
            "assistant" => ChatMessage::assistant(content),
            other => return Err(invalid(format!("非法 role: {other:?}"))),
        });
    }
    Ok(out)
}

pub fn complete(n: &NotesCtx, io_time: &mut Duration, params: &Value) -> HostResult {
    let messages = parse_messages(params)?;

    // options 钳制（spec §6.5）：temperature 0..=2、max_tokens 1..=32768
    let opts = params.get("options").cloned().unwrap_or(Value::Null);
    let mut chat_opts = ChatOptions::default();
    if let Some(t) = opts.get("temperature").and_then(Value::as_f64) {
        chat_opts = chat_opts.with_temperature(t.clamp(0.0, 2.0));
    }
    if let Some(mt) = opts.get("max_tokens").and_then(Value::as_u64) {
        chat_opts = chat_opts.with_max_tokens(mt.clamp(1, 32_768) as u32);
    }

    // 宿主配置校验（未配置 → internal，spec §6.5：不用 unsupported，避免被误读为宿主太旧）
    let ai = n.ai.clone();
    let adapter = match ai.provider.as_str() {
        "anthropic" => AdapterKind::Anthropic,
        "openai" => AdapterKind::OpenAI,
        "" => return Err(internal("AI 未配置：请在设置页「AI」段配置 provider / API Key / 模型")),
        other => return Err(internal(format!("不支持的 AI provider: {other:?}（支持 anthropic|openai）"))),
    };
    if ai.api_key.is_empty() {
        return Err(internal("AI 未配置 API Key：请在设置页「AI」段填写"));
    }
    let model = opts
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .or_else(|| Some(ai.model.clone()).filter(|s| !s.is_empty()))
        .ok_or_else(|| internal("AI 未配置模型：请在设置页「AI」段填写（或经 options.model 指定）"))?;

    // 绑定 provider（不从模型名猜）+ 注入密钥 + 可选自定义端点
    let api_key = ai.api_key.clone();
    let base_url = ai.base_url.trim().to_string();
    let client = genai::Client::builder()
        .with_adapter_kind(adapter)
        .with_auth_resolver_fn(move |_| Ok(Some(AuthData::from_single(api_key.clone()))))
        .with_service_target_resolver_fn(move |mut target: genai::ServiceTarget| {
            if !base_url.is_empty() {
                target.endpoint = Endpoint::from_owned(base_url.clone());
            }
            Ok(target)
        })
        .build();

    let req = ChatRequest::new(messages);
    let started = Instant::now();
    // spawn_blocking 线程上 block_on 合法（不在异步上下文内）
    let resp = n.handle.block_on(client.exec_chat(&model, req, Some(&chat_opts)));
    *io_time += started.elapsed();

    let resp = resp.map_err(|e| internal(format!("AI 请求失败: {e}")))?;
    let content = resp
        .first_text()
        .map(str::to_string)
        .ok_or_else(|| internal("AI 响应无文本内容"))?;
    Ok(json!({ "content": content }))
}
