use chrono::Utc;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};
use uuid::Uuid;

use adk_agent::LlmAgentBuilder;
use adk_core::{
    AdkError, Agent, CallbackContext, Content, ErrorCategory, ErrorComponent, Llm, LlmRequest,
    LlmResponse, Part, RunConfig, SessionId, Tool, ToolConfirmationDecision, ToolContext, UserId,
};
use adk_devtools::{DevToolset, Workspace};
use adk_model::{OpenAIClient, OpenAIConfig};
use adk_plugin::{
    AfterModelCallResult, AfterToolCallResult, BeforeModelCallResult, BeforeToolCallResult,
    EnhancedPlugin, PluginContext,
};
use adk_runner::Runner;
use adk_session::{CreateRequest, SessionService, SqliteSessionService};
use adk_tool::FunctionTool;
use anyhow::{Context, Result, anyhow};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const APP_NAME: &str = "ai-company-employee";
const SESSION_DATABASE_URL: &str = "sqlite:///agent-state/adk-sessions.db?mode=rwc";

#[derive(Debug, Default)]
struct GovernancePlugin;

#[derive(Clone, Debug, Default)]
struct GovernanceCounter {
    model_calls: u32,
    tool_calls: u32,
}

#[adk_core::async_trait]
impl EnhancedPlugin for GovernancePlugin {
    fn name(&self) -> &str {
        "ai-company-governance"
    }

    fn priority(&self) -> i32 {
        10
    }

