use std::collections::{HashMap, HashSet};

use anyhow::{anyhow, Result};
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

use crate::{
    app_state::AppState,
    db, events,
    models::{MeetingInput, MeetingSummary},
    office, spatial,
};

pub async fn create(
    state: &AppState,
    company_id: &str,
    input: MeetingInput,
) -> Result<MeetingSummary> {
    let participant_ids = input
        .participant_ids
        .into_iter()
        .filter(|id| !id.trim().is_empty())
        .collect::<Vec<_>>();
    let mut unique_participants = Vec::new();
    let mut seen = HashSet::new();
    for participant_id in participant_ids {
        if seen.insert(participant_id.clone()) {
            unique_participants.push(participant_id);
        }
    }
    if unique_participants.is_empty() {
        return Err(anyhow!("a meeting needs at least one participant"));
    }

    let layout = office::default_office();
    let seat_ids = layout
        .locations
        .iter()
        .filter(|location| location.location_type == "MEETING_SEAT")
        .map(|location| location.id.clone())
        .collect::<Vec<_>>();
    if unique_participants.len() > seat_ids.len() {
        return Err(anyhow!("not enough meeting seats"));
    }

    let mut tx = state.pool.begin().await?;
    let now = Utc::now();
    let now_text = now.to_rfc3339();
    let active_meeting = sqlx::query(
        "SELECT id FROM meetings WHERE company_id = ?
         AND status IN ('CREATED', 'WAITING_FOR_PARTICIPANTS', 'IN_PROGRESS')
         LIMIT 1",
    )
    .bind(company_id)
    .fetch_optional(&mut *tx)
    .await?;
    if active_meeting.is_some() {
        return Err(anyhow!("another meeting is already active"));
    }
    sqlx::query("DELETE FROM location_reservations WHERE company_id = ? AND reserved_until <= ?")
        .bind(company_id)
        .bind(&now_text)
        .execute(&mut *tx)
        .await?;
    for employee_id in &unique_participants {
        sqlx::query("SELECT id FROM employees WHERE company_id = ? AND id = ?")
            .bind(company_id)
            .bind(employee_id)
            .fetch_one(&mut *tx)
            .await?;
    }
    let participant_set = unique_participants.iter().collect::<HashSet<_>>();
    let mut assigned_seats = HashMap::<String, String>::new();
    let mut unreserved_seats = Vec::new();
    for seat_id in seat_ids {
        let reserved_agent = sqlx::query(
            "SELECT agent_id FROM location_reservations
             WHERE company_id = ? AND location_id = ?",
        )
        .bind(company_id)
        .bind(&seat_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| row.try_get::<String, _>("agent_id"))
        .transpose()?;
        if let Some(agent_id) = reserved_agent {
            if participant_set.contains(&agent_id) {
                assigned_seats.insert(agent_id, seat_id);
            }
        } else {
            unreserved_seats.push(seat_id);
        }
    }
    let mut unreserved_seats = unreserved_seats.into_iter();
    for employee_id in &unique_participants {
        if !assigned_seats.contains_key(employee_id) {
            let seat_id = unreserved_seats
                .next()
                .ok_or_else(|| anyhow!("meeting seats are currently reserved"))?;
            assigned_seats.insert(employee_id.clone(), seat_id);
        }
    }

    let meeting_id = format!("meeting-{}", Uuid::new_v4());
    let participant_json = serde_json::to_string(&unique_participants)?;
    let title = input
        .title
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| "产品协作会议".to_string());
    let meeting_type = input
        .meeting_type
        .filter(|meeting_type| !meeting_type.trim().is_empty())
        .unwrap_or_else(|| "TEAM".to_string());
    sqlx::query(
        "INSERT INTO meetings
         (id, company_id, title, type, room_id, participant_ids, status,
          agenda, transcript, decisions, action_items, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'meeting-a', ?, 'WAITING_FOR_PARTICIPANTS',
                 '[]', '[]', '[]', '[]', ?, ?)",
    )
    .bind(&meeting_id)
    .bind(company_id)
    .bind(&title)
    .bind(&meeting_type)
    .bind(&participant_json)
    .bind(&now_text)
    .bind(&now_text)
    .execute(&mut *tx)
    .await?;

    let reserved_until = (now + Duration::hours(2)).to_rfc3339();
    for employee_id in &unique_participants {
        let seat_location_id = assigned_seats
            .get(employee_id)
            .ok_or_else(|| anyhow!("meeting seat assignment is incomplete"))?;
        sqlx::query(
            "INSERT INTO meeting_participants
             (meeting_id, agent_id, seat_location_id, status, created_at, updated_at)
             VALUES (?, ?, ?, 'INVITED', ?, ?)",
        )
        .bind(&meeting_id)
        .bind(employee_id)
        .bind(seat_location_id)
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO location_reservations
             (id, company_id, location_id, agent_id, reserved_until, reason, created_at)
             VALUES (?, ?, ?, ?, ?, 'MEETING', ?)
             ON CONFLICT(company_id, location_id) DO UPDATE SET
                 agent_id = excluded.agent_id,
                 reserved_until = excluded.reserved_until,
                 reason = excluded.reason",
        )
        .bind(format!("reservation-{company_id}-{seat_location_id}"))
        .bind(company_id)
        .bind(seat_location_id)
        .bind(employee_id)
        .bind(&reserved_until)
        .bind(&now_text)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;

    events::emit(
        state,
        company_id,
        "MEETING_CREATED",
        None,
        Some(&meeting_id),
        json!({ "meetingId": meeting_id, "title": title, "participantIds": unique_participants }),
    )
    .await?;

    for employee_id in &unique_participants {
        let seat_location_id = assigned_seats
            .get(employee_id)
            .ok_or_else(|| anyhow!("meeting seat assignment is incomplete"))?;
        match spatial::request_move(state, company_id, employee_id, seat_location_id, "MEETING")
            .await
        {
            Ok(movement) => {
                set_participant_status(&state.pool, &meeting_id, employee_id, "ON_THE_WAY").await?;
                events::emit(
                    state,
                    company_id,
                    "MEETING_PARTICIPANT_ON_THE_WAY",
                    Some(employee_id),
                    Some(&meeting_id),
                    json!({
                        "meetingId": meeting_id,
                        "agentId": employee_id,
                        "seatLocationId": seat_location_id,
                        "movementId": movement.id,
                    }),
                )
                .await?;
            }
            Err(error) => {
                tracing::error!(%error, employee_id, "unable to move meeting participant");
                events::emit(
                    state,
                    company_id,
                    "MEETING_PARTICIPANT_MOVE_FAILED",
                    Some(employee_id),
                    Some(&meeting_id),
                    json!({ "meetingId": meeting_id, "error": error.to_string() }),
                )
                .await?;
            }
        }
    }

    db::meetings(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|meeting| meeting.id == meeting_id)
        .ok_or_else(|| anyhow!("meeting disappeared after creation"))
}

