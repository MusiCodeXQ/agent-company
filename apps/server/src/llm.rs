use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};

use crate::{model_profiles::ModelProfileCredentials, models::TokenUsageSummary};

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct ChatResponse {
    pub content: String,
    pub usage: TokenUsageSummary,
}

pub async fn chat(
    profile: &ModelProfileCredentials,
    system_prompt: &str,
    messages: &[ChatMessage],
) -> Result<ChatResponse> {
    if !profile.enabled {
        return Err(anyhow!("model profile is disabled"));
    }
    if profile.protocol.eq_ignore_ascii_case("mock") {
        let latest = messages
            .iter()
            .rev()
            .find(|message| message.role == "user")
            .map(|message| message.content.as_str())
            .unwrap_or("这条消息");
        return Ok(ChatResponse {
            content: format!(
                "我已收到你的消息：{}。当前运行在 Mock Provider；切换到 OpenAI Compatible Provider 后，我会继续沿用同一条持久化工作流。",
                latest
            ),
            usage: TokenUsageSummary::default(),
        });
    }
    if !is_openai_compatible(&profile.protocol) {
        return Err(anyhow!("unsupported model protocol: {}", profile.protocol));
    }
    let endpoint = format!(
        "{}/chat/completions",
        profile.base_url.trim_end_matches('/')
    );
    let mut request = reqwest::Client::new()
        .post(endpoint)
        .timeout(Duration::from_secs(120))
        .json(&json!({
            "model": profile.model,
            "temperature": 0.2,
            "messages": std::iter::once(json!({ "role": "system", "content": system_prompt }))
                .chain(messages.iter().map(|message| json!({ "role": message.role, "content": message.content })))
                .collect::<Vec<_>>()
        }));
    if let Some(api_key) = &profile.api_key {
        request = request.bearer_auth(api_key);
    }
    let response = request
        .send()
        .await
        .context("send chat completion request")?;
    let status = response.status();
    let body = response
        .text()
        .await
        .context("read chat completion response")?;
    if !status.is_success() {
        return Err(anyhow!(
            "model request failed with {}: {}",
            status,
            safe_body(&body, profile.api_key.as_deref())
        ));
    }
    let value: Value = serde_json::from_str(&body).context("parse chat completion response")?;
    let content = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|content| !content.trim().is_empty())
        .ok_or_else(|| anyhow!("model response did not contain choices[0].message.content"))?;
    Ok(ChatResponse {
        content,
        usage: parse_usage(&value),
    })
}

fn parse_usage(value: &Value) -> TokenUsageSummary {
    let usage = value.get("usage").unwrap_or(&Value::Null);
    let input_tokens = first_token_value(usage, &["prompt_tokens", "input_tokens"]);
    let output_tokens = first_token_value(usage, &["completion_tokens", "output_tokens"]);
    let cache_tokens = usage
        .pointer("/prompt_tokens_details/cached_tokens")
        .and_then(Value::as_i64)
        .or_else(|| {
            usage
                .pointer("/input_tokens_details/cached_tokens")
                .and_then(Value::as_i64)
        })
        .or_else(|| usage.get("cache_read_input_tokens").and_then(Value::as_i64))
        .or_else(|| usage.get("cached_tokens").and_then(Value::as_i64))
        .unwrap_or(0)
        .max(0);
    let total_tokens = usage
        .get("total_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(input_tokens + output_tokens)
        .max(0);
    TokenUsageSummary {
        input_tokens,
        output_tokens,
        cache_tokens,
        total_tokens,
    }
}

fn first_token_value(usage: &Value, keys: &[&str]) -> i64 {
    keys.iter()
        .find_map(|key| usage.get(*key).and_then(Value::as_i64))
        .unwrap_or(0)
        .max(0)
}

pub async fn test_connection(profile: &ModelProfileCredentials) -> Result<String> {
    if !profile.enabled {
        return Err(anyhow!("model profile is disabled"));
    }
    if profile.protocol.eq_ignore_ascii_case("mock") {
        return Ok("Mock Provider 可用".to_string());
    }
    if !is_openai_compatible(&profile.protocol) {
        return Err(anyhow!("unsupported model protocol: {}", profile.protocol));
    }
    let endpoint = format!("{}/models", profile.base_url.trim_end_matches('/'));
    let mut request = reqwest::Client::new()
        .get(endpoint)
        .timeout(Duration::from_secs(15));
    if let Some(api_key) = &profile.api_key {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().await.context("send provider test request")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!(
            "provider returned {}: {}",
            status,
            safe_body(&body, profile.api_key.as_deref())
        ));
    }
    Ok(format!("连接成功（HTTP {}）", status))
}

fn is_openai_compatible(protocol: &str) -> bool {
    protocol.eq_ignore_ascii_case("openai_compatible")
        || protocol.eq_ignore_ascii_case("openai-compatible")
        || protocol.eq_ignore_ascii_case("openai")
}

fn truncate(value: &str) -> String {
    let mut result = value.chars().take(240).collect::<String>();
    if value.chars().count() > 240 {
        result.push('…');
    }
    result
}

fn safe_body(value: &str, api_key: Option<&str>) -> String {
    let redacted = api_key
        .filter(|key| !key.is_empty())
        .map(|key| value.replace(key, "[REDACTED]"))
        .unwrap_or_else(|| value.to_string());
    truncate(&redacted)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::parse_usage;

    #[test]
    fn parses_openai_token_usage_with_cached_prompt_tokens() {
        let usage = parse_usage(&json!({
            "usage": {
                "prompt_tokens": 120,
                "completion_tokens": 30,
                "total_tokens": 150,
                "prompt_tokens_details": { "cached_tokens": 80 }
            }
        }));
        assert_eq!(usage.input_tokens, 120);
        assert_eq!(usage.output_tokens, 30);
        assert_eq!(usage.cache_tokens, 80);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn parses_input_output_compatible_usage_and_derives_total() {
        let usage = parse_usage(&json!({
            "usage": {
                "input_tokens": 50,
                "output_tokens": 12,
                "cache_read_input_tokens": 20
            }
        }));
        assert_eq!(usage.input_tokens, 50);
        assert_eq!(usage.output_tokens, 12);
        assert_eq!(usage.cache_tokens, 20);
        assert_eq!(usage.total_tokens, 62);
    }
}
