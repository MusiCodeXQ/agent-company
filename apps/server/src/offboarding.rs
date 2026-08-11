use anyhow::{anyhow, Result};
use chrono::Utc;
use serde_json::json;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    app_state::AppState,
    container_runtime, db,
    db::MovementArrival,
    events,
    models::{TerminationPreview, TerminationRecordSummary, TerminationTaskSummary},
    office, openclaw, spatial,
};

const ACTIVE_TASK_STATUSES: &str = "('PENDING', 'READY', 'IN_PROGRESS', 'WAITING', 'RUNNING')";

pub async fn preview(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<TerminationPreview> {
    let employee = db::employees(pool, company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| anyhow!("employee was not found: {employee_id}"))?;
    let task_rows = sqlx::query(&format!(
        "SELECT id, title, status, goal_id FROM tasks
         WHERE company_id = ? AND assignee_id = ? AND status IN {ACTIVE_TASK_STATUSES}
         ORDER BY created_at"
    ))
    .bind(company_id)
    .bind(employee_id)
    .fetch_all(pool)
    .await?;
    let tasks = task_rows
        .iter()
        .map(|row| {
            Ok(TerminationTaskSummary {
                id: row.try_get("id")?,
                title: row.try_get("title")?,
                status: row.try_get("status")?,
                goal_id: row.try_get("goal_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let active_task_count = tasks.len() as i64;
    let active_goal_count: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(DISTINCT goal_id) FROM tasks
         WHERE company_id = ? AND assignee_id = ? AND goal_id IS NOT NULL
           AND status IN {ACTIVE_TASK_STATUSES}"
    ))
    .bind(company_id)
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    let running_job_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM agent_jobs
         WHERE company_id = ? AND employee_id = ? AND status IN ('PENDING', 'RUNNING', 'WAITING')",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    let active_meeting_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM meeting_participants p
         JOIN meetings m ON m.id = p.meeting_id
         WHERE m.company_id = ? AND p.agent_id = ?
           AND m.status IN ('CREATED', 'WAITING_FOR_PARTICIPANTS', 'IN_PROGRESS')
           AND p.status != 'LEFT'",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    let active_termination_id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM termination_records
         WHERE company_id = ? AND employee_id = ?
           AND status IN ('PENDING_CONFIRMATION', 'TERMINATING')
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_optional(pool)
    .await?;
    let (workspace_path, runtime_type, runtime_status) = employee
        .runtime_binding
        .as_ref()
        .map(|binding| {
            (
                Some(binding.workspace_path.clone()),
                binding.runtime_type.clone(),
                binding.runtime_status.clone(),
            )
        })
        .unwrap_or((None, "MOCK".to_string(), "ACTIVE".to_string()));
    Ok(TerminationPreview {
        workstation_id: employee.desk_id.clone(),
        workspace_status: if workspace_path.is_some() {
            "ACTIVE".to_string()
        } else {
            "NONE".to_string()
        },
        workspace_path,
        runtime_type,
        runtime_status,
        unread_count: employee.unread_count,
        employee,
        active_task_count,
        running_job_count,
        active_goal_count,
        active_meeting_count,
        // Artifact persistence is not present in the original V1 schema yet;
        // keep the preview explicit instead of inventing a count.
        artifact_count: 0,
        tasks,
        active_termination_id,
    })
}

pub async fn records(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<Vec<TerminationRecordSummary>> {
    let rows = sqlx::query(
        "SELECT id, company_id, employee_id, initiated_by, reason, notes, status,
                workspace_action, task_handover_status, runtime_shutdown_status,
                workstation_release_status, created_at, effective_at, completed_at
         FROM termination_records
         WHERE company_id = ? AND employee_id = ? ORDER BY created_at DESC",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(record_from_row)
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Into::into)
}

pub async fn create_request(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
    initiated_by: &str,
    reason: &str,
    notes: &str,
) -> Result<TerminationRecordSummary> {
    if let Some(existing) = sqlx::query(
        "SELECT id, company_id, employee_id, initiated_by, reason, notes, status,
                workspace_action, task_handover_status, runtime_shutdown_status,
                workstation_release_status, created_at, effective_at, completed_at
         FROM termination_records
         WHERE company_id = ? AND employee_id = ?
           AND status IN ('PENDING_CONFIRMATION', 'TERMINATING')
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_optional(pool)
    .await?
    {
        return Ok(record_from_row(existing)?);
    }
    let employee_status = sqlx::query_scalar::<_, String>(
        "SELECT employment_status FROM employees WHERE company_id = ? AND id = ?",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    if employee_status == "TERMINATED" {
        return Err(anyhow!("employee is already terminated"));
    }
    let now = Utc::now().to_rfc3339();
    let id = format!("termination-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO termination_records
         (id, company_id, employee_id, initiated_by, reason, notes, status,
          workspace_action, task_handover_status, runtime_shutdown_status,
          workstation_release_status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, 'PENDING_CONFIRMATION', 'ARCHIVE', 'PENDING',
                 'PENDING', 'PENDING', ?)",
    )
    .bind(&id)
    .bind(company_id)
    .bind(employee_id)
    .bind(initiated_by)
    .bind(reason.trim())
    .bind(notes.trim())
    .bind(&now)
    .execute(pool)
    .await?;
    let row = sqlx::query(
        "SELECT id, company_id, employee_id, initiated_by, reason, notes, status,
                workspace_action, task_handover_status, runtime_shutdown_status,
                workstation_release_status, created_at, effective_at, completed_at
         FROM termination_records WHERE id = ?",
    )
    .bind(id)
    .fetch_one(pool)
    .await?;
    Ok(record_from_row(row)?)
}

pub async fn handle_administrator_message(
    state: &AppState,
    company_id: &str,
    administrator_employee_id: &str,
    content: &str,
) -> Result<Option<String>> {
    if is_confirmation(content) {
        if let Some(row) = sqlx::query(
            "SELECT employee_id, id FROM termination_records
             WHERE company_id = ? AND initiated_by = ? AND status = 'PENDING_CONFIRMATION'
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(company_id)
        .bind(administrator_employee_id)
        .fetch_optional(&state.pool)
        .await?
        {
            let employee_id: String = row.try_get("employee_id")?;
            let termination_id: String = row.try_get("id")?;
            let record = start(
                state,
                company_id,
                &employee_id,
                administrator_employee_id,
                Some(&termination_id),
                "由管理员确认",
                "",
                "REASSIGN",
                "ARCHIVE",
            )
            .await?;
            return Ok(Some(format!(
                "已确认解雇流程（{}）。未完成任务已标记为待重新分配，Workspace 将归档保留；员工会先沿后端路线离开办公室。",
                record.employee_id
            )));
        }
    }
    if content.contains("不要") || content.contains("取消") {
        return Ok(None);
    }
    if !termination_intent(content) {
        return Ok(None);
    }
    let employee = db::employees(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|employee| {
            employee.id != administrator_employee_id
                && employee.employment_status == "ACTIVE"
                && content.contains(&employee.name)
        });
    let Some(employee) = employee else {
        return Ok(None);
    };
    let request = create_request(
        &state.pool,
        company_id,
        &employee.id,
        administrator_employee_id,
        "由管理员对话发起",
        content,
    )
    .await?;
    let preview = preview(&state.pool, company_id, &employee.id).await?;
    Ok(Some(format_preview_message(&preview, &request.id)))
}

#[allow(clippy::too_many_arguments)]
pub async fn start(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
    initiated_by: &str,
    termination_id: Option<&str>,
    reason: &str,
    notes: &str,
    task_action: &str,
    workspace_action: &str,
) -> Result<TerminationRecordSummary> {
    let task_action = normalize_task_action(task_action)?;
    let workspace_action = normalize_workspace_action(workspace_action)?;
    let employee = db::employees(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| anyhow!("employee was not found: {employee_id}"))?;
    if employee.employment_status == "TERMINATED" {
        return Err(anyhow!("employee is already terminated"));
    }
    if employee.is_floor_administrator {
        return Err(anyhow!(
            "楼层管理员不能直接解雇；请先完成管理员交接，以保证楼层始终有管理员"
        ));
    }
    let termination_id = match termination_id {
        Some(id) => id.to_string(),
        None => {
            create_request(
                &state.pool,
                company_id,
                employee_id,
                initiated_by,
                reason,
                notes,
            )
            .await?
            .id
        }
    };
    let now = Utc::now().to_rfc3339();
    let mut tx = state.pool.begin().await?;
    let task_rows = sqlx::query(&format!(
        "SELECT id, assignee_id FROM tasks
         WHERE company_id = ? AND assignee_id = ? AND status IN {ACTIVE_TASK_STATUSES}"
    ))
    .bind(company_id)
    .bind(employee_id)
    .fetch_all(&mut *tx)
    .await?;
    for task in &task_rows {
        let task_id: String = task.try_get("id")?;
        let previous_assignee_id: Option<String> = task.try_get("assignee_id")?;
        let next_status = match task_action.as_str() {
            "REASSIGN" => "NEEDS_REASSIGNMENT",
            "PAUSE" => "PAUSED",
            "CANCEL" => "CANCELLED",
            _ => unreachable!(),
        };
        let handover_status = match task_action.as_str() {
            "REASSIGN" => "NEEDS_REASSIGNMENT",
            "PAUSE" => "PAUSED",
            "CANCEL" => "CANCELLED",
            _ => unreachable!(),
        };
        sqlx::query(
            "UPDATE tasks SET previous_assignee_id = ?, assignee_id = NULL,
             status = ?, handover_status = ?, updated_at = ?
             WHERE company_id = ? AND id = ?",
        )
        .bind(&previous_assignee_id)
        .bind(next_status)
        .bind(handover_status)
        .bind(&now)
        .bind(company_id)
        .bind(&task_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO task_handover_records
             (id, company_id, termination_id, task_id, previous_assignee_id,
              new_assignee_id, status, notes, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, NULL, ?, '', ?, ?)",
        )
        .bind(format!("handover-{}", Uuid::new_v4()))
        .bind(company_id)
        .bind(&termination_id)
        .bind(&task_id)
        .bind(previous_assignee_id)
        .bind(handover_status)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE agent_jobs SET status = 'CANCELLED', error = 'employee is terminating', updated_at = ?
         WHERE company_id = ? AND employee_id = ? AND status IN ('PENDING', 'RUNNING', 'WAITING')",
    )
    .bind(&now)
    .bind(company_id)
    .bind(employee_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE meeting_participants SET status = 'LEFT', updated_at = ?
         WHERE agent_id = ? AND status != 'LEFT'",
    )
    .bind(&now)
    .bind(employee_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE employees SET employment_status = 'TERMINATING', status = 'offboarding',
         presence = 'BUSY', activity = 'IDLE', device = 'NONE', posture = 'STANDING'
         WHERE company_id = ? AND id = ?",
    )
    .bind(company_id)
    .bind(employee_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE employee_runtime_bindings SET runtime_status = 'DRAINING',
         last_error = NULL, updated_at = ? WHERE company_id = ? AND employee_id = ?",
    )
    .bind(&now)
    .bind(company_id)
    .bind(employee_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE termination_records SET status = 'TERMINATING', workspace_action = ?,
         task_handover_status = ?, runtime_shutdown_status = 'DRAINING',
         workstation_release_status = 'PENDING', effective_at = ?, reason = ?, notes = ?
         WHERE id = ? AND company_id = ?",
    )
    .bind(workspace_action)
    .bind("COMPLETED")
    .bind(&now)
    .bind(reason.trim())
    .bind(notes.trim())
    .bind(&termination_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    events::emit(
        state,
        company_id,
        "EMPLOYEE_TERMINATION_REQUESTED",
        Some(initiated_by),
        Some(&termination_id),
        json!({ "employeeId": employee_id, "terminationId": termination_id }),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "EMPLOYEE_TERMINATION_STARTED",
        Some(employee_id),
        Some(&termination_id),
        json!({ "employeeId": employee_id, "terminationId": termination_id }),
    )
    .await?;
    if !task_rows.is_empty() {
        events::emit(
            state,
            company_id,
            "TASK_REASSIGNMENT_REQUIRED",
            Some(employee_id),
            Some(&termination_id),
            json!({ "employeeId": employee_id, "taskCount": task_rows.len(), "taskAction": task_action }),
        )
        .await?;
    }
    events::emit(
        state,
        company_id,
        "EMPLOYEE_RUNTIME_DRAINING",
        Some(employee_id),
        Some(&termination_id),
        json!({ "employeeId": employee_id }),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "EMPLOYEE_PERMISSIONS_REVOKED",
        Some(employee_id),
        Some(&termination_id),
        json!({ "employeeId": employee_id }),
    )
    .await?;

    if let Some(binding) = employee.runtime_binding.as_ref() {
        if binding.runtime_type == "OPENCLAW" {
            let session_key = format!("agent:{}:main", binding.runtime_agent_id);
            let runtime_error = openclaw::archive_agent(&binding.runtime_agent_id, &session_key)
                .await
                .err()
                .map(|error| error.to_string());
            db::update_runtime_binding_status(
                &state.pool,
                company_id,
                employee_id,
                Some("DISABLED"),
                Some("ARCHIVED"),
                runtime_error.as_deref(),
            )
            .await?;
        } else if binding.runtime_type == "ADK_RUST" {
            let runtime_error = container_runtime::archive_employee(state, company_id, employee_id)
                .await
                .err()
                .map(|error| error.to_string());
            db::update_runtime_binding_status(
                &state.pool,
                company_id,
                employee_id,
                Some("DISABLED"),
                Some("ARCHIVED"),
                runtime_error.as_deref(),
            )
            .await?;
        } else {
            db::update_runtime_binding_status(
                &state.pool,
                company_id,
                employee_id,
                Some("DISABLED"),
                Some("ARCHIVED"),
                None,
            )
            .await?;
        }
    }
    events::emit(
        state,
        company_id,
        "EMPLOYEE_RUNTIME_DISABLED",
        Some(employee_id),
        Some(&termination_id),
        json!({ "employeeId": employee_id }),
    )
    .await?;

    let destination = office::floor_location_id(
        employee.floor_id.as_deref().unwrap_or("floor-01"),
        "office_entrance",
    );
    if employee.location_id.as_deref() == Some(destination.as_str())
        && employee.active_movement_id.is_none()
    {
        finalize(state, company_id, employee_id, &termination_id).await?;
    } else if let Err(error) =
        spatial::request_move(state, company_id, employee_id, &destination, "OFFBOARDING").await
    {
        tracing::warn!(%error, employee_id, "offboarding movement was not started");
    } else {
        events::emit(
            state,
            company_id,
            "EMPLOYEE_LEAVING_OFFICE",
            Some(employee_id),
            Some(&termination_id),
            json!({ "employeeId": employee_id, "destinationLocationId": destination }),
        )
        .await?;
    }
    records(&state.pool, company_id, employee_id)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("termination record disappeared"))
}

pub async fn on_agent_arrived(state: &AppState, arrival: &MovementArrival) -> Result<()> {
    let employee = db::employees(&state.pool, &arrival.movement.company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == arrival.agent_id);
    let Some(employee) = employee else {
        return Ok(());
    };
    if employee.employment_status != "TERMINATING" {
        return Ok(());
    }
    let location_type = sqlx::query_scalar::<_, String>(
        "SELECT type FROM office_locations WHERE company_id = ? AND id = ?",
    )
    .bind(&arrival.movement.company_id)
    .bind(&arrival.to_location_id)
    .fetch_optional(&state.pool)
    .await?;
    if location_type.as_deref() == Some("ENTRANCE") {
        let termination_id = sqlx::query_scalar::<_, String>(
            "SELECT id FROM termination_records
             WHERE company_id = ? AND employee_id = ? AND status = 'TERMINATING'
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&arrival.movement.company_id)
        .bind(&arrival.agent_id)
        .fetch_optional(&state.pool)
        .await?;
        if let Some(termination_id) = termination_id {
            finalize(
                state,
                &arrival.movement.company_id,
                &arrival.agent_id,
                &termination_id,
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn finalize(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
    termination_id: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE employees SET employment_status = 'TERMINATED', status = 'offline',
         current_state = 'IDLE', activity = 'IDLE', movement = 'STATIONARY',
         device = 'NONE', posture = 'STANDING', presence = 'OFFLINE',
         active_movement_id = NULL, current_destination = NULL
         WHERE company_id = ? AND id = ?",
    )
    .bind(company_id)
    .bind(employee_id)
    .execute(&mut *tx)
    .await?;
    let released = sqlx::query(
        "UPDATE workstations SET assigned_employee_id = NULL, status = 'AVAILABLE', updated_at = ?
         WHERE company_id = ? AND assigned_employee_id = ?",
    )
    .bind(&now)
    .bind(company_id)
    .bind(employee_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query("DELETE FROM location_reservations WHERE company_id = ? AND agent_id = ?")
        .bind(company_id)
        .bind(employee_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE employee_runtime_bindings SET provision_status = 'DISABLED',
         runtime_status = 'ARCHIVED', updated_at = ? WHERE company_id = ? AND employee_id = ?",
    )
    .bind(&now)
    .bind(company_id)
    .bind(employee_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE termination_records SET status = 'COMPLETED',
         runtime_shutdown_status = 'ARCHIVED', workstation_release_status = ?,
         completed_at = ? WHERE id = ? AND company_id = ?",
    )
    .bind(if released > 0 {
        "RELEASED"
    } else {
        "NOT_ASSIGNED"
    })
    .bind(&now)
    .bind(termination_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    events::emit(
        state,
        company_id,
        "EMPLOYEE_WORKSTATION_RELEASED",
        Some(employee_id),
        Some(termination_id),
        json!({ "employeeId": employee_id, "released": released > 0 }),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "EMPLOYEE_LEFT_OFFICE",
        Some(employee_id),
        Some(termination_id),
        json!({ "employeeId": employee_id }),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "EMPLOYEE_TERMINATED",
        Some(employee_id),
        Some(termination_id),
        json!({ "employeeId": employee_id, "terminationId": termination_id }),
    )
    .await?;
    Ok(())
}

fn record_from_row(row: sqlx::sqlite::SqliteRow) -> Result<TerminationRecordSummary, sqlx::Error> {
    Ok(TerminationRecordSummary {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        employee_id: row.try_get("employee_id")?,
        initiated_by: row.try_get("initiated_by")?,
        reason: row.try_get("reason")?,
        notes: row.try_get("notes")?,
        status: row.try_get("status")?,
        workspace_action: row.try_get("workspace_action")?,
        task_handover_status: row.try_get("task_handover_status")?,
        runtime_shutdown_status: row.try_get("runtime_shutdown_status")?,
        workstation_release_status: row.try_get("workstation_release_status")?,
        created_at: row.try_get("created_at")?,
        effective_at: row.try_get("effective_at")?,
        completed_at: row.try_get("completed_at")?,
    })
}

fn format_preview_message(preview: &TerminationPreview, termination_id: &str) -> String {
    format!(
        "准备解雇：{}（{}）。当前未完成任务：{}，运行中 Job：{}，负责 Goal：{}，进行中会议：{}，工位：{}，Runtime：{} / {}，Workspace：{}。默认将任务标记为待重新分配、Workspace 归档保留。请回复“继续”确认执行。\ntermination_id={}",
        preview.employee.name,
        preview.employee.title,
        preview.active_task_count,
        preview.running_job_count,
        preview.active_goal_count,
        preview.active_meeting_count,
        preview.workstation_id.as_deref().unwrap_or("无"),
        preview.runtime_type,
        preview.runtime_status,
        preview.workspace_status,
        termination_id
    )
}

fn termination_intent(content: &str) -> bool {
    [
        "解雇",
        "辞退",
        "开除",
        "终止劳动关系",
        "让他离职",
        "让她离职",
        "让其离职",
    ]
    .iter()
    .any(|marker| content.contains(marker))
}

fn is_confirmation(content: &str) -> bool {
    ["继续", "确认", "执行", "确定", "同意"]
        .iter()
        .any(|marker| content.trim() == *marker || content.contains(&format!("{}执行", marker)))
}

fn normalize_task_action(value: &str) -> Result<String> {
    match value.to_uppercase().as_str() {
        "REASSIGN" | "自动交给主管重新分配" => Ok("REASSIGN".to_string()),
        "PAUSE" | "先暂停" => Ok("PAUSE".to_string()),
        "CANCEL" | "取消任务" => Ok("CANCEL".to_string()),
        other => Err(anyhow!("unsupported task handover action: {other}")),
    }
}

fn normalize_workspace_action(value: &str) -> Result<String> {
    match value.to_uppercase().as_str() {
        "ARCHIVE" | "归档保留" => Ok("ARCHIVE".to_string()),
        "READ_ONLY" | "保留为只读" => Ok("READ_ONLY".to_string()),
        "DELETE" | "删除" => Err(anyhow!(
            "workspace deletion is disabled; archive is required"
        )),
        other => Err(anyhow!("unsupported workspace action: {other}")),
    }
}
