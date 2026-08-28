use chrono::Utc;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::domain::models::{
    RuntimeEnvironmentSuggestionSummary, RuntimeToolApprovalSummary, RuntimeWorkspaceSummary,
    TokenUsageSummary,
};

#[derive(Debug, FromRow)]
struct RuntimeWorkspaceRow {
    id: String,
    company_id: String,
    employee_id: String,
    host_root_path: String,
    workspace_path: String,
    state_path: String,
    artifacts_path: String,
    environment_path: String,
    status: String,
    last_error: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, FromRow)]
struct RuntimeEnvironmentSuggestionRow {
    id: String,
    company_id: String,
    employee_id: String,
    runtime_run_id: Option<String>,
    ecosystem: String,
    package_name: String,
    requested_spec: Option<String>,
    source_command: String,
    status: String,
    created_at: String,
    updated_at: String,
}

pub struct RuntimeWorkspaceUpsert<'a> {
    pub company_id: &'a str,
    pub employee_id: &'a str,
    pub host_root_path: &'a str,
    pub workspace_path: &'a str,
    pub state_path: &'a str,
    pub artifacts_path: &'a str,
    pub environment_path: &'a str,
}

pub struct EnvironmentSuggestionInput<'a> {
    pub company_id: &'a str,
    pub employee_id: &'a str,
    pub runtime_run_id: &'a str,
    pub ecosystem: &'a str,
    pub package_name: &'a str,
    pub requested_spec: Option<&'a str>,
    pub source_command: &'a str,
}

