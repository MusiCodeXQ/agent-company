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
        let content = if system_prompt.contains("会议桌旁") {
            mock_meeting_reply(system_prompt, latest)
        } else {
            apply_mock_voice(
                system_prompt,
                &format!(
                    "我已收到你的消息：{}。当前运行在 Mock Provider；切换模型后会沿用同一条持久化工作流。",
                    latest
                ),
            )
        };
        return Ok(ChatResponse {
            content,
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

fn mock_meeting_reply(system_prompt: &str, latest: &str) -> String {
    let normalized = latest.trim();
    if normalized.len() < 6
        || ["大家好", "收到", "谢谢", "好的", "明白了", "辛苦了"].contains(&normalized)
    {
        return "NO_REPLY".to_string();
    }
    let title = system_prompt
        .split("职位：")
        .nth(1)
        .and_then(|value| value.split('；').next())
        .unwrap_or("当前职能");
    let role_question = if contains_any(
        title,
        &["后端", "服务端", "架构", "技术", "Backend", "Architect"],
    ) {
        "从技术方案角度，我需要确认：这项工作的核心约束是什么（性能、数据、合规、成本或系统集成）？最终验收指标由谁确认？"
    } else if contains_any(
        title,
        &["前端", "客户端", "UI", "UX", "设计", "Frontend", "Designer"],
    ) {
        "从用户体验和交付范围角度，我需要确认：目标用户是谁，优先覆盖哪些平台或场景？哪些体验指标代表这项工作完成？"
    } else if contains_any(title, &["测试", "质量", "QA", "Test"]) {
        "从质量保障角度，我需要确认：成功标准和不可接受的问题分别是什么？需要覆盖哪些边界场景，谁负责最终验收？"
    } else if contains_any(title, &["产品", "Product"]) {
        "从产品角度，我需要确认：这次工作的目标用户、优先级和成功指标是什么？如果范围冲突，哪个结果优先？"
    } else if contains_any(title, &["管理员", "招聘", "人才"]) {
        "从人员和组织角度，我需要确认：需要哪些角色参与、由谁负责决策？时间范围和人员预算边界是什么？"
    } else if contains_any(title, &["财务", "Finance", "会计", "采购"]) {
        "从预算与执行角度，我需要确认：预算上限、审批人和成本口径是什么？哪些支出可以先行，哪些必须事前审批？"
    } else if contains_any(title, &["法务", "合规", "Legal", "Compliance"]) {
        "从风险与合规角度，我需要确认：涉及哪些地区、数据或监管要求？需要保留哪些审计记录和审批证据？"
    } else if contains_any(
        title,
        &[
            "销售",
            "市场",
            "运营",
            "客服",
            "Sales",
            "Marketing",
            "Operations",
        ],
    ) {
        "从业务落地角度，我需要确认：服务对象和成功指标是什么？上线或执行后由谁承接反馈、响应和后续迭代？"
    } else if contains_any(title, &["数据", "算法", "Data", "AI", "研究", "Research"]) {
        "从数据与评估角度，我需要确认：数据来源、隐私边界和评估指标是什么？结果需要达到什么置信度才算可用？"
    } else if contains_any(title, &["安全", "Security"]) {
        "从安全角度，我需要确认：需要防护的资产和威胁边界是什么？是否有必须满足的权限、审计或应急响应要求？"
    } else {
        "从我的职能角度，我需要确认：这项工作的目标、优先级和完成标准是什么？有哪些不能突破的时间、预算或风险边界？"
    };
    apply_mock_voice(
        system_prompt,
        &format!("关于“{}”：{}", normalized, role_question),
    )
}

fn apply_mock_voice(system_prompt: &str, content: &str) -> String {
    let prefix = if system_prompt.contains("姓名：林屿") {
        "结论是，"
    } else if system_prompt.contains("姓名：苏晴") {
        "我确认一下："
    } else if system_prompt.contains("姓名：周启") {
        "先按目标、场景、验收三点来看："
    } else if system_prompt.contains("姓名：沈知夏") {
        "我会更倾向于从用户实际感受出发："
    } else if system_prompt.contains("姓名：陈默") {
        "可以，前提是边界明确。"
    } else if system_prompt.contains("姓名：顾言") {
        "这里有一个前提需要先钉住："
    } else if system_prompt.contains("姓名：唐宁") {
        "先反过来问一句：如果关键边界条件不成立呢？"
    } else {
        ""
    };
    format!("{prefix}{content}")
}

fn contains_any(value: &str, candidates: &[&str]) -> bool {
    candidates.iter().any(|candidate| value.contains(candidate))
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

    use super::apply_mock_voice;

    #[test]
    fn mock_employee_voices_are_distinct() {
        let backend = apply_mock_voice("姓名：顾言", "需要确认数据边界。");
        let designer = apply_mock_voice("姓名：沈知夏", "需要确认数据边界。");

        assert!(backend.starts_with("这里有一个前提"));
        assert!(designer.starts_with("我会更倾向于"));
        assert_ne!(backend, designer);
    }

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
