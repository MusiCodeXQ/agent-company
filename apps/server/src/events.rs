use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use crate::{app_state::AppState, models::DomainEvent};

#[derive(Debug, FromRow)]
struct EventRow {
    event_id: String,
    company_id: String,
    event_type: String,
    timestamp: String,
    actor_id: Option<String>,
    correlation_id: Option<String>,
    payload: String,
}

pub async fn emit(
    state: &AppState,
    company_id: &str,
    event_type: &str,
    actor_id: Option<&str>,
    correlation_id: Option<&str>,
    payload: Value,
) -> Result<DomainEvent, sqlx::Error> {
    let event = DomainEvent {
        event_id: format!("evt-{}", Uuid::new_v4()),
        company_id: company_id.to_string(),
        event_type: event_type.to_string(),
        timestamp: Utc::now().to_rfc3339(),
        actor_id: actor_id.map(str::to_string),
        correlation_id: correlation_id.map(str::to_string),
        payload,
    };
    sqlx::query(
        "INSERT INTO events (event_id, company_id, type, timestamp, actor_id, correlation_id, payload)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&event.event_id)
    .bind(&event.company_id)
    .bind(&event.event_type)
    .bind(&event.timestamp)
    .bind(&event.actor_id)
    .bind(&event.correlation_id)
    .bind(serde_json::to_string(&event.payload).expect("event payload is serializable"))
    .execute(&state.pool)
    .await?;

    let _ = state.event_tx.send(event.clone());
    Ok(event)
}

pub async fn recent(pool: &SqlitePool, company_id: &str) -> Result<Vec<DomainEvent>, sqlx::Error> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT event_id, company_id, type AS event_type, timestamp, actor_id, correlation_id, payload
         FROM events WHERE company_id = ? ORDER BY timestamp DESC LIMIT 100",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;

    let mut events = rows.into_iter().map(event_from_row).collect::<Vec<_>>();
    events.reverse();
    Ok(events)
}

pub async fn after(
    pool: &SqlitePool,
    company_id: &str,
    after_id: Option<&str>,
) -> Result<Vec<DomainEvent>, sqlx::Error> {
    let events = recent(pool, company_id).await?;
    let Some(after_id) = after_id else {
        return Ok(events);
    };
    let Some(index) = events.iter().position(|event| event.event_id == after_id) else {
        return Ok(events);
    };
    Ok(events.into_iter().skip(index + 1).collect())
}

pub async fn transition(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
    to: &str,
    task_id: Option<&str>,
    correlation_id: Option<&str>,
) -> Result<DomainEvent, sqlx::Error> {
    let from = sqlx::query_scalar::<_, String>(
        "SELECT current_state FROM employees WHERE id = ? AND company_id = ?",
    )
    .bind(employee_id)
    .bind(company_id)
    .fetch_one(&state.pool)
    .await?;
    sqlx::query("UPDATE employees SET current_state = ? WHERE id = ? AND company_id = ?")
        .bind(to)
        .bind(employee_id)
        .bind(company_id)
        .execute(&state.pool)
        .await?;
    emit(
        state,
        company_id,
        "employee.state.changed",
        Some(employee_id),
        correlation_id,
        json!({ "from": from, "to": to, "taskId": task_id }),
    )
    .await
}

fn event_from_row(row: EventRow) -> DomainEvent {
    DomainEvent {
        event_id: row.event_id,
        company_id: row.company_id,
        event_type: row.event_type,
        timestamp: row.timestamp,
        actor_id: row.actor_id,
        correlation_id: row.correlation_id,
        payload: serde_json::from_str(&row.payload).unwrap_or(Value::Null),
    }
}
