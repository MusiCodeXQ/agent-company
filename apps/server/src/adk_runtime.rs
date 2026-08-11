use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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
    run_id: &'a str,
    employee_id: &'a str,
    session_id: &'a str,
    system_prompt: &'a str,
    content: &'a str,
    model: ModelRequest<'a>,
    allow_tools: bool,
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentChange {
    ecosystem: String,
    package_name: String,
    requested_spec: Option<String>,
    source_command: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeEvent {
    event_type: String,
    tool_name: Option<String>,
    arguments: Option<Value>,
    status: Option<String>,
    result_summary: Option<String>,
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
    let request = RunRequest {
        run_id: &run_id,
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
        allow_tools: job.meeting_id.is_none(),
    };
    let input = serde_json::to_vec(&request)?;
    let started = Instant::now();
    let output = match state
        .container_engine
        .exec_employee_runtime(
            &RunContainerRequest {
                company_id: job.company_id.clone(),
                employee_id: job.employee_id.clone(),
                run_id: run_id.clone(),
                workspace_path: workspace.workspace_path.clone().into(),
                state_path: workspace.state_path.clone().into(),
                artifacts_path: workspace.artifacts_path.clone().into(),
                environment_path: workspace.environment_path.clone().into(),
            },
            &input,
        )
        .await
    {
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
    let response: RunResponse = match serde_json::from_slice(&output)
        .context("decode response from employee ADK runtime")
    {
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
    };
    for event in &response.events {
        if let Some(tool_name) = event.tool_name.as_deref() {
            runtime_store::record_tool_call(
                &state.pool,
                &run_id,
                &job.employee_id,
                tool_name,
                &event
                    .arguments
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "{}".to_string())
                    .chars()
                    .take(2_000)
                    .collect::<String>(),
                event.status.as_deref().unwrap_or("COMPLETED"),
                event.result_summary.as_deref(),
            )
            .await?;
        }
        db::record_runtime_event(
            &state.pool,
            &job.company_id,
            Some(&job.employee_id),
            Some(&employee.id),
            Some(&run_id),
            &event.event_type,
            &serde_json::to_string(event.arguments.as_ref().unwrap_or(&Value::Null))?,
        )
        .await?;
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
            job_id: &job.id,
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