pub async fn finish(
    state: &AppState,
    company_id: &str,
    meeting_id: &str,
) -> Result<MeetingSummary> {
    let participant_rows = sqlx::query(
        "SELECT agent_id, seat_location_id FROM meeting_participants
         WHERE meeting_id = ? AND status IN ('INVITED', 'ON_THE_WAY', 'ARRIVED', 'WAITING', 'JOINED')",
    )
    .bind(meeting_id)
    .fetch_all(&state.pool)
    .await?;
    if participant_rows.is_empty() {
        return Err(anyhow!("meeting has no active participants"));
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE meetings SET status = 'FINISHED', updated_at = ?
         WHERE id = ? AND company_id = ? AND status != 'FINISHED'",
    )
    .bind(&now)
    .bind(meeting_id)
    .bind(company_id)
    .execute(&state.pool)
    .await?;
    sqlx::query(
        "UPDATE meeting_participants SET status = 'LEFT', updated_at = ? WHERE meeting_id = ?",
    )
    .bind(&now)
    .bind(meeting_id)
    .execute(&state.pool)
    .await?;

    events::emit(
        state,
        company_id,
        "MEETING_FINISHED",
        None,
        Some(meeting_id),
        json!({ "meetingId": meeting_id }),
    )
    .await?;

    for row in participant_rows {
        let employee_id: String = row.try_get("agent_id")?;
        let seat_location_id: String = row.try_get("seat_location_id")?;
        db::release_location_reservation(&state.pool, company_id, &seat_location_id, &employee_id)
            .await?;
        let _ = events::transition(
            state,
            company_id,
            &employee_id,
            "IDLE",
            None,
            Some(meeting_id),
        )
        .await;
        let employee = db::employees(&state.pool, company_id)
            .await?
            .into_iter()
            .find(|employee| employee.id == employee_id)
            .ok_or_else(|| anyhow!("employee {employee_id} was not found"))?;
        let workstation =
            spatial::employee_workstation_location(state, company_id, &employee).await?;
        if employee.location_id.as_deref() != Some(workstation.as_str()) {
            spatial::request_move(
                state,
                company_id,
                &employee_id,
                &workstation,
                "MEETING_RETURN",
            )
            .await?;
        }
    }

    db::meetings(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|meeting| meeting.id == meeting_id)
        .ok_or_else(|| anyhow!("meeting was not found"))
}

