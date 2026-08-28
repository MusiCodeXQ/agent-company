use anyhow::{anyhow, Context, Result};
use chrono::{Duration, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    agent_runtime::worker as runtime,
    app_state::AppState,
    application::organization,
    domain::{
        models::{
            A2aCommunicationAudit, A2aCommunicationDetail, A2aCommunicationMessage,
            A2aCommunicationTask, A2aFaceToFaceSession, A2aFaceToFaceSessionCreated,
            A2aFaceToFaceSessionInput, A2aSendMessageInput,
        },
        office,
    },
    infrastructure::{
        db::{self, AgentJob},
        events,
    },
    office::navigation as spatial,
};

pub const PROTOCOL_VERSION: &str = "1.0";

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub version: String,
    pub protocol_version: String,
    pub supported_interfaces: Vec<AgentInterface>,
    pub capabilities: AgentCapabilities,
    pub default_input_modes: Vec<String>,
    pub default_output_modes: Vec<String>,
    pub skills: Vec<AgentSkill>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentInterface {
    pub url: String,
    pub protocol_binding: String,
    pub protocol_version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    pub streaming: bool,
    pub push_notifications: bool,
    pub extended_agent_card: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkill {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub input_modes: Vec<String>,
    pub output_modes: Vec<String>,
}

pub async fn agent_card(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
    service_url: &str,
) -> Result<AgentCard> {
    let employee = db::employees(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|item| item.id == employee_id && item.employment_status == "ACTIVE")
        .ok_or_else(|| anyhow!("active employee was not found"))?;
    Ok(AgentCard {
        name: employee.name.clone(),
        description: format!("{} · {}", employee.title, employee.department),
        version: "1.0.0".to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
        supported_interfaces: vec![AgentInterface {
            url: service_url.to_string(),
            protocol_binding: "HTTP+JSON".to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
        }],
        capabilities: AgentCapabilities {
            streaming: false,
            push_notifications: false,
            extended_agent_card: false,
        },
        default_input_modes: vec!["text/plain".to_string()],
        default_output_modes: vec!["text/plain".to_string()],
        skills: vec![AgentSkill {
            id: format!("employee-{employee_id}-collaboration"),
            name: format!("{}协作", employee.title),
            description: format!("在受治理的面对面会话中提供{}能力", employee.title),
            tags: employee
                .skills
                .into_iter()
                .map(|skill| skill.name)
                .collect(),
            input_modes: vec!["text/plain".to_string()],
            output_modes: vec!["text/plain".to_string()],
        }],
    })
}

pub async fn create_face_to_face_session(
    state: &AppState,
    company_id: &str,
    input: A2aFaceToFaceSessionInput,
) -> Result<A2aFaceToFaceSessionCreated> {
    if input.initiator_id == input.recipient_id {
        return Err(anyhow!("face-to-face A2A requires two different employees"));
    }
    let employees = db::employees(&state.pool, company_id).await?;
    let initiator = employees
        .iter()
        .find(|item| item.id == input.initiator_id && item.employment_status == "ACTIVE")
        .ok_or_else(|| anyhow!("active initiator was not found"))?;
    let recipient = employees
        .iter()
        .find(|item| item.id == input.recipient_id && item.employment_status == "ACTIVE")
        .ok_or_else(|| anyhow!("active recipient was not found"))?;
    if initiator.floor_id != recipient.floor_id {
        return Err(anyhow!("employees must be on the same floor"));
    }
    for employee in [initiator, recipient] {
        if employee.active_movement_id.is_some() {
            return Err(anyhow!("employee {} is already moving", employee.id));
        }
    }
    if let Some(task_id) = input.company_task_id.as_deref() {
        let exists: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM tasks WHERE id = ? AND company_id = ?")
                .bind(task_id)
                .bind(company_id)
                .fetch_one(&state.pool)
                .await?;
        if exists == 0 {
            return Err(anyhow!("company task was not found"));
        }
    }

    let floor_id = initiator.floor_id.as_deref().unwrap_or("floor-01");
    let floor = organization::floor(&state.pool, company_id, floor_id).await?;
    let workstation_inputs =
        organization::workstation_layout_inputs(&state.pool, company_id, floor_id).await?;
    let layout = office::office_for_floor(&floor.id, &floor.name, &workstation_inputs);
    let seats = layout
        .locations
        .iter()
        .filter(|location| location.location_type == "MEETING_SEAT")
        .take(2)
        .collect::<Vec<_>>();
    if seats.len() != 2 {
        return Err(anyhow!(
            "two meeting seats are required for face-to-face A2A"
        ));
    }
    let room_id = seats[0]
        .room_id
        .clone()
        .filter(|room| seats[1].room_id.as_deref() == Some(room.as_str()))
        .ok_or_else(|| anyhow!("face-to-face seats must share a room"))?;

    let now = Utc::now();
    let ttl_seconds = input.ttl_seconds.unwrap_or(900).clamp(60, 7200);
    let expires_at = now + Duration::seconds(ttl_seconds);
    let session_id = format!("a2a-session-{}", Uuid::new_v4());
    let context_id = format!("a2a-context-{}", Uuid::new_v4());
    let capability_token = format!("a2a-cap-{}", Uuid::new_v4());
    let capability_token_hash = token_hash(&capability_token);
    let max_turns = input.max_turns.unwrap_or(8).clamp(1, 64);
    let token_budget = input.token_budget.unwrap_or(8_000).clamp(1, 1_000_000);
    let now_text = a2a_timestamp(now);
    let expires_at_text = a2a_timestamp(expires_at);
    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM location_reservations WHERE company_id = ? AND reserved_until <= ?")
        .bind(company_id)
        .bind(&now_text)
        .execute(&mut *tx)
        .await?;
    for (employee_id, seat) in [
        (input.initiator_id.as_str(), seats[0]),
        (input.recipient_id.as_str(), seats[1]),
    ] {
        sqlx::query(
            "INSERT INTO location_reservations
             (id, company_id, location_id, agent_id, reserved_until, reason, created_at)
             VALUES (?, ?, ?, ?, ?, 'A2A_FACE_TO_FACE', ?)",
        )
        .bind(format!("reservation-{session_id}-{}", seat.id))
        .bind(company_id)
        .bind(&seat.id)
        .bind(employee_id)
        .bind(&expires_at_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .context("face-to-face seats are already reserved")?;
    }
    sqlx::query(
        "INSERT INTO a2a_face_to_face_sessions
         (id, company_id, context_id, initiator_id, recipient_id, initiator_location_id,
          recipient_location_id, room_id, status, capability_token_hash, max_turns,
          token_budget, company_task_id, expires_at, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'GATHERING', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&session_id)
    .bind(company_id)
    .bind(&context_id)
    .bind(&input.initiator_id)
    .bind(&input.recipient_id)
    .bind(&seats[0].id)
    .bind(&seats[1].id)
    .bind(&room_id)
    .bind(&capability_token_hash)
    .bind(max_turns)
    .bind(token_budget)
    .bind(&input.company_task_id)
    .bind(&expires_at_text)
    .bind(&now_text)
    .execute(&mut *tx)
    .await?;
    audit_tx(
        &mut tx,
        company_id,
        Some(&session_id),
        None,
        Some(&input.initiator_id),
        "session.create",
        "ALLOWED",
        &json!({"recipientId": input.recipient_id, "roomId": room_id}),
    )
    .await?;
    tx.commit().await?;

    let first_move = spatial::request_move(
        state,
        company_id,
        &input.initiator_id,
        &seats[0].id,
        "A2A_FACE_TO_FACE",
    )
    .await;
    let second_move = spatial::request_move(
        state,
        company_id,
        &input.recipient_id,
        &seats[1].id,
        "A2A_FACE_TO_FACE",
    )
    .await;
    if let Err(error) = first_move.and(second_move) {
        close_session(
            state,
            company_id,
            &session_id,
            &format!("movement failed: {error}"),
        )
        .await?;
        return Err(error);
    }
    let session = get_session(&state.pool, company_id, &session_id).await?;
    events::emit(
        state,
        company_id,
        "a2a.face_to_face.gathering",
        Some(&input.initiator_id),
        Some(&session_id),
        json!({"session": session}),
    )
    .await?;
    Ok(A2aFaceToFaceSessionCreated {
        session,
        capability_token,
    })
}

pub async fn on_agent_arrived(state: &AppState, arrival: &db::MovementArrival) -> Result<()> {
    let rows = sqlx::query(
        "SELECT id FROM a2a_face_to_face_sessions
         WHERE company_id = ? AND status = 'GATHERING'
           AND (? = initiator_location_id OR ? = recipient_location_id)",
    )
    .bind(&arrival.movement.company_id)
    .bind(&arrival.to_location_id)
    .bind(&arrival.to_location_id)
    .fetch_all(&state.pool)
    .await?;
    for row in rows {
        let session_id: String = row.try_get("id")?;
        let session = get_session(&state.pool, &arrival.movement.company_id, &session_id).await?;
        if participants_are_present(&state.pool, &session).await? {
            let now = a2a_now();
            let changed = sqlx::query(
                "UPDATE a2a_face_to_face_sessions SET status = 'ACTIVE', activated_at = ?
                 WHERE id = ? AND status = 'GATHERING' AND expires_at > ?",
            )
            .bind(&now)
            .bind(&session_id)
            .bind(&now)
            .execute(&state.pool)
            .await?
            .rows_affected();
            if changed > 0 {
                events::transition(
                    state,
                    &session.company_id,
                    &session.initiator_id,
                    "MEETING",
                    session.company_task_id.as_deref(),
                    Some(&session.id),
                )
                .await?;
                events::transition(
                    state,
                    &session.company_id,
                    &session.recipient_id,
                    "MEETING",
                    session.company_task_id.as_deref(),
                    Some(&session.id),
                )
                .await?;
                events::emit(
                    state,
                    &session.company_id,
                    "a2a.face_to_face.activated",
                    None,
                    Some(&session.id),
                    json!({"sessionId": session.id, "contextId": session.context_id}),
                )
                .await?;
            }
        }
    }
    Ok(())
}

pub async fn send_message(
    state: &AppState,
    company_id: &str,
    recipient_id: &str,
    sender_id: &str,
    capability_token: &str,
    input: A2aSendMessageInput,
) -> Result<Value> {
    if input.message.role != "ROLE_USER" && input.message.role != "user" {
        return Err(anyhow!("A2A client messages must use ROLE_USER"));
    }
    let context_id = input
        .message
        .context_id
        .as_deref()
        .ok_or_else(|| anyhow!("contextId is required for governed face-to-face A2A"))?;
    let content = input
        .message
        .parts
        .iter()
        .filter_map(|part| part.text.as_deref())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    if input.message.message_id.trim().is_empty() || content.is_empty() {
        return Err(anyhow!("messageId and at least one text part are required"));
    }

    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        "SELECT id, initiator_id, recipient_id, initiator_location_id, recipient_location_id,
                room_id, status, capability_token_hash, max_turns, turns_used, token_budget,
                tokens_used, company_task_id, expires_at, created_at, activated_at,
                closed_at, close_reason
         FROM a2a_face_to_face_sessions WHERE company_id = ? AND context_id = ?",
    )
    .bind(company_id)
    .bind(context_id)
    .fetch_one(&mut *tx)
    .await?;
    let session = session_from_row(company_id, context_id, &row)?;
    let deny_reason =
        validate_send(&mut tx, &session, sender_id, recipient_id, capability_token).await?;
    if let Some(reason) = deny_reason {
        audit_tx(
            &mut tx,
            company_id,
            Some(&session.id),
            input.message.task_id.as_deref(),
            Some(sender_id),
            "message.send",
            "DENIED",
            &json!({"reason": reason}),
        )
        .await?;
        tx.commit().await?;
        return Err(anyhow!(reason));
    }
    if let Some(existing) =
        sqlx::query("SELECT a2a_task_id FROM a2a_messages WHERE id = ? AND company_id = ?")
            .bind(&input.message.message_id)
            .bind(company_id)
            .fetch_optional(&mut *tx)
            .await?
    {
        let task_id: Option<String> = existing.try_get("a2a_task_id")?;
        tx.commit().await?;
        let task_id = task_id.ok_or_else(|| anyhow!("existing message has no A2A task"))?;
        return get_task(&state.pool, company_id, &task_id).await;
    }

    let task_id = format!("a2a-task-{}", Uuid::new_v4());
    let job_id = format!("job-{}", Uuid::new_v4());
    let company_message_id = format!("msg-{}", Uuid::new_v4());
    let now = a2a_now();
    let parts_json = serde_json::to_string(&input.message.parts)?;
    let metadata_json = serde_json::to_string(&input.message.metadata)?;
    sqlx::query(
        "INSERT INTO messages
         (id, company_id, from_type, from_id, to_type, to_id, content, created_at, job_id)
         VALUES (?, ?, 'employee', ?, 'employee', ?, ?, ?, ?)",
    )
    .bind(&company_message_id)
    .bind(company_id)
    .bind(sender_id)
    .bind(recipient_id)
    .bind(&content)
    .bind(&now)
    .bind(&job_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO agent_jobs
         (id, company_id, job_type, status, employee_id, message_id, task_id, payload,
          attempts, available_at, created_at, updated_at)
         VALUES (?, ?, 'A2A_RESPONSE', 'PENDING', ?, ?, ?, ?, 0, ?, ?, ?)",
    )
    .bind(&job_id)
    .bind(company_id)
    .bind(recipient_id)
    .bind(&company_message_id)
    .bind(&session.company_task_id)
    .bind(
        json!({
            "content": content,
            "a2aTaskId": task_id,
            "contextId": context_id,
            "sessionId": session.id,
            "senderId": sender_id,
            "recipientId": recipient_id,
        })
        .to_string(),
    )
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO a2a_tasks
         (id, company_id, context_id, session_id, sender_id, recipient_id, company_task_id,
          agent_job_id, state, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'TASK_STATE_SUBMITTED', ?, ?)",
    )
    .bind(&task_id)
    .bind(company_id)
    .bind(context_id)
    .bind(&session.id)
    .bind(sender_id)
    .bind(recipient_id)
    .bind(&session.company_task_id)
    .bind(&job_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO a2a_messages
         (id, company_id, session_id, context_id, a2a_task_id, company_message_id,
          sender_id, recipient_id, role, parts, metadata, direction, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'ROLE_USER', ?, ?, 'REQUEST', ?)",
    )
    .bind(&input.message.message_id)
    .bind(company_id)
    .bind(&session.id)
    .bind(context_id)
    .bind(&task_id)
    .bind(&company_message_id)
    .bind(sender_id)
    .bind(recipient_id)
    .bind(&parts_json)
    .bind(&metadata_json)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE a2a_face_to_face_sessions SET turns_used = turns_used + 1 WHERE id = ?")
        .bind(&session.id)
        .execute(&mut *tx)
        .await?;
    audit_tx(
        &mut tx,
        company_id,
        Some(&session.id),
        Some(&task_id),
        Some(sender_id),
        "message.send",
        "ALLOWED",
        &json!({"recipientId": recipient_id, "messageId": input.message.message_id}),
    )
    .await?;
    tx.commit().await?;
    events::emit(
        state,
        company_id,
        "a2a.message.sent",
        Some(sender_id),
        Some(&task_id),
        json!({"taskId": task_id, "contextId": context_id, "recipientId": recipient_id}),
    )
    .await?;
    state.worker_notify.notify_one();
    get_task(&state.pool, company_id, &task_id).await
}

pub async fn process_job(state: &AppState, job: &AgentJob) -> Result<()> {
    let payload: Value = serde_json::from_str(&job.payload)?;
    let task_id = required_payload(&payload, "a2aTaskId")?;
    let context_id = required_payload(&payload, "contextId")?;
    let session_id = required_payload(&payload, "sessionId")?;
    let sender_id = required_payload(&payload, "senderId")?;
    let recipient_id = required_payload(&payload, "recipientId")?;
    let session = get_session(&state.pool, &job.company_id, session_id).await?;
    if session.status != "ACTIVE" || !participants_are_present(&state.pool, &session).await? {
        fail_task(state, job, task_id, "face-to-face presence was lost").await?;
        return Ok(());
    }
    sqlx::query("UPDATE a2a_tasks SET state = 'TASK_STATE_WORKING', updated_at = ? WHERE id = ?")
        .bind(a2a_now())
        .bind(task_id)
        .execute(&state.pool)
        .await?;
    let before = db::employee_token_usage(&state.pool, &job.company_id, recipient_id).await?;
    let reply = match runtime::generate_reply(state, job).await {
        Ok(reply) => reply,
        Err(error) => {
            fail_task(state, job, task_id, &error.to_string()).await?;
            return Ok(());
        }
    };
    if db::job_is_cancelled(&state.pool, &job.id).await? {
        return Ok(());
    }
    let after = db::employee_token_usage(&state.pool, &job.company_id, recipient_id).await?;
    let used = (after.total_tokens - before.total_tokens).max(0);
    let response_message_id = format!("a2a-msg-{}", Uuid::new_v4());
    let company_message_id = format!("msg-{}", Uuid::new_v4());
    let now = a2a_now();
    let parts = json!([{"text": reply}]);
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO messages
         (id, company_id, from_type, from_id, to_type, to_id, content, created_at, job_id)
         VALUES (?, ?, 'employee', ?, 'employee', ?, ?, ?, ?)",
    )
    .bind(&company_message_id)
    .bind(&job.company_id)
    .bind(recipient_id)
    .bind(sender_id)
    .bind(&reply)
    .bind(&now)
    .bind(&job.id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO a2a_messages
         (id, company_id, session_id, context_id, a2a_task_id, company_message_id,
          sender_id, recipient_id, role, parts, metadata, direction, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'ROLE_AGENT', ?, '{}', 'RESPONSE', ?)",
    )
    .bind(&response_message_id)
    .bind(&job.company_id)
    .bind(session_id)
    .bind(context_id)
    .bind(task_id)
    .bind(&company_message_id)
    .bind(recipient_id)
    .bind(sender_id)
    .bind(parts.to_string())
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE a2a_tasks SET state = 'TASK_STATE_COMPLETED', updated_at = ?, completed_at = ?
         WHERE id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(task_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE a2a_face_to_face_sessions SET tokens_used = tokens_used + ? WHERE id = ?")
        .bind(used)
        .bind(session_id)
        .execute(&mut *tx)
        .await?;
    audit_tx(
        &mut tx,
        &job.company_id,
        Some(session_id),
        Some(task_id),
        Some(recipient_id),
        "task.complete",
        "ALLOWED",
        &json!({"tokensUsed": used, "messageId": response_message_id}),
    )
    .await?;
    tx.commit().await?;
    db::finish_job(&state.pool, &job.id, "COMPLETED", Some(&reply), None).await?;
    events::emit(
        state,
        &job.company_id,
        "a2a.task.completed",
        Some(recipient_id),
        Some(task_id),
        json!({"taskId": task_id, "contextId": context_id, "messageId": response_message_id}),
    )
    .await?;
    Ok(())
}

pub async fn get_task(pool: &SqlitePool, company_id: &str, task_id: &str) -> Result<Value> {
    let row = sqlx::query(
        "SELECT id, context_id, state, created_at, updated_at, error
         FROM a2a_tasks WHERE id = ? AND company_id = ?",
    )
    .bind(task_id)
    .bind(company_id)
    .fetch_one(pool)
    .await?;
    let messages = sqlx::query(
        "SELECT id, role, parts, created_at FROM a2a_messages
         WHERE a2a_task_id = ? AND company_id = ? ORDER BY created_at",
    )
    .bind(task_id)
    .bind(company_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|message| {
        let parts: String = message.try_get("parts")?;
        Ok(json!({
            "messageId": message.try_get::<String, _>("id")?,
            "role": message.try_get::<String, _>("role")?,
            "parts": serde_json::from_str::<Value>(&parts).unwrap_or(Value::Array(Vec::new())),
            "taskId": task_id,
            "contextId": row.try_get::<String, _>("context_id")?,
        }))
    })
    .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let artifacts = messages
        .iter()
        .rev()
        .find(|message| message["role"] == "ROLE_AGENT")
        .map(|message| {
            vec![json!({
                "artifactId": format!("artifact-{task_id}"),
                "name": "face-to-face-response",
                "parts": message["parts"].clone(),
            })]
        })
        .unwrap_or_default();
    Ok(json!({"task": {
        "id": row.try_get::<String, _>("id")?,
        "contextId": row.try_get::<String, _>("context_id")?,
        "status": {
            "state": row.try_get::<String, _>("state")?,
            "timestamp": row.try_get::<String, _>("updated_at")?,
            "error": row.try_get::<Option<String>, _>("error")?,
        },
        "history": messages,
        "artifacts": artifacts,
    }}))
}

pub async fn cancel_task(
    state: &AppState,
    company_id: &str,
    task_id: &str,
    actor_id: &str,
    capability_token: &str,
) -> Result<Value> {
    let mut tx = state.pool.begin().await?;
    let row = sqlx::query(
        "SELECT t.session_id, t.agent_job_id, t.state, s.initiator_id, s.recipient_id,
                s.capability_token_hash
         FROM a2a_tasks t
         INNER JOIN a2a_face_to_face_sessions s ON s.id = t.session_id
         WHERE t.id = ? AND t.company_id = ?",
    )
    .bind(task_id)
    .bind(company_id)
    .fetch_one(&mut *tx)
    .await?;
    let state_value: String = row.try_get("state")?;
    let initiator_id: String = row.try_get("initiator_id")?;
    let recipient_id: String = row.try_get("recipient_id")?;
    let stored_hash: String = row.try_get("capability_token_hash")?;
    if (actor_id != initiator_id && actor_id != recipient_id)
        || token_hash(capability_token) != stored_hash
    {
        return Err(anyhow!("not authorized to cancel this A2A task"));
    }
    if matches!(
        state_value.as_str(),
        "TASK_STATE_COMPLETED"
            | "TASK_STATE_FAILED"
            | "TASK_STATE_CANCELED"
            | "TASK_STATE_REJECTED"
    ) {
        return Err(anyhow!("task is already terminal"));
    }
    let job_id: Option<String> = row.try_get("agent_job_id")?;
    sqlx::query(
        "UPDATE a2a_tasks SET state = 'TASK_STATE_CANCELED', updated_at = ?, completed_at = ?
         WHERE id = ?",
    )
    .bind(a2a_now())
    .bind(a2a_now())
    .bind(task_id)
    .execute(&mut *tx)
    .await?;
    if let Some(job_id) = job_id {
        sqlx::query("UPDATE agent_jobs SET status = 'CANCELLED', updated_at = ? WHERE id = ?")
            .bind(a2a_now())
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
    }
    let session_id: String = row.try_get("session_id")?;
    audit_tx(
        &mut tx,
        company_id,
        Some(&session_id),
        Some(task_id),
        Some(actor_id),
        "task.cancel",
        "ALLOWED",
        &json!({}),
    )
    .await?;
    tx.commit().await?;
    get_task(&state.pool, company_id, task_id).await
}

pub async fn authorize_task(
    pool: &SqlitePool,
    company_id: &str,
    task_id: &str,
    actor_id: &str,
    capability_token: &str,
) -> Result<()> {
    let row = sqlx::query(
        "SELECT s.initiator_id, s.recipient_id, s.capability_token_hash
         FROM a2a_tasks t
         INNER JOIN a2a_face_to_face_sessions s ON s.id = t.session_id
         WHERE t.id = ? AND t.company_id = ?",
    )
    .bind(task_id)
    .bind(company_id)
    .fetch_one(pool)
    .await?;
    let initiator_id: String = row.try_get("initiator_id")?;
    let recipient_id: String = row.try_get("recipient_id")?;
    let stored_hash: String = row.try_get("capability_token_hash")?;
    if (actor_id != initiator_id && actor_id != recipient_id)
        || token_hash(capability_token) != stored_hash
    {
        return Err(anyhow!("not authorized to read this A2A task"));
    }
    Ok(())
}

pub async fn close_sessions_for_departing_agent(
    state: &AppState,
    company_id: &str,
    agent_id: &str,
    destination_location_id: &str,
) -> Result<()> {
    let rows = sqlx::query(
        "SELECT id, initiator_id, initiator_location_id, recipient_location_id
         FROM a2a_face_to_face_sessions
         WHERE company_id = ? AND status = 'ACTIVE' AND (initiator_id = ? OR recipient_id = ?)",
    )
    .bind(company_id)
    .bind(agent_id)
    .bind(agent_id)
    .fetch_all(&state.pool)
    .await?;
    for row in rows {
        let initiator_id: String = row.try_get("initiator_id")?;
        let assigned_location: String = if initiator_id == agent_id {
            row.try_get("initiator_location_id")?
        } else {
            row.try_get("recipient_location_id")?
        };
        if destination_location_id != assigned_location {
            let session_id: String = row.try_get("id")?;
            close_session(
                state,
                company_id,
                &session_id,
                "participant left the face-to-face position",
            )
            .await?;
        }
    }
    Ok(())
}

pub async fn close_session(
    state: &AppState,
    company_id: &str,
    session_id: &str,
    reason: &str,
) -> Result<A2aFaceToFaceSession> {
    let session = get_session(&state.pool, company_id, session_id).await?;
    let now = a2a_now();
    let mut tx = state.pool.begin().await?;
    sqlx::query(
        "UPDATE a2a_face_to_face_sessions SET status = 'CLOSED', closed_at = ?, close_reason = ?
         WHERE id = ? AND company_id = ? AND status IN ('GATHERING', 'ACTIVE')",
    )
    .bind(&now)
    .bind(reason)
    .bind(session_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "DELETE FROM location_reservations WHERE company_id = ? AND agent_id IN (?, ?)
         AND reason = 'A2A_FACE_TO_FACE'",
    )
    .bind(company_id)
    .bind(&session.initiator_id)
    .bind(&session.recipient_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE agent_movements SET movement_status = 'CANCELLED', completed_at = ?
         WHERE company_id = ? AND movement_status IN ('PENDING', 'MOVING')
           AND ((agent_id = ? AND to_location_id = ?) OR
                (agent_id = ? AND to_location_id = ?))",
    )
    .bind(&now)
    .bind(company_id)
    .bind(&session.initiator_id)
    .bind(&session.initiator_location_id)
    .bind(&session.recipient_id)
    .bind(&session.recipient_location_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE employees SET active_movement_id = NULL, movement = 'STATIONARY',
         current_destination = NULL, posture = 'STANDING'
         WHERE company_id = ? AND id IN (?, ?) AND active_movement_id IN (
           SELECT id FROM agent_movements WHERE company_id = ? AND movement_status = 'CANCELLED'
         )",
    )
    .bind(company_id)
    .bind(&session.initiator_id)
    .bind(&session.recipient_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await?;
    audit_tx(
        &mut tx,
        company_id,
        Some(session_id),
        None,
        None,
        "session.close",
        "ALLOWED",
        &json!({"reason": reason}),
    )
    .await?;
    tx.commit().await?;
    events::emit(
        state,
        company_id,
        "a2a.face_to_face.closed",
        None,
        Some(session_id),
        json!({"sessionId": session_id, "reason": reason}),
    )
    .await?;
    get_session(&state.pool, company_id, session_id).await
}

async fn validate_send(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session: &A2aFaceToFaceSession,
    sender_id: &str,
    recipient_id: &str,
    capability_token: &str,
) -> Result<Option<String>> {
    let expected_sender = if session.turns_used % 2 == 0 {
        &session.initiator_id
    } else {
        &session.recipient_id
    };
    let expected_recipient = if expected_sender == &session.initiator_id {
        &session.recipient_id
    } else {
        &session.initiator_id
    };
    let stored_hash: String = sqlx::query_scalar(
        "SELECT capability_token_hash FROM a2a_face_to_face_sessions WHERE id = ?",
    )
    .bind(&session.id)
    .fetch_one(&mut **tx)
    .await?;
    let presence_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM employees
         WHERE company_id = ? AND active_movement_id IS NULL AND
           ((id = ? AND location_id = ? AND room_id = ?) OR
            (id = ? AND location_id = ? AND room_id = ?))",
    )
    .bind(&session.company_id)
    .bind(&session.initiator_id)
    .bind(&session.initiator_location_id)
    .bind(&session.room_id)
    .bind(&session.recipient_id)
    .bind(&session.recipient_location_id)
    .bind(&session.room_id)
    .fetch_one(&mut **tx)
    .await?;
    let active_tasks: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM a2a_tasks WHERE session_id = ?
         AND state IN ('TASK_STATE_SUBMITTED', 'TASK_STATE_WORKING',
                       'TASK_STATE_INPUT_REQUIRED', 'TASK_STATE_AUTH_REQUIRED')",
    )
    .bind(&session.id)
    .fetch_one(&mut **tx)
    .await?;
    let reason = if token_hash(capability_token) != stored_hash {
        Some("invalid A2A face-to-face capability token")
    } else if session.status != "ACTIVE" {
        Some("face-to-face session is not active")
    } else if session.expires_at <= a2a_now() {
        Some("face-to-face session has expired")
    } else if presence_count != 2 {
        Some("both animated employees must still be at their assigned face-to-face seats")
    } else if sender_id != expected_sender || recipient_id != expected_recipient {
        Some("sender is not allowed in the current A2A turn")
    } else if active_tasks != 0 {
        Some("the previous A2A turn has not reached a terminal state")
    } else if session.turns_used >= session.max_turns {
        Some("face-to-face A2A turn limit reached")
    } else if session.tokens_used >= session.token_budget {
        Some("face-to-face A2A token budget reached")
    } else {
        None
    };
    Ok(reason.map(str::to_string))
}

