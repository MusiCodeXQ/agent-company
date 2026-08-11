use std::{collections::HashMap, io::Read, sync::Arc, time::Duration};

use adk_agent::LlmAgentBuilder;
use adk_core::{Agent, Content, Llm, Part, ToolContext};
use adk_devtools::{DevToolset, Workspace};
use adk_model::{OpenAIClient, OpenAIConfig};
use adk_runner::Runner;
use adk_session::{CreateRequest, SessionService, SqliteSessionService};
use adk_tool::FunctionTool;
use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const APP_NAME: &str = "ai-company-employee";
const SESSION_DATABASE_URL: &str = "sqlite:///agent-state/adk-sessions.db?mode=rwc";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    run_id: String,
    employee_id: String,
    session_id: String,
    system_prompt: String,
    content: String,
    model: ModelRequest,
    #[serde(default = "default_true")]
    allow_tools: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelRequest {
    protocol: String,
    base_url: String,
    api_key: Option<String>,
    model: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    content: String,
    usage: Usage,
    events: Vec<RuntimeEvent>,
    environment_changes: Vec<EnvironmentChange>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    input_tokens: i64,
    output_tokens: i64,
    cache_tokens: i64,
    total_tokens: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeEvent {
    event_type: String,
    tool_name: Option<String>,
    arguments: Option<Value>,
    status: Option<String>,
    result_summary: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentChange {
    ecosystem: String,
    package_name: String,
    requested_spec: Option<String>,
    source_command: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() != Some("run") {
        return Err(anyhow!("usage: employee-runtime run"));
    }
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .context("read runtime request")?;
    let request: RunRequest = serde_json::from_str(&input).context("decode runtime request")?;
    let response = run(request).await?;
    println!("{}", serde_json::to_string(&response)?);
    Ok(())
}

async fn run(request: RunRequest) -> Result<RunResponse> {
    if !matches!(
        request.model.protocol.to_ascii_lowercase().as_str(),
        "openai_compatible" | "openai-compatible" | "openai"
    ) {
        return Err(anyhow!("ADK runtime requires an OpenAI-compatible model"));
    }
    let model: Arc<dyn Llm> = Arc::new(OpenAIClient::new(OpenAIConfig::compatible(
        request.model.api_key.unwrap_or_default(),
        request.model.base_url,
        request.model.model,
    ))?);
    let instruction = format!(
        "{}\n\n你在一次性 Ubuntu 执行容器中工作，容器结束后会被删除。长期状态都挂载自宿主机员工目录：\
         项目文件放在 /workspace，Agent 状态放在 /agent-state，交付物放在 /artifacts，员工安装的依赖放在 /employee-env。\
         pip 已绑定到 /employee-env/python-venv，npm -g 已绑定到 /employee-env/npm，cargo install 已绑定到 /employee-env/cargo。\
         这些目录会自动持久化。不要调用 Docker，不要尝试保存容器。软件安装命令会被记录为后续统一基础镜像建议。\
         如果需要 apt 系统包或 root 安装，不要直接安装，调用 company_recommend_base_image_package 提交镜像建议。不要输出 API Key 或凭据。",
        request.system_prompt
    );
    let mut builder = LlmAgentBuilder::new(safe_identity(&request.employee_id))
        .instruction(instruction)
        .model(model)
        .max_iterations(20)
        .tool_timeout(Duration::from_secs(120))
        .circuit_breaker_threshold(3);
    if request.allow_tools {
        let workspace = Workspace::new("/workspace")
            .writable(true)
            .allow_bash(true)
            .bash_timeout(Duration::from_secs(120))
            .max_output_bytes(512 * 1024)
            .env_allowlist([
                "PATH",
                "HOME",
                "LANG",
                "LC_ALL",
                "TMPDIR",
                "TERM",
                "USER",
                "SHELL",
                "VIRTUAL_ENV",
                "NPM_CONFIG_PREFIX",
                "CARGO_INSTALL_ROOT",
            ]);
        builder = builder.toolset(Arc::new(DevToolset::new(workspace)));
    }
    let image_package_tool = FunctionTool::new(
        "company_recommend_base_image_package",
        "Recommend a system package for the next centrally built employee base image instead of using apt or root.",
        |_ctx: Arc<dyn ToolContext>, args: Value| async move {
            let package_name = args
                .get("packageName")
                .or_else(|| args.get("package_name"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim();
            if package_name.is_empty() {
                return Err(adk_core::AdkError::tool("packageName is required"));
            }
            Ok(serde_json::json!({
                "recommended": true,
                "packageName": package_name,
                "reason": args.get("reason").and_then(Value::as_str).unwrap_or_default()
            }))
        },
    );
    let agent: Arc<dyn Agent> = Arc::new(builder.tool(Arc::new(image_package_tool)).build()?);

    let sessions = Arc::new(SqliteSessionService::new(SESSION_DATABASE_URL).await?);
    sessions.migrate().await?;
    let _ = sessions
        .create(CreateRequest {
            app_name: APP_NAME.to_string(),
            user_id: request.employee_id.clone(),
            session_id: Some(request.session_id.clone()),
            state: Default::default(),
        })
        .await;
    let session_service: Arc<dyn SessionService> = sessions;
    let runner = Runner::builder()
        .app_name(APP_NAME)
        .agent(agent)
        .session_service(session_service)
        .build()?;
    let mut stream = runner
        .run_str(
            &request.employee_id,
            &request.session_id,
            Content::new("user").with_text(request.content),
        )
        .await?;

    let mut response = RunResponse::default();
    let mut pending_environment_changes: HashMap<String, Vec<EnvironmentChange>> = HashMap::new();
    while let Some(event) = stream.next().await {
        let event = event?;
        for call in event.tool_calls() {
            let changes = environment_changes(call.name, call.args);
            if !changes.is_empty() {
                pending_environment_changes
                    .insert(call.call_id.unwrap_or(call.name).to_string(), changes);
            }
            response.events.push(RuntimeEvent {
                event_type: "tool.called".to_string(),
                tool_name: Some(call.name.to_string()),
                arguments: Some(redact_arguments(call.args.clone())),
                status: Some("RUNNING".to_string()),
                result_summary: None,
            });
        }
        for result in event.tool_results() {
            if result.name == "company_recommend_base_image_package"
                || result.response.get("exit_code").and_then(Value::as_i64) == Some(0)
            {
                let key = result.call_id.unwrap_or(result.name);
                for change in pending_environment_changes.remove(key).unwrap_or_default() {
                    if !response.environment_changes.contains(&change) {
                        response.environment_changes.push(change);
                    }
                }
            }
            response.events.push(RuntimeEvent {
                event_type: "tool.completed".to_string(),
                tool_name: Some(result.name.to_string()),
                arguments: None,
                status: Some("COMPLETED".to_string()),
                result_summary: Some(compact_value(result.response)),
            });
        }
        if let Some(usage) = &event.llm_response.usage_metadata {
            response.usage.input_tokens += i64::from(usage.prompt_token_count);
            response.usage.output_tokens += i64::from(usage.candidates_token_count);
            response.usage.cache_tokens +=
                i64::from(usage.cache_read_input_token_count.unwrap_or_default());
            response.usage.total_tokens += i64::from(usage.total_token_count);
        }
        if event.is_final_response()
            && let Some(content) = &event.llm_response.content
        {
            response.content = content
                .parts
                .iter()
                .filter_map(|part| match part {
                    Part::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
        }
    }
    if response.content.trim().is_empty() {
        return Err(anyhow!("ADK runtime completed without a text response"));
    }
    response.events.push(RuntimeEvent {
        event_type: "run.completed".to_string(),
        tool_name: None,
        arguments: Some(serde_json::json!({"runId": request.run_id})),
        status: Some("COMPLETED".to_string()),
        result_summary: None,
    });
    Ok(response)
}

fn environment_changes(tool_name: &str, arguments: &Value) -> Vec<EnvironmentChange> {
    if tool_name == "company_recommend_base_image_package" {
        let package_name = arguments
            .get("packageName")
            .or_else(|| arguments.get("package_name"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        if package_name.is_empty() {
            return Vec::new();
        }
        return vec![EnvironmentChange {
            ecosystem: "system".to_string(),
            package_name: package_name.to_string(),
            requested_spec: None,
            source_command: "company_recommend_base_image_package".to_string(),
        }];
    }
    if tool_name != "bash" {
        return Vec::new();
    }
    let command = arguments
        .get("command")
        .or_else(|| arguments.get("cmd"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let lower = command.to_ascii_lowercase();
    let (ecosystem, marker) = if lower.contains("pip3 install ") {
        ("python", "pip3 install")
    } else if lower.contains("pip install ") {
        ("python", "pip install")
    } else if lower.contains("cargo install ") {
        ("rust", "cargo install")
    } else if lower.contains("npm install -g ") {
        ("node", "npm install -g")
    } else if lower.contains("npm i -g ") {
        ("node", "npm i -g")
    } else {
        return Vec::new();
    };
    let Some(start) = lower.find(marker) else {
        return Vec::new();
    };
    command[start + marker.len()..]
        .split_whitespace()
        .take_while(|token| !["&&", ";", "||"].contains(token))
        .filter(|token| !token.starts_with('-'))
        .take(16)
        .map(|requested| {
            let (package_name, requested_spec) = split_package_spec(ecosystem, requested);
            EnvironmentChange {
                ecosystem: ecosystem.to_string(),
                package_name,
                requested_spec,
                source_command: format!("{marker} {requested}"),
            }
        })
        .collect()
}

fn split_package_spec(ecosystem: &str, requested: &str) -> (String, Option<String>) {
    if ecosystem == "python" {
        for separator in ["==", ">=", "<=", "~=", ">", "<"] {
            if let Some(index) = requested.find(separator) {
                return (
                    requested[..index].to_string(),
                    Some(requested[index..].to_string()),
                );
            }
        }
    }
    (requested.to_string(), None)
}

fn redact_arguments(mut value: Value) -> Value {
    if let Some(object) = value.as_object_mut() {
        for (key, item) in object {
            if ["api_key", "apikey", "authorization", "token", "password"]
                .iter()
                .any(|secret| key.to_ascii_lowercase().contains(secret))
            {
                *item = Value::String("[REDACTED]".to_string());
            }
        }
    }
    value
}

fn compact_value(value: &Value) -> String {
    value.to_string().chars().take(800).collect()
}

fn safe_identity(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if value.is_empty() {
        "employee".to_string()
    } else {
        value
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{environment_changes, redact_arguments, safe_identity};

    #[test]
    fn extracts_environment_install_suggestions() {
        let changes =
            environment_changes("bash", &json!({"command": "pip install ruff==0.12 pytest"}));
        assert_eq!(changes.len(), 2);
        assert_eq!(changes[0].package_name, "ruff");
        assert_eq!(changes[0].requested_spec.as_deref(), Some("==0.12"));
        assert!(environment_changes("bash", &json!({"command": "cargo test"})).is_empty());
    }

    #[test]
    fn sanitizes_identity_and_secrets() {
        assert_eq!(safe_identity("后端 Employee 01"), "___employee_01");
        assert_eq!(
            redact_arguments(json!({"api_key": "secret"}))["api_key"],
            "[REDACTED]"
        );
    }
}