pub async fn on_agent_arrived(state: &AppState, arrival: &db::MovementArrival) -> Result<()> {
    let row = sqlx::query(
        "SELECT meeting_id, agent_id FROM meeting_participants
         WHERE agent_id = ? AND seat_location_id = ? AND status IN ('INVITED', 'ON_THE_WAY')
         ORDER BY updated_at DESC LIMIT 1",
    )
    .bind(&arrival.agent_id)
    .bind(&arrival.to_location_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some(row) = row else {
        return Ok(());
    };
    let meeting_id: String = row.try_get("meeting_id")?;
    set_participant_status(&state.pool, &meeting_id, &arrival.agent_id, "WAITING").await?;
    events::emit(
        state,
        &arrival.movement.company_id,
        "MEETING_PARTICIPANT_ARRIVED",
        Some(&arrival.agent_id),
        Some(&meeting_id),
        json!({
            "meetingId": meeting_id,
            "agentId": arrival.agent_id,
            "seatLocationId": arrival.to_location_id,
            "status": "WAITING",
        }),
    )
    .await?;

    let waiting: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM meeting_participants
         WHERE meeting_id = ? AND status NOT IN ('ARRIVED', 'WAITING', 'JOINED')",
    )
    .bind(&meeting_id)
    .fetch_one(&state.pool)
    .await?;
    if waiting != 0 {
        return Ok(());
    }
    let changed = sqlx::query(
        "UPDATE meetings SET status = 'IN_PROGRESS', updated_at = ?
         WHERE id = ? AND company_id = ? AND status = 'WAITING_FOR_PARTICIPANTS'",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(&meeting_id)
    .bind(&arrival.movement.company_id)
    .execute(&state.pool)
    .await?
    .rows_affected();
    if changed == 0 {
        return Ok(());
    }
    sqlx::query(
        "UPDATE meeting_participants SET status = 'JOINED', updated_at = ? WHERE meeting_id = ?",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(&meeting_id)
    .execute(&state.pool)
    .await?;
    let participants =
        sqlx::query("SELECT agent_id FROM meeting_participants WHERE meeting_id = ?")
            .bind(&meeting_id)
            .fetch_all(&state.pool)
            .await?;
    for participant in participants {
        let agent_id: String = participant.try_get("agent_id")?;
        events::transition(
            state,
            &arrival.movement.company_id,
            &agent_id,
            "MEETING",
            None,
            Some(&meeting_id),
        )
        .await?;
    }
    events::emit(
        state,
        &arrival.movement.company_id,
        "MEETING_STARTED",
        None,
        Some(&meeting_id),
        json!({ "meetingId": meeting_id }),
    )
    .await?;
    Ok(())
}

async fn set_participant_status(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
    agent_id: &str,
    status: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE meeting_participants SET status = ?, updated_at = ?
         WHERE meeting_id = ? AND agent_id = ?",
    )
    .bind(status)
    .bind(Utc::now().to_rfc3339())
    .bind(meeting_id)
    .bind(agent_id)
    .execute(pool)
    .await?;
    Ok(())
}