async fn participants_are_present(
    pool: &SqlitePool,
    session: &A2aFaceToFaceSession,
) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM employees
         WHERE company_id = ? AND active_movement_id IS NULL AND
           ((id = ? AND location_id = ? AND room_id = ?) OR
            (id = ? AND location_id = ? AND room_id = ?))",
    )
    .bind(&session.company_id)
    .bind(&session.initiator_id)
    .bind(&session.initiator_location_id)
    .bind(&session.room_id)
    .bind(&session.recipient_id)
    .bind(&session.recipient_location_id)
    .bind(&session.room_id)
    .fetch_one(pool)
    .await?;
    Ok(count == 2)
}

async fn get_session(
    pool: &SqlitePool,
    company_id: &str,
    session_id: &str,
) -> Result<A2aFaceToFaceSession> {
    let row = sqlx::query(
        "SELECT context_id, initiator_id, recipient_id, initiator_location_id,
                recipient_location_id, room_id, status, max_turns, turns_used, token_budget,
                tokens_used, company_task_id, expires_at, created_at, activated_at,
                closed_at, close_reason
         FROM a2a_face_to_face_sessions WHERE id = ? AND company_id = ?",
    )
    .bind(session_id)
    .bind(company_id)
    .fetch_one(pool)
    .await?;
    Ok(
        session_from_row(company_id, &row.try_get::<String, _>("context_id")?, &row).map(
            |mut item| {
                item.id = session_id.to_string();
                item
            },
        )?,
    )
}

