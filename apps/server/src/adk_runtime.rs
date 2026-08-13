use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::{
    app_state::AppState,
    container_engine::RunContainerRequest,
    container_runtime,
    db::{self, AgentJob},
    events,
    model_profiles::ModelProfileCredentials,
    models::{Employee, TokenUsageSummary},
    runtime_store,
};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunRequest<'a> {
    company_id: &'a str,
    run_id: &'a str,
    agent_job_id: &'a str,
    employee_id: &'a str,
    session_id: &'a str,
    system_prompt: &'a str,
    content: &'a str,
    model: ModelRequest<'a>,
    allow_tools: bool,
    /// Employee runtime runs are started by the restricted Docker engine. Keep
    /// this explicit in the wire protocol so a non-container fallback remains
    /// conservative and continues to require bash confirmation.
    sandboxed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_approval: Option<ToolApprovalRequest<'a>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolApprovalRequest<'a> {
    call_id: &'a str,
    decision: &'a str,
    fingerprint: &'a str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelRequest<'a> {
    protocol: &'a str,
    base_url: &'a str,
    api_key: Option<&'a str>,
    model: &'a str,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunResponse {
    content: String,
    usage: TokenUsageSummary,
    #[serde(default)]
    events: Vec<RuntimeEvent>,
    #[serde(default)]
    environment_changes: Vec<EnvironmentChange>,
    #[serde(default = "default_run_status")]
    status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentChange {
    ecosystem: String,
    package_name: String,
    requested_spec: Option<String>,
    source_command: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeEvent {
    #[serde(default)]
    schema_version: u32,
    #[serde(default)]
    event_id: String,
    #[serde(default)]
    company_id: String,
    #[serde(default)]
    employee_id: String,
    #[serde(default)]
    agent_job_id: String,
    #[serde(default)]
    runtime_run_id: String,
    #[serde(default)]
    runtime_session_id: String,
    #[serde(default)]
    timestamp: String,
    #[serde(default)]
    sequence: u64,
    event_type: String,
    tool_name: Option<String>,
    #[serde(default)]
    call_id: Option<String>,
    arguments: Option<Value>,
    status: Option<String>,
    result_summary: Option<String>,
    #[serde(default)]
    fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum RuntimeWireMessage {
    Event { event: Box<RuntimeEvent> },
    Result { response: RunResponse },
}

pub async fn run_turn(
    state: &AppState,
    job: &AgentJob,
    employee: &Employee,
    profile_id: &str,
    profile: &ModelProfileCredentials,
    system_prompt: &str,
) -> Result<String> {
    if !profile.enabled {
        return Err(anyhow!("the employee model profile is disabled"));
    }
    let workspace =
        container_runtime::ensure_workspace(state, &job.company_id, &job.employee_id).await?;
    let session_id = format!("employee-{}-main", employee.id);
    db::upsert_runtime_session_binding(
        &state.pool,
        &job.company_id,
        &job.employee_id,
        "USER_CHAT",
        "user",
        &session_id,
    )
    .await?;
    let run_id = runtime_store::create_run(
        &state.pool,
        &job.company_id,
        &job.employee_id,
        &job.id,
        &session_id,
    )
    .await?;
    let content = super::runtime::job_message_content(job);
    let approval_payload = serde_json::from_str::<Value>(&job.payload).ok();
    let approval_call_id = approval_payload
        .as_ref()
        .and_then(|value| value.get("toolApproval"))
        .and_then(|value| value.get("callId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let approval_decision = approval_payload
        .as_ref()
        .and_then(|value| value.get("toolApproval"))
        .and_then(|value| value.get("decision"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let approval_fingerprint = approval_payload
        .as_ref()
        .and_then(|value| value.get("toolApproval"))
        .and_then(|value| value.get("fingerprint"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let tool_approval = match (
        approval_call_id.as_deref(),
        approval_decision.as_deref(),
        approval_fingerprint.as_deref(),
    ) {
        (Some(call_id), Some(decision), Some(fingerprint)) => Some(ToolApprovalRequest {
            call_id,
            decision,
            fingerprint,
        }),
        _ => None,
    };
    let request = RunRequest {
        company_id: &job.company_id,
        run_id: &run_id,
        agent_job_id: &job.id,
        employee_id: &job.employee_id,
        session_id: &session_id,
        system_prompt,
        content: &content,
        model: ModelRequest {
            protocol: &profile.protocol,
            base_url: &profile.base_url,
            api_key: profile.api_key.as_deref(),
            model: &profile.model,
        },
        // A2A responses are deliberately text-only. The face-to-face
        // capability authorizes protocol messages, never arbitrary container
        // tools; only ordinary workstation jobs receive DevToolset.
        allow_tools: job.meeting_id.is_none() && job.job_type != "A2A_RESPONSE",
        sandboxed: state.container_engine.config().enabled,
        tool_approval,
    };
    let input = serde_json::to_vec(&request)?;
    let started = Instant::now();
    let container_request = RunContainerRequest {
        company_id: job.company_id.clone(),
        employee_id: job.employee_id.clone(),
        run_id: run_id.clone(),
        workspace_path: workspace.workspace_path.clone().into(),
        state_path: workspace.state_path.clone().into(),
        artifacts_path: workspace.artifacts_path.clone().into(),
        environment_path: workspace.environment_path.clone().into(),
    };
    let (event_tx, mut event_rx) = mpsc::channel(128);
    let mut execution = Box::pin(state.container_engine.exec_employee_runtime(
        &container_request,
        &input,
        event_tx,
    ));
    let mut output_result = None;
    let mut output = None;
    let mut streamed_events = false;
    let mut streamed_response = None;
    while output.is_none() {
        tokio::select! {
            line = event_rx.recv() => {
                if let Some(line) = line {
                    match serde_json::from_slice::<RuntimeWireMessage>(&line) {
                        Ok(RuntimeWireMessage::Event { event }) => {
                            streamed_events = true;
                            persist_runtime_event(state, job, employee, &run_id, &event).await?;
                        }
                        Ok(RuntimeWireMessage::Result { response }) => {
                            streamed_response = Some(response);
                        }
                        Err(error) => {
                            if let Ok(raw) = serde_json::from_slice::<Value>(&line) {
                                persist_unknown_runtime_line(
                                    state, job, employee, &run_id, &raw,
                                )
                                .await?;
                            }
                            tracing::debug!(%error, run_id = %run_id, "ignored non-NDJSON employee runtime line");
                        }
                    }
                }
            }
            result = &mut execution, if output_result.is_none() => {
                output_result = Some(result);
            }
        }
        if output_result.is_some() && event_rx.is_closed() && event_rx.is_empty() {
            output = Some(
                output_result
                    .take()
                    .expect("runtime output result is present"),
            );
        }
    }
    let output = match output.expect("employee runtime execution completed") {
        Ok(output) => output,
        Err(error) => {
            let usage = TokenUsageSummary::default();
            runtime_store::finish_run(
                &state.pool,
                &run_id,
                "FAILED",
                &usage,
                Some(&error.to_string()),
            )
            .await?;
            record_llm_call(
                state,
                job,
                profile_id,
                profile,
                "FAILED",
                &usage,
                started,
                Some(&error.to_string()),
            )
            .await;
            return Err(error);
        }
    };
    let response = if let Some(response) = streamed_response {
        response
    } else {
        match parse_runtime_response(&output).context("decode response from employee ADK runtime") {
            Ok(response) => response,
            Err(error) => {
                let usage = TokenUsageSummary::default();
                runtime_store::finish_run(
                    &state.pool,
                    &run_id,
                    "FAILED",
                    &usage,
                    Some(&error.to_string()),
                )
                .await?;
                record_llm_call(
                    state,
                    job,
                    profile_id,
                    profile,
                    "FAILED",
                    &usage,
                    started,
                    Some(&error.to_string()),
                )
                .await;
                return Err(error);
            }
        }
    };
    if !streamed_events {
        for event in &response.events {
            persist_runtime_event(state, job, employee, &run_id, event).await?;
        }
    }
    /*
     * The old runtime emitted one JSON object only after the process exited.
     * Keep accepting that format while all current runtimes use the live NDJSON
     * channel above. This makes rolling upgrades safe for existing containers.
     */
    /*
     * Errors are handled below after the streamed events have already been
     * persisted, so a UI can see the exact point at which an invocation failed.
     */
    let response_status = response.status.clone();
    if response_status == "WAITING_APPROVAL" {
        runtime_store::finish_run(
            &state.pool,
            &run_id,
            "WAITING_APPROVAL",
            &response.usage,
            None,
        )
        .await?;
        record_llm_call(
            state,
            job,
            profile_id,
            profile,
            "WAITING_APPROVAL",
            &response.usage,
            started,
            None,
        )
        .await;
        let approval_id = response
            .events
            .iter()
            .find(|event| event.event_type == "approval.required")
            .and_then(|event| event.call_id.as_deref())
            .unwrap_or("pending");
        return Ok(format!("__AI_COMPANY_WAITING_APPROVAL__:{approval_id}"));
    }
    for change in &response.environment_changes {
        runtime_store::recommend_environment_change(
            &state.pool,
            runtime_store::EnvironmentSuggestionInput {
                company_id: &job.company_id,
                employee_id: &job.employee_id,
                runtime_run_id: &run_id,
                ecosystem: &change.ecosystem,
                package_name: &change.package_name,
                requested_spec: change.requested_spec.as_deref(),
                source_command: &change.source_command,
            },
        )
        .await?;
    }
    if !response.environment_changes.is_empty() {
        if let Err(error) =
            write_environment_manifest(&workspace.environment_path, &response.environment_changes)
                .await
        {
            tracing::warn!(%error, employee_id = %job.employee_id, "cannot update employee software suggestion manifest");
        }
    }
    runtime_store::finish_run(&state.pool, &run_id, "COMPLETED", &response.usage, None).await?;
    record_llm_call(
        state,
        job,
        profile_id,
        profile,
        "COMPLETED",
        &response.usage,
        started,
        None,
    )
    .await;

    if !response.environment_changes.is_empty() {
        events::emit(
            state,
            &job.company_id,
            "runtime.environment.suggested",
            Some(&job.employee_id),
            Some(&run_id),
            json!({
                "employeeId": job.employee_id,
                "changes": response.environment_changes.iter().map(|change| json!({
                    "ecosystem": change.ecosystem,
                    "packageName": change.package_name,
                    "requestedSpec": change.requested_spec
                })).collect::<Vec<_>>()
            }),
        )
        .await?;
    }
    Ok(response.content)
}

async fn persist_runtime_event(
    state: &AppState,
    job: &AgentJob,
    employee: &Employee,
    run_id: &str,
    event: &RuntimeEvent,
) -> Result<()> {
    if matches!(
        event.event_type.as_str(),
        "tool.called" | "tool.requested" | "tool.started" | "tool.completed" | "approval.required"
    ) {
        if let Some(tool_name) = event.tool_name.as_deref() {
            runtime_store::record_tool_call(
                &state.pool,
                run_id,
                &job.employee_id,
                tool_name,
                event.call_id.as_deref(),
                &event
                    .arguments
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "{}".to_string())
                    .chars()
                    .take(2_000)
                    .collect::<String>(),
                event.status.as_deref().unwrap_or("RUNNING"),
                event.result_summary.as_deref(),
            )
            .await?;
        }
    }
    let approval_id = if event.event_type == "approval.required" {
        let call_id = event
            .call_id
            .as_deref()
            .or(event.tool_name.as_deref())
            .unwrap_or("unknown-call");
        let fingerprint = event.fingerprint.as_deref().unwrap_or("unavailable");
        Some(
            runtime_store::create_tool_approval(
                &state.pool,
                &job.company_id,
                run_id,
                &job.id,
                &job.employee_id,
                call_id,
                event.tool_name.as_deref().unwrap_or("unknown-tool"),
                &event
                    .arguments
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "{}".to_string())
                    .chars()
                    .take(2_000)
                    .collect::<String>(),
                fingerprint,
            )
            .await?,
        )
    } else {
        None
    };
    let payload = serde_json::to_string(event)?;
    let event_key = if event.event_id.is_empty() {
        (event.sequence > 0).then(|| format!("{}:{}", event.sequence, event.event_type))
    } else {
        Some(event.event_id.clone())
    };
    db::record_runtime_event_with_key(
        &state.pool,
        &job.company_id,
        Some(&job.employee_id),
        Some(&employee.id),
        Some(run_id),
        &event.event_type,
        &payload,
        i64::try_from(event.sequence).unwrap_or(i64::MAX),
        event_key.as_deref(),
    )
    .await?;
    events::emit(
        state,
        &job.company_id,
        "runtime.event",
        Some(&job.employee_id),
        Some(run_id),
        json!({
            "schemaVersion": event.schema_version,
            "eventId": event.event_id,
            "runId": run_id,
            "jobId": job.id,
            "sequence": event.sequence,
            "eventType": event.event_type,
            "toolName": event.tool_name,
            "callId": event.call_id,
            "status": event.status,
            "arguments": event.arguments,
            "resultSummary": event.result_summary,
            "fingerprint": event.fingerprint,
            "approvalId": approval_id,
        }),
    )
    .await?;
    Ok(())
}

async fn persist_unknown_runtime_line(
    state: &AppState,
    job: &AgentJob,
    employee: &Employee,
    run_id: &str,
    raw: &Value,
) -> Result<()> {
    let event_id = raw.get("eventId").and_then(Value::as_str);
    let sequence = raw
        .get("sequence")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let event_key = event_id
        .map(str::to_string)
        .or_else(|| (sequence > 0).then(|| format!("unknown:{sequence}")));
    db::record_runtime_event_with_key(
        &state.pool,
        &job.company_id,
        Some(&job.employee_id),
        Some(&employee.id),
        Some(run_id),
        "runtime.unknown",
        &raw.to_string(),
        sequence,
        event_key.as_deref(),
    )
    .await?;
    Ok(())
}

fn parse_runtime_response(output: &[u8]) -> Result<RunResponse> {
    for line in output.rsplit(|byte| *byte == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Ok(RuntimeWireMessage::Result { response }) =
            serde_json::from_slice::<RuntimeWireMessage>(line)
        {
            return Ok(response);
        }
        if let Ok(response) = serde_json::from_slice::<RunResponse>(line) {
            return Ok(response);
        }
    }
    Err(anyhow!("employee runtime did not return a result message"))
}

async fn write_environment_manifest(
    environment_path: &str,
    changes: &[EnvironmentChange],
) -> Result<()> {
    let manifest_path = std::path::Path::new(environment_path).join("software-suggestions.json");
    let mut manifest = match tokio::fs::read(&manifest_path).await {
        Ok(bytes) => serde_json::from_slice::<Vec<EnvironmentChange>>(&bytes).unwrap_or_default(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    for change in changes {
        if let Some(existing) = manifest.iter_mut().find(|item| {
            item.ecosystem == change.ecosystem && item.package_name == change.package_name
        }) {
            *existing = change.clone();
        } else {
            manifest.push(change.clone());
        }
    }
    let temporary_path = manifest_path.with_extension("json.tmp");
    tokio::fs::write(&temporary_path, serde_json::to_vec_pretty(&manifest)?).await?;
    tokio::fs::rename(temporary_path, manifest_path).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn record_llm_call(
    state: &AppState,
    job: &AgentJob,
    profile_id: &str,
    profile: &ModelProfileCredentials,
    status: &str,
    usage: &TokenUsageSummary,
    started: Instant,
    error: Option<&str>,
) {
    let latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    if let Err(record_error) = db::record_llm_call(
        &state.pool,
        db::LlmCallInput {
            company_id: &job.company_id,
            employee_id: &job.employee_id,
            job_id: Some(&job.id),
            model_profile_id: profile_id,
            model: &profile.model,
            status,
            usage,
            latency_ms,
            error,
        },
    )
    .await
    {
        tracing::warn!(%record_error, run_id = %job.id, "cannot record ADK LLM usage");
    }
}

fn default_run_status() -> String {
    "COMPLETED".to_string()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ModelRequest, RunRequest};

    #[test]
    fn runtime_request_serializes_explicit_sandbox_marker() {
        let request = RunRequest {
            company_id: "company-01",
            run_id: "run-01",
            agent_job_id: "job-01",
            employee_id: "employee-01",
            session_id: "session-01",
            system_prompt: "system",
            content: "content",
            model: ModelRequest {
                protocol: "openai",
                base_url: "http://localhost",
                api_key: None,
                model: "test",
            },
            allow_tools: true,
            sandboxed: true,
            tool_approval: None,
        };

        let encoded = serde_json::to_value(request).expect("runtime request should serialize");
        assert_eq!(encoded["sandboxed"], json!(true));
    }
}
