use chrono::Utc;
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::models::{
    RuntimeEnvironmentSuggestionSummary, RuntimeWorkspaceSummary, TokenUsageSummary,
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

pub async fn record_tool_call(
    pool: &SqlitePool,
    run_id: &str,
    employee_id: &str,
    tool_name: &str,
    arguments_redacted: &str,
    status: &str,
    result_summary: Option<&str>,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO runtime_tool_calls
         (id, run_id, employee_id, tool_name, arguments_redacted, status,
          approval_status, started_at, completed_at, result_summary)
         VALUES (?, ?, ?, ?, ?, ?, 'NOT_REQUIRED', ?, ?, ?)",
    )
    .bind(format!("runtime-tool-call-{}", Uuid::new_v4()))
    .bind(run_id)
    .bind(employee_id)
    .bind(tool_name)
    .bind(arguments_redacted)
    .bind(status)
    .bind(&now)
    .bind(&now)
    .bind(result_summary)
    .execute(pool)
    .await?;
    Ok(())
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