pub async fn recover_interrupted_runs(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE runtime_runs
         SET status = 'FAILED', completed_at = ?, heartbeat_at = ?,
             stop_reason = 'backend restarted before runtime completion'
         WHERE status = 'RUNNING'",
    )
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn workspace_for_employee(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<Option<RuntimeWorkspaceSummary>, sqlx::Error> {
    let row = sqlx::query_as::<_, RuntimeWorkspaceRow>(
        "SELECT id, company_id, employee_id, host_root_path, workspace_path,
                state_path, artifacts_path, environment_path, status, last_error,
                created_at, updated_at
         FROM employee_runtime_workspaces WHERE company_id = ? AND employee_id = ?",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(workspace_from_row))
}

pub async fn upsert_workspace(
    pool: &SqlitePool,
    input: RuntimeWorkspaceUpsert<'_>,
) -> Result<RuntimeWorkspaceSummary, sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO employee_runtime_workspaces
         (id, company_id, employee_id, host_root_path, workspace_path, state_path,
          artifacts_path, environment_path, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'READY', ?, ?)
         ON CONFLICT(employee_id) DO UPDATE SET
           host_root_path = excluded.host_root_path,
           workspace_path = excluded.workspace_path,
           state_path = excluded.state_path,
           artifacts_path = excluded.artifacts_path,
           environment_path = excluded.environment_path,
           status = 'READY', last_error = NULL, updated_at = excluded.updated_at",
    )
    .bind(format!("runtime-workspace-{}", Uuid::new_v4()))
    .bind(input.company_id)
    .bind(input.employee_id)
    .bind(input.host_root_path)
    .bind(input.workspace_path)
    .bind(input.state_path)
    .bind(input.artifacts_path)
    .bind(input.environment_path)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    workspace_for_employee(pool, input.company_id, input.employee_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn archive_workspace(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE employee_runtime_workspaces SET status = 'ARCHIVED', updated_at = ?
         WHERE company_id = ? AND employee_id = ?",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(company_id)
    .bind(employee_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn environment_suggestions(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<Vec<RuntimeEnvironmentSuggestionSummary>, sqlx::Error> {
    let rows = sqlx::query_as::<_, RuntimeEnvironmentSuggestionRow>(
        "SELECT id, company_id, employee_id, runtime_run_id, ecosystem, package_name,
                requested_spec, source_command, status, created_at, updated_at
         FROM runtime_environment_suggestions WHERE company_id = ?
         ORDER BY created_at DESC, id DESC",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(suggestion_from_row).collect())
}

pub async fn recommend_environment_change(
    pool: &SqlitePool,
    input: EnvironmentSuggestionInput<'_>,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO runtime_environment_suggestions
         (id, company_id, employee_id, runtime_run_id, ecosystem, package_name,
          requested_spec, source_command, status, created_at, updated_at)
         SELECT ?, ?, ?, ?, ?, ?, ?, ?, 'SUGGESTED', ?, ?
         WHERE NOT EXISTS (
           SELECT 1 FROM runtime_environment_suggestions
           WHERE company_id = ? AND employee_id = ? AND ecosystem = ?
             AND package_name = ? AND status IN ('SUGGESTED', 'ACCEPTED', 'PACKAGED')
         )",
    )
    .bind(format!("runtime-env-suggestion-{}", Uuid::new_v4()))
    .bind(input.company_id)
    .bind(input.employee_id)
    .bind(input.runtime_run_id)
    .bind(input.ecosystem)
    .bind(input.package_name)
    .bind(input.requested_spec)
    .bind(input.source_command)
    .bind(&now)
    .bind(&now)
    .bind(input.company_id)
    .bind(input.employee_id)
    .bind(input.ecosystem)
    .bind(input.package_name)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_run(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
    agent_job_id: &str,
    runtime_session_id: &str,
) -> Result<String, sqlx::Error> {
    let id = format!("runtime-run-{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO runtime_runs
         (id, company_id, employee_id, agent_job_id, runtime_session_id,
          container_id, status, started_at, heartbeat_at)
         VALUES (?, ?, ?, ?, ?, NULL, 'RUNNING', ?, ?)",
    )
    .bind(&id)
    .bind(company_id)
    .bind(employee_id)
    .bind(agent_job_id)
    .bind(runtime_session_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE agent_jobs SET runtime_run_id = ?, heartbeat_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&id)
    .bind(&now)
    .bind(&now)
    .bind(agent_job_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(id)
}

pub async fn finish_run(
    pool: &SqlitePool,
    run_id: &str,
    status: &str,
    usage: &TokenUsageSummary,
    stop_reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE runtime_runs SET status = ?, completed_at = ?, heartbeat_at = ?,
          stop_reason = ?, input_tokens = ?, output_tokens = ?, cache_tokens = ?, total_tokens = ?
         WHERE id = ?",
    )
    .bind(status)
    .bind(&now)
    .bind(&now)
    .bind(stop_reason)
    .bind(usage.input_tokens)
    .bind(usage.output_tokens)
    .bind(usage.cache_tokens)
    .bind(usage.total_tokens)
    .bind(run_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn record_tool_call(
    pool: &SqlitePool,
    run_id: &str,
    employee_id: &str,
    tool_name: &str,
    function_call_id: Option<&str>,
    arguments_redacted: &str,
    status: &str,
    result_summary: Option<&str>,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let completed_at = (status == "COMPLETED").then_some(now.clone());
    if let Some(function_call_id) = function_call_id {
        let updated = sqlx::query(
            "UPDATE runtime_tool_calls
             SET tool_name = ?,
                 arguments_redacted = CASE WHEN ? = '{}' THEN arguments_redacted ELSE ? END,
                 status = ?, completed_at = COALESCE(?, completed_at),
                 result_summary = COALESCE(?, result_summary)
             WHERE run_id = ? AND function_call_id = ?",
        )
        .bind(tool_name)
        .bind(arguments_redacted)
        .bind(arguments_redacted)
        .bind(status)
        .bind(completed_at.as_deref())
        .bind(result_summary)
        .bind(run_id)
        .bind(function_call_id)
        .execute(pool)
        .await?;
        if updated.rows_affected() > 0 {
            return Ok(());
        }
    }
    sqlx::query(
        "INSERT INTO runtime_tool_calls
         (id, run_id, employee_id, function_call_id, tool_name, arguments_redacted, status,
          approval_status, started_at, completed_at, result_summary)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'NOT_REQUIRED', ?, ?, ?)",
    )
    .bind(format!("runtime-tool-call-{}", Uuid::new_v4()))
    .bind(run_id)
    .bind(employee_id)
    .bind(function_call_id)
    .bind(tool_name)
    .bind(arguments_redacted)
    .bind(status)
    .bind(&now)
    .bind(completed_at.as_deref())
    .bind(result_summary)
    .execute(pool)
    .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn create_tool_approval(
    pool: &SqlitePool,
    company_id: &str,
    run_id: &str,
    agent_job_id: &str,
    employee_id: &str,
    function_call_id: &str,
    tool_name: &str,
    arguments_redacted: &str,
    arguments_fingerprint: &str,
) -> Result<String, sqlx::Error> {
    let id = format!("runtime-approval-{}", Uuid::new_v4());
    let now = Utc::now();
    let expires_at = (now + chrono::Duration::minutes(30)).to_rfc3339();
    sqlx::query(
        "INSERT INTO runtime_tool_approvals
         (id, company_id, run_id, agent_job_id, employee_id, function_call_id,
          tool_name, arguments_redacted, arguments_fingerprint, status, created_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'PENDING', ?, ?)
         ON CONFLICT(run_id, function_call_id) DO NOTHING",
    )
    .bind(&id)
    .bind(company_id)
    .bind(run_id)
    .bind(agent_job_id)
    .bind(employee_id)
    .bind(function_call_id)
    .bind(tool_name)
    .bind(arguments_redacted)
    .bind(arguments_fingerprint)
    .bind(now.to_rfc3339())
    .bind(&expires_at)
    .execute(pool)
    .await?;
    sqlx::query(
        "UPDATE runtime_tool_calls SET approval_status = 'PENDING', function_call_id = ?
         WHERE run_id = ? AND tool_name = ? AND approval_status = 'NOT_REQUIRED'",
    )
    .bind(function_call_id)
    .bind(run_id)
    .bind(tool_name)
    .execute(pool)
    .await?;
    let approval_id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM runtime_tool_approvals WHERE run_id = ? AND function_call_id = ?",
    )
    .bind(run_id)
    .bind(function_call_id)
    .fetch_one(pool)
    .await?;
    Ok(approval_id)
}

pub async fn pending_tool_approvals(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<Vec<RuntimeToolApprovalSummary>, sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE runtime_tool_approvals
         SET status = 'EXPIRED', decided_at = ?
         WHERE company_id = ? AND status = 'PENDING' AND expires_at <= ?",
    )
    .bind(&now)
    .bind(company_id)
    .bind(&now)
    .execute(pool)
    .await?;
    let rows = sqlx::query_as::<_, RuntimeToolApprovalRow>(
        "SELECT id, company_id, run_id, agent_job_id, employee_id, function_call_id,
                tool_name, arguments_redacted, arguments_fingerprint, status,
                decided_by, decision_reason, created_at, expires_at, decided_at
         FROM runtime_tool_approvals
         WHERE company_id = ? AND status = 'PENDING'
         ORDER BY created_at DESC, id DESC",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(approval_from_row).collect())
}

pub async fn decide_tool_approval(
    pool: &SqlitePool,
    company_id: &str,
    approval_id: &str,
    decision: &str,
    decided_by: &str,
    reason: Option<&str>,
) -> Result<Option<RuntimeToolApprovalSummary>, sqlx::Error> {
    let decision = decision.to_ascii_uppercase();
    if !matches!(decision.as_str(), "APPROVED" | "DENIED") {
        return Ok(None);
    }
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE runtime_tool_approvals SET status = 'EXPIRED', decided_at = ?
         WHERE id = ? AND company_id = ? AND status = 'PENDING' AND expires_at <= ?",
    )
    .bind(&now)
    .bind(approval_id)
    .bind(company_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    let row = sqlx::query_as::<_, RuntimeToolApprovalRow>(
        "SELECT id, company_id, run_id, agent_job_id, employee_id, function_call_id,
                tool_name, arguments_redacted, arguments_fingerprint, status,
                decided_by, decision_reason, created_at, expires_at, decided_at
         FROM runtime_tool_approvals
         WHERE id = ? AND company_id = ? AND status = 'PENDING' AND expires_at > ?",
    )
    .bind(approval_id)
    .bind(company_id)
    .bind(&now)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    sqlx::query(
        "UPDATE runtime_tool_approvals
         SET status = ?, decided_by = ?, decision_reason = ?, decided_at = ?
         WHERE id = ? AND status = 'PENDING'",
    )
    .bind(&decision)
    .bind(decided_by)
    .bind(reason)
    .bind(&now)
    .bind(approval_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE runtime_tool_calls SET approval_status = ?, completed_at = ?
         WHERE run_id = ? AND function_call_id = ?",
    )
    .bind(&decision)
    .bind(&now)
    .bind(&row.run_id)
    .bind(&row.function_call_id)
    .execute(&mut *tx)
    .await?;
    let current_payload =
        sqlx::query_scalar::<_, String>("SELECT payload FROM agent_jobs WHERE id = ?")
            .bind(&row.agent_job_id)
            .fetch_one(&mut *tx)
            .await?;
    let mut payload = serde_json::from_str::<serde_json::Value>(&current_payload)
        .unwrap_or_else(|_| serde_json::json!({}));
    payload["toolApproval"] = serde_json::json!({
        "callId": row.function_call_id,
        "decision": if decision == "APPROVED" { "approve" } else { "deny" },
        "fingerprint": row.arguments_fingerprint,
    });
    let payload = payload.to_string();
    sqlx::query(
        "UPDATE agent_jobs SET status = 'PENDING', payload = ?,
                available_at = ?, updated_at = ?, error = NULL
         WHERE id = ? AND status IN ('WAITING', 'WAITING_APPROVAL', 'RUNNING')",
    )
    .bind(payload)
    .bind(&now)
    .bind(&now)
    .bind(&row.agent_job_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Some(approval_from_row(RuntimeToolApprovalRow {
        status: decision,
        decided_by: Some(decided_by.to_string()),
        decision_reason: reason.map(str::to_string),
        decided_at: Some(now),
        ..row
    })))
}

#[derive(Debug, FromRow)]
struct RuntimeToolApprovalRow {
    id: String,
    company_id: String,
    run_id: String,
    agent_job_id: String,
    employee_id: String,
    function_call_id: String,
    tool_name: String,
    arguments_redacted: String,
    arguments_fingerprint: String,
    status: String,
    decided_by: Option<String>,
    decision_reason: Option<String>,
    created_at: String,
    expires_at: String,
    decided_at: Option<String>,
}

fn approval_from_row(row: RuntimeToolApprovalRow) -> RuntimeToolApprovalSummary {
    RuntimeToolApprovalSummary {
        id: row.id,
        company_id: row.company_id,
        run_id: row.run_id,
        agent_job_id: row.agent_job_id,
        employee_id: row.employee_id,
        function_call_id: row.function_call_id,
        tool_name: row.tool_name,
        arguments_redacted: row.arguments_redacted,
        arguments_fingerprint: row.arguments_fingerprint,
        status: row.status,
        decided_by: row.decided_by,
        decision_reason: row.decision_reason,
        created_at: row.created_at,
        expires_at: row.expires_at,
        decided_at: row.decided_at,
    }
}

fn workspace_from_row(row: RuntimeWorkspaceRow) -> RuntimeWorkspaceSummary {
    RuntimeWorkspaceSummary {
        id: row.id,
        company_id: row.company_id,
        employee_id: row.employee_id,
        host_root_path: row.host_root_path,
        workspace_path: row.workspace_path,
        state_path: row.state_path,
        artifacts_path: row.artifacts_path,
        environment_path: row.environment_path,
        status: row.status,
        last_error: row.last_error,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn suggestion_from_row(
    row: RuntimeEnvironmentSuggestionRow,
) -> RuntimeEnvironmentSuggestionSummary {
    RuntimeEnvironmentSuggestionSummary {
        id: row.id,
        company_id: row.company_id,
        employee_id: row.employee_id,
        runtime_run_id: row.runtime_run_id,
        ecosystem: row.ecosystem,
        package_name: row.package_name,
        requested_spec: row.requested_spec,
        source_command: row.source_command,
        status: row.status,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}