pub async fn session(
    pool: &SqlitePool,
    company_id: &str,
    session_id: &str,
) -> Result<A2aFaceToFaceSession> {
    get_session(pool, company_id, session_id).await
}

pub async fn list_communications(
    pool: &SqlitePool,
    company_id: &str,
    limit: u32,
    before: Option<&str>,
) -> Result<Vec<A2aFaceToFaceSession>> {
    let limit = i64::from(limit.clamp(1, 100));
    let rows = match before {
        Some(before) => {
            sqlx::query(
                "SELECT id, context_id, initiator_id, recipient_id, initiator_location_id,
                        recipient_location_id, room_id, status, max_turns, turns_used,
                        token_budget, tokens_used, company_task_id, expires_at, created_at,
                        activated_at, closed_at, close_reason
                 FROM a2a_face_to_face_sessions
                 WHERE company_id = ? AND created_at < ?
                 ORDER BY created_at DESC, id DESC LIMIT ?",
            )
            .bind(company_id)
            .bind(before)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                "SELECT id, context_id, initiator_id, recipient_id, initiator_location_id,
                        recipient_location_id, room_id, status, max_turns, turns_used,
                        token_budget, tokens_used, company_task_id, expires_at, created_at,
                        activated_at, closed_at, close_reason
                 FROM a2a_face_to_face_sessions
                 WHERE company_id = ?
                 ORDER BY created_at DESC, id DESC LIMIT ?",
            )
            .bind(company_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };
    rows.into_iter()
        .map(|row| {
            let context_id: String = row.try_get("context_id")?;
            session_from_row(company_id, &context_id, &row)
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()
        .map_err(Into::into)
}

pub async fn communication_detail(
    pool: &SqlitePool,
    company_id: &str,
    session_id: &str,
) -> Result<A2aCommunicationDetail> {
    let session = get_session(pool, company_id, session_id).await?;
    let message_rows = sqlx::query(
        "SELECT id, session_id, context_id, a2a_task_id, sender_id, recipient_id,
                role, parts, metadata, direction, created_at
         FROM a2a_messages
         WHERE company_id = ? AND session_id = ?
         ORDER BY created_at ASC, id ASC",
    )
    .bind(company_id)
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let messages = message_rows
        .into_iter()
        .map(|row| {
            let parts: String = row.try_get("parts")?;
            let metadata: String = row.try_get("metadata")?;
            Ok(A2aCommunicationMessage {
                id: row.try_get("id")?,
                session_id: row.try_get("session_id")?,
                context_id: row.try_get("context_id")?,
                task_id: row.try_get("a2a_task_id")?,
                sender_id: row.try_get("sender_id")?,
                recipient_id: row.try_get("recipient_id")?,
                role: row.try_get("role")?,
                parts: serde_json::from_str(&parts).unwrap_or(Value::Array(Vec::new())),
                metadata: serde_json::from_str(&metadata)
                    .unwrap_or(Value::Object(serde_json::Map::new())),
                direction: row.try_get("direction")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let task_rows = sqlx::query(
        "SELECT id, context_id, session_id, sender_id, recipient_id, company_task_id,
                agent_job_id, state, created_at, updated_at, completed_at, error
         FROM a2a_tasks
         WHERE company_id = ? AND session_id = ?
         ORDER BY created_at ASC, id ASC",
    )
    .bind(company_id)
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let tasks = task_rows
        .into_iter()
        .map(|row| {
            Ok(A2aCommunicationTask {
                id: row.try_get("id")?,
                context_id: row.try_get("context_id")?,
                session_id: row.try_get("session_id")?,
                sender_id: row.try_get("sender_id")?,
                recipient_id: row.try_get("recipient_id")?,
                company_task_id: row.try_get("company_task_id")?,
                agent_job_id: row.try_get("agent_job_id")?,
                state: row.try_get("state")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
                completed_at: row.try_get("completed_at")?,
                error: row.try_get("error")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    let audit_rows = sqlx::query(
        "SELECT id, session_id, task_id, actor_id, action, outcome, details, created_at
         FROM a2a_audit_log
         WHERE company_id = ? AND session_id = ?
         ORDER BY created_at ASC, id ASC",
    )
    .bind(company_id)
    .bind(session_id)
    .fetch_all(pool)
    .await?;
    let audit = audit_rows
        .into_iter()
        .map(|row| {
            let details: String = row.try_get("details")?;
            Ok(A2aCommunicationAudit {
                id: row.try_get("id")?,
                session_id: row.try_get("session_id")?,
                task_id: row.try_get("task_id")?,
                actor_id: row.try_get("actor_id")?,
                action: row.try_get("action")?,
                outcome: row.try_get("outcome")?,
                details: serde_json::from_str(&details)
                    .unwrap_or(Value::Object(serde_json::Map::new())),
                created_at: row.try_get("created_at")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    Ok(A2aCommunicationDetail {
        session,
        messages,
        tasks,
        audit,
    })
}

fn session_from_row(
    company_id: &str,
    context_id: &str,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<A2aFaceToFaceSession, sqlx::Error> {
    Ok(A2aFaceToFaceSession {
        id: row.try_get("id").unwrap_or_default(),
        company_id: company_id.to_string(),
        context_id: context_id.to_string(),
        initiator_id: row.try_get("initiator_id")?,
        recipient_id: row.try_get("recipient_id")?,
        initiator_location_id: row.try_get("initiator_location_id")?,
        recipient_location_id: row.try_get("recipient_location_id")?,
        room_id: row.try_get("room_id")?,
        status: row.try_get("status")?,
        max_turns: row.try_get("max_turns")?,
        turns_used: row.try_get("turns_used")?,
        token_budget: row.try_get("token_budget")?,
        tokens_used: row.try_get("tokens_used")?,
        company_task_id: row.try_get("company_task_id")?,
        expires_at: row.try_get("expires_at")?,
        created_at: row.try_get("created_at")?,
        activated_at: row.try_get("activated_at")?,
        closed_at: row.try_get("closed_at")?,
        close_reason: row.try_get("close_reason")?,
    })
}

async fn fail_task(state: &AppState, job: &AgentJob, task_id: &str, error: &str) -> Result<()> {
    let now = a2a_now();
    sqlx::query(
        "UPDATE a2a_tasks SET state = 'TASK_STATE_FAILED', error = ?, updated_at = ?,
         completed_at = ? WHERE id = ?",
    )
    .bind(error)
    .bind(&now)
    .bind(&now)
    .bind(task_id)
    .execute(&state.pool)
    .await?;
    db::finish_job(&state.pool, &job.id, "FAILED", None, Some(error)).await?;
    events::emit(
        state,
        &job.company_id,
        "a2a.task.failed",
        Some(&job.employee_id),
        Some(task_id),
        json!({"taskId": task_id, "error": error}),
    )
    .await?;
    Ok(())
}

fn required_payload<'a>(payload: &'a Value, key: &str) -> Result<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("A2A job payload is missing {key}"))
}

fn token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

fn a2a_now() -> String {
    a2a_timestamp(Utc::now())
}

fn a2a_timestamp(timestamp: chrono::DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

#[allow(clippy::too_many_arguments)]
async fn audit_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    company_id: &str,
    session_id: Option<&str>,
    task_id: Option<&str>,
    actor_id: Option<&str>,
    action: &str,
    outcome: &str,
    details: &Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO a2a_audit_log
         (id, company_id, session_id, task_id, actor_id, action, outcome, details, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(format!("a2a-audit-{}", Uuid::new_v4()))
    .bind(company_id)
    .bind(session_id)
    .bind(task_id)
    .bind(actor_id)
    .bind(action)
    .bind(outcome)
    .bind(details.to_string())
    .bind(a2a_now())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::token_hash;

    #[test]
    fn capability_tokens_are_hashed_before_storage() {
        let token = "a2a-cap-secret";
        assert_ne!(token_hash(token), token);
        assert_eq!(token_hash(token), token_hash(token));
    }
}