    async fn before_tool_call(
        &self,
        tool: Arc<dyn Tool>,
        args: Value,
        _ctx: Arc<dyn CallbackContext>,
        plugin_ctx: &PluginContext,
    ) -> adk_core::Result<BeforeToolCallResult> {
        let mut counter = plugin_ctx
            .get::<GovernanceCounter>()
            .await
            .unwrap_or_default();
        counter.tool_calls = counter.tool_calls.saturating_add(1);
        plugin_ctx.insert(counter.clone()).await;
        if counter.tool_calls > 20 {
            return Err(AdkError::new(
                ErrorComponent::Tool,
                ErrorCategory::RateLimited,
                "tool_budget_exceeded",
                "governance tool-call budget exceeded",
            ));
        }
        if tool.name() == "bash" {
            let command = args
                .get("command")
                .or_else(|| args.get("cmd"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let blocked = ["sudo ", "apt ", "apt-get", "docker ", "mount ", "rm -rf /"];
            if blocked.iter().any(|marker| command.contains(marker)) {
                return Err(AdkError::new(
                    ErrorComponent::Tool,
                    ErrorCategory::Forbidden,
                    "governance_blocked",
                    "governance policy blocked a host/root/network escape command",
                ));
            }
        }
        Ok(BeforeToolCallResult::Continue(args))
    }

    async fn after_tool_call(
        &self,
        _tool: Arc<dyn Tool>,
        _args: &Value,
        result: Value,
        _ctx: Arc<dyn CallbackContext>,
        _plugin_ctx: &PluginContext,
    ) -> adk_core::Result<AfterToolCallResult> {
        Ok(AfterToolCallResult::Continue(redact_arguments(result)))
    }

    async fn before_model_call(
        &self,
        request: LlmRequest,
        _ctx: Arc<dyn CallbackContext>,
        plugin_ctx: &PluginContext,
    ) -> adk_core::Result<BeforeModelCallResult> {
        let mut counter = plugin_ctx
            .get::<GovernanceCounter>()
            .await
            .unwrap_or_default();
        counter.model_calls = counter.model_calls.saturating_add(1);
        plugin_ctx.insert(counter.clone()).await;
        if counter.model_calls > 30 {
            return Err(AdkError::new(
                ErrorComponent::Model,
                ErrorCategory::RateLimited,
                "model_budget_exceeded",
                "governance model-call budget exceeded",
            ));
        }
        Ok(BeforeModelCallResult::Continue(request))
    }

    async fn after_model_call(
        &self,
        response: LlmResponse,
        _ctx: Arc<dyn CallbackContext>,
        _plugin_ctx: &PluginContext,
    ) -> adk_core::Result<AfterModelCallResult> {
        Ok(AfterModelCallResult::Continue(response))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest {
    company_id: String,
    run_id: String,
    agent_job_id: String,
    employee_id: String,
    session_id: String,
    system_prompt: String,
    content: String,
    model: ModelRequest,
    #[serde(default = "default_true")]
    allow_tools: bool,
    #[serde(default)]
    tool_approval: Option<ToolApprovalRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolApprovalRequest {
    call_id: String,
    decision: String,
    fingerprint: String,
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
    #[serde(default = "default_run_status")]
    status: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Usage {
    input_tokens: i64,
    output_tokens: i64,
    cache_tokens: i64,
    total_tokens: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeEvent {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(rename = "eventId")]
    event_id: String,
    company_id: String,
    employee_id: String,
    #[serde(rename = "agentJobId")]
    agent_job_id: String,
    #[serde(rename = "runtimeRunId")]
    runtime_run_id: String,
    #[serde(rename = "runtimeSessionId")]
    runtime_session_id: String,
    timestamp: String,
    sequence: u64,
    event_type: String,
    tool_name: Option<String>,
    call_id: Option<String>,
    arguments: Option<Value>,
    status: Option<String>,
    result_summary: Option<String>,
    fingerprint: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum WireMessage<'a> {
    Event { event: &'a RuntimeEvent },
    Result { response: &'a RunResponse },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentChange {
    ecosystem: String,
    package_name: String,
    requested_spec: Option<String>,
    source_command: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersonalToolchainRecord {
    name: String,
    version: String,
    install_path: String,
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
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());
    let mut emit = |event: &RuntimeEvent| -> Result<()> {
        serde_json::to_writer(&mut stdout, &WireMessage::Event { event })?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(())
    };
    let response = run(request, &mut emit).await?;
    serde_json::to_writer(
        &mut stdout,
        &WireMessage::Result {
            response: &response,
        },
    )?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

async fn run(
    request: RunRequest,
    emit: &mut dyn FnMut(&RuntimeEvent) -> Result<()>,
) -> Result<RunResponse> {
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
        "{}\n\n你在一次性 Debian stable-slim 执行容器中工作，容器结束后会被删除。基础镜像不提供 Python、Node.js、Rust、JDK、Gradle 或 Maven。\
         项目文件放在 /workspace，Agent 状态放在 /agent-state，交付物放在 /artifacts。$HOME=/home/employee 是宿主机上的个人员工目录，不属于临时容器层。\
         工作前阅读 $HOME/.ai-company/RUNTIME_POLICY.md 和 toolchains.json。所有个人工具链必须安装到 $HOME/.local/opt 或 $HOME/.local/share，可执行文件放到 $HOME/.local/bin；Rust 使用 $HOME/.cargo 与 $HOME/.rustup，Gradle/Maven 缓存分别使用 $HOME/.gradle 与 $HOME/.m2。\
         Python、Node.js、Rust、JDK、Gradle、Maven 以及未来工具都由你在个人目录中安装、升级、复用和清理。安装或升级后必须调用 company_record_personal_toolchain 登记名称、版本和安装路径。\
         不要调用 Docker，不要使用 sudo/apt，不要写容器根文件系统。如果确实缺少系统库，调用 company_recommend_base_image_package 提交基础镜像建议。不要输出 API Key 或凭据。",
        request.system_prompt
    );
    let mut builder = LlmAgentBuilder::new(safe_identity(&request.employee_id))
        .instruction(instruction)
        .model(model)
        .max_iterations(20)
        .tool_timeout(Duration::from_secs(120))
        .circuit_breaker_threshold(3)
        .enhanced_plugin(Arc::new(GovernancePlugin));
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
                "XDG_CONFIG_HOME",
                "XDG_CACHE_HOME",
                "XDG_DATA_HOME",
                "NPM_CONFIG_PREFIX",
                "CARGO_HOME",
                "RUSTUP_HOME",
                "GRADLE_USER_HOME",
            ]);
        builder = builder
            .toolset(Arc::new(DevToolset::new(workspace)))
            .require_tool_confirmation("bash");
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
    let personal_toolchain_tool = FunctionTool::new(
        "company_record_personal_toolchain",
        "Record a language or build toolchain installed under the persistent employee home directory.",
        |_ctx: Arc<dyn ToolContext>, args: Value| async move { record_personal_toolchain(&args).await },
    );
    if request.allow_tools {
        builder = builder
            .tool(Arc::new(image_package_tool))
            .tool(Arc::new(personal_toolchain_tool));
    }
    let agent: Arc<dyn Agent> = Arc::new(builder.build()?);

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
    let run_config = request.tool_approval.as_ref().and_then(|approval| {
        let decision = match approval.decision.to_ascii_lowercase().as_str() {
            "approve" | "approved" => ToolConfirmationDecision::Approve,
            "deny" | "denied" => ToolConfirmationDecision::Deny,
            _ => return None,
        };
        let mut decisions = HashMap::new();
        decisions.insert(approval.call_id.clone(), decision);
        let mut fingerprints = HashMap::new();
        fingerprints.insert(approval.call_id.clone(), approval.fingerprint.clone());
        Some(
            RunConfig::builder()
                .tool_confirmation_decisions(decisions)
                .tool_confirmation_fingerprints(fingerprints)
                .build(),
        )
    });
    let mut stream = runner
        .run_with_config(
            UserId::try_from(request.employee_id.as_str())?,
            SessionId::try_from(request.session_id.as_str())?,
            Content::new("user").with_text(request.content),
            run_config,
        )
        .await?;

    let mut response = RunResponse {
        status: default_run_status(),
        ..RunResponse::default()
    };
    let mut sequence = 0_u64;
    let mut waiting_for_approval = false;
    let mut pending_environment_changes: HashMap<String, Vec<EnvironmentChange>> = HashMap::new();
    let started_event = RuntimeEvent {
        schema_version: 1,
        event_id: format!("runtime-event-{}", Uuid::new_v4()),
        company_id: request.company_id.clone(),
        employee_id: request.employee_id.clone(),
        agent_job_id: request.agent_job_id.clone(),
        runtime_run_id: request.run_id.clone(),
        runtime_session_id: request.session_id.clone(),
        timestamp: Utc::now().to_rfc3339(),
        sequence: next_sequence(&mut sequence),
        event_type: "run.started".to_string(),
        tool_name: None,
        call_id: None,
        arguments: Some(serde_json::json!({ "runId": request.run_id })),
        status: Some("RUNNING".to_string()),
        result_summary: None,
        fingerprint: None,
    };
    emit(&started_event)?;
    response.events.push(started_event);
    let model_started_event = RuntimeEvent {
        schema_version: 1,
        event_id: format!("runtime-event-{}", Uuid::new_v4()),
        company_id: request.company_id.clone(),
        employee_id: request.employee_id.clone(),
        agent_job_id: request.agent_job_id.clone(),
        runtime_run_id: request.run_id.clone(),
        runtime_session_id: request.session_id.clone(),
        timestamp: Utc::now().to_rfc3339(),
        sequence: next_sequence(&mut sequence),
        event_type: "model.started".to_string(),
        tool_name: None,
        call_id: None,
        arguments: None,
        status: Some("RUNNING".to_string()),
        result_summary: None,
        fingerprint: None,
    };
    emit(&model_started_event)?;
    response.events.push(model_started_event);
    while let Some(event) = stream.next().await {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                let failed_event = RuntimeEvent {
                    schema_version: 1,
                    event_id: format!("runtime-event-{}", Uuid::new_v4()),
                    company_id: request.company_id.clone(),
                    employee_id: request.employee_id.clone(),
                    agent_job_id: request.agent_job_id.clone(),
                    runtime_run_id: request.run_id.clone(),
                    runtime_session_id: request.session_id.clone(),
                    timestamp: Utc::now().to_rfc3339(),
                    sequence: next_sequence(&mut sequence),
                    event_type: "run.failed".to_string(),
                    tool_name: None,
                    call_id: None,
                    arguments: Some(serde_json::json!({ "error": error.to_string() })),
                    status: Some("FAILED".to_string()),
                    result_summary: Some(error.to_string()),
                    fingerprint: None,
                };
                emit(&failed_event)?;
                response.events.push(failed_event);
                return Err(error.into());
            }
        };
        if let Some(confirmation) = event.actions.tool_confirmation.as_ref() {
            let args = redact_arguments(confirmation.args.clone());
            let call_id = confirmation.function_call_id.clone();
            let fingerprint = Some(adk_core::tool_call_fingerprint(
                &confirmation.tool_name,
                &confirmation.args,
            ));
            let runtime_event = RuntimeEvent {
                schema_version: 1,
                event_id: format!("runtime-event-{}", Uuid::new_v4()),
                company_id: request.company_id.clone(),
                employee_id: request.employee_id.clone(),
                agent_job_id: request.agent_job_id.clone(),
                runtime_run_id: request.run_id.clone(),
                runtime_session_id: request.session_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                sequence: next_sequence(&mut sequence),
                event_type: "approval.required".to_string(),
                tool_name: Some(confirmation.tool_name.clone()),
                call_id,
                arguments: Some(args),
                status: Some("WAITING_APPROVAL".to_string()),
                result_summary: None,
                fingerprint,
            };
            emit(&runtime_event)?;
            response.events.push(runtime_event);
            waiting_for_approval = true;
        }
        for call in event.tool_calls() {
            let changes = environment_changes(call.name, call.args);
            if !changes.is_empty() {
                pending_environment_changes
                    .insert(call.call_id.unwrap_or(call.name).to_string(), changes);
            }
            let runtime_event = RuntimeEvent {
                schema_version: 1,
                event_id: format!("runtime-event-{}", Uuid::new_v4()),
                company_id: request.company_id.clone(),
                employee_id: request.employee_id.clone(),
                agent_job_id: request.agent_job_id.clone(),
                runtime_run_id: request.run_id.clone(),
                runtime_session_id: request.session_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                sequence: next_sequence(&mut sequence),
                event_type: "tool.requested".to_string(),
                tool_name: Some(call.name.to_string()),
                call_id: call.call_id.map(str::to_string),
                arguments: Some(redact_arguments(call.args.clone())),
                status: Some("RUNNING".to_string()),
                result_summary: None,
                fingerprint: Some(adk_core::tool_call_fingerprint(call.name, call.args)),
            };
            emit(&runtime_event)?;
            response.events.push(runtime_event.clone());
            let mut started_event = runtime_event;
            started_event.event_id = format!("runtime-event-{}", Uuid::new_v4());
            started_event.sequence = next_sequence(&mut sequence);
            started_event.event_type = "tool.started".to_string();
            emit(&started_event)?;
            response.events.push(started_event);
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
            let runtime_event = RuntimeEvent {
                schema_version: 1,
                event_id: format!("runtime-event-{}", Uuid::new_v4()),
                company_id: request.company_id.clone(),
                employee_id: request.employee_id.clone(),
                agent_job_id: request.agent_job_id.clone(),
                runtime_run_id: request.run_id.clone(),
                runtime_session_id: request.session_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                sequence: next_sequence(&mut sequence),
                event_type: "tool.completed".to_string(),
                tool_name: Some(result.name.to_string()),
                call_id: result.call_id.map(str::to_string),
                arguments: None,
                status: Some("COMPLETED".to_string()),
                result_summary: Some(compact_value(result.response)),
                fingerprint: None,
            };
            emit(&runtime_event)?;
            response.events.push(runtime_event);
        }
        if let Some(usage) = &event.llm_response.usage_metadata {
            response.usage.input_tokens += i64::from(usage.prompt_token_count);
            response.usage.output_tokens += i64::from(usage.candidates_token_count);
            response.usage.cache_tokens +=
                i64::from(usage.cache_read_input_token_count.unwrap_or_default());
            response.usage.total_tokens += i64::from(usage.total_token_count);
            let usage_event = RuntimeEvent {
                schema_version: 1,
                event_id: format!("runtime-event-{}", Uuid::new_v4()),
                company_id: request.company_id.clone(),
                employee_id: request.employee_id.clone(),
                agent_job_id: request.agent_job_id.clone(),
                runtime_run_id: request.run_id.clone(),
                runtime_session_id: request.session_id.clone(),
                timestamp: Utc::now().to_rfc3339(),
                sequence: next_sequence(&mut sequence),
                event_type: "usage.updated".to_string(),
                tool_name: None,
                call_id: None,
                arguments: Some(serde_json::json!({ "usage": &response.usage })),
                status: Some("RUNNING".to_string()),
                result_summary: None,
                fingerprint: None,
            };
            emit(&usage_event)?;
            response.events.push(usage_event);
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
    if waiting_for_approval {
        response.status = "WAITING_APPROVAL".to_string();
        let runtime_event = RuntimeEvent {
            schema_version: 1,
            event_id: format!("runtime-event-{}", Uuid::new_v4()),
            company_id: request.company_id.clone(),
            employee_id: request.employee_id.clone(),
            agent_job_id: request.agent_job_id.clone(),
            runtime_run_id: request.run_id.clone(),
            runtime_session_id: request.session_id.clone(),
            timestamp: Utc::now().to_rfc3339(),
            sequence: next_sequence(&mut sequence),
            event_type: "run.waiting_approval".to_string(),
            tool_name: None,
            call_id: None,
            arguments: Some(serde_json::json!({ "runId": request.run_id })),
            status: Some("WAITING_APPROVAL".to_string()),
            result_summary: None,
            fingerprint: None,
        };
        emit(&runtime_event)?;
        response.events.push(runtime_event);
        return Ok(response);
    }
    if response.content.trim().is_empty() {
        return Err(anyhow!("ADK runtime completed without a text response"));
    }
    let runtime_event = RuntimeEvent {
        schema_version: 1,
        event_id: format!("runtime-event-{}", Uuid::new_v4()),
        company_id: request.company_id.clone(),
        employee_id: request.employee_id.clone(),
        agent_job_id: request.agent_job_id.clone(),
        runtime_run_id: request.run_id.clone(),
        runtime_session_id: request.session_id.clone(),
        timestamp: Utc::now().to_rfc3339(),
        sequence: next_sequence(&mut sequence),
        event_type: "run.completed".to_string(),
        tool_name: None,
        call_id: None,
        arguments: Some(serde_json::json!({"runId": request.run_id})),
        status: Some("COMPLETED".to_string()),
        result_summary: None,
        fingerprint: None,
    };
    emit(&runtime_event)?;
    response.events.push(runtime_event);
    Ok(response)
}

fn next_sequence(sequence: &mut u64) -> u64 {
    *sequence = sequence.saturating_add(1);
    *sequence
}

async fn record_personal_toolchain(args: &Value) -> Result<Value, adk_core::AdkError> {
    record_personal_toolchain_at(args, Path::new("/home/employee")).await
}

async fn record_personal_toolchain_at(
    args: &Value,
    employee_home: &Path,
) -> Result<Value, adk_core::AdkError> {
    let field = |camel: &str, snake: &str| {
        args.get(camel)
            .or_else(|| args.get(snake))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
    };
    let name = field("name", "name");
    let version = field("version", "version");
    let install_path = field("installPath", "install_path");
    if name.is_empty() || version.is_empty() || install_path.is_empty() {
        return Err(adk_core::AdkError::tool(
            "name, version and installPath are required",
        ));
    }
    if name.len() > 64
        || !name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".-_".contains(character))
        || version.len() > 128
        || version.contains(['\n', '\r'])
    {
        return Err(adk_core::AdkError::tool(
            "toolchain name or version is not safe",
        ));
    }
    let installed = tokio::fs::canonicalize(install_path)
        .await
        .map_err(|error| {
            adk_core::AdkError::tool(format!("installPath does not exist: {error}"))
        })?;
    let home = tokio::fs::canonicalize(employee_home)
        .await
        .map_err(|error| {
            adk_core::AdkError::tool(format!("employee home is unavailable: {error}"))
        })?;
    if !installed.starts_with(&home) {
        return Err(adk_core::AdkError::tool(
            "installPath must be inside the persistent employee home",
        ));
    }
    let manifest = home.join(".ai-company/toolchains.json");
    let mut records = match tokio::fs::read(&manifest).await {
        Ok(bytes) => {
            serde_json::from_slice::<Vec<PersonalToolchainRecord>>(&bytes).map_err(|error| {
                adk_core::AdkError::tool(format!("invalid toolchain manifest: {error}"))
            })?
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            return Err(adk_core::AdkError::tool(format!(
                "cannot read toolchain manifest: {error}"
            )));
        }
    };
    let record = PersonalToolchainRecord {
        name: name.to_string(),
        version: version.to_string(),
        install_path: installed.to_string_lossy().into_owned(),
    };
    if let Some(existing) = records.iter_mut().find(|item| item.name == record.name) {
        *existing = record.clone();
    } else {
        records.push(record.clone());
    }
    let temporary = manifest.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(&records)
        .map_err(|error| adk_core::AdkError::tool(format!("encode toolchain manifest: {error}")))?;
    tokio::fs::write(&temporary, bytes)
        .await
        .map_err(|error| adk_core::AdkError::tool(format!("write toolchain manifest: {error}")))?;
    tokio::fs::rename(&temporary, &manifest)
        .await
        .map_err(|error| {
            adk_core::AdkError::tool(format!("replace toolchain manifest: {error}"))
        })?;
    Ok(serde_json::json!({"recorded": true, "toolchain": record}))
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

fn default_run_status() -> String {
    "COMPLETED".to_string()
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::{
        PersonalToolchainRecord, environment_changes, record_personal_toolchain_at,
        redact_arguments, safe_identity,
    };

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

    #[tokio::test]
    async fn records_personal_toolchains_inside_employee_home() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "ai-company-toolchain-test-{}-{nonce}",
            std::process::id()
        ));
        let installation = home.join(".local/opt/node-22");
        tokio::fs::create_dir_all(home.join(".ai-company"))
            .await
            .expect("manifest directory should be created");
        tokio::fs::create_dir_all(&installation)
            .await
            .expect("toolchain directory should be created");
        record_personal_toolchain_at(
            &json!({
                "name": "node",
                "version": "22.18.0",
                "installPath": installation
            }),
            &home,
        )
        .await
        .expect("personal toolchain should be recorded");
        let records: Vec<PersonalToolchainRecord> = serde_json::from_slice(
            &tokio::fs::read(home.join(".ai-company/toolchains.json"))
                .await
                .expect("manifest should exist"),
        )
        .expect("manifest should be valid JSON");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "node");
        tokio::fs::remove_dir_all(home)
            .await
            .expect("test directory should be removable");
    }
}
