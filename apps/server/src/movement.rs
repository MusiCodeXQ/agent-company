use std::time::Duration;

use anyhow::Result;
use serde_json::json;

use crate::{a2a, app_state::AppState, db, events, meetings, offboarding};

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        tracing::info!("office movement worker started");
        loop {
            if let Err(error) = process_once(&state).await {
                if error.to_string().contains("database is locked") {
                    tracing::debug!(%error, "movement write conflicted with another SQLite transaction; retrying");
                } else {
                    tracing::error!(%error, "office movement worker iteration failed");
                }
            }
            tokio::select! {
                _ = state.worker_notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_millis(200)) => {}
            }
        }
    });
}

async fn process_once(state: &AppState) -> Result<()> {
    for movement_id in db::pending_movement_ids(&state.pool).await? {
        if let Some(movement) = db::start_movement(&state.pool, &movement_id).await? {
            events::emit(
                state,
                &movement.company_id,
                "AGENT_MOVE_STARTED",
                Some(&movement.agent_id),
                Some(&movement.id),
                json!({ "movement": movement }),
            )
            .await?;
        }
    }

    for movement_id in db::due_movements(&state.pool).await? {
        let Some(arrival) = db::complete_movement(&state.pool, &movement_id).await? else {
            continue;
        };
        events::emit(
            state,
            &arrival.movement.company_id,
            "AGENT_ARRIVED",
            Some(&arrival.agent_id),
            Some(&arrival.movement.id),
            json!({
                "movement": arrival.movement,
                "locationId": arrival.to_location_id,
                "roomId": arrival.room_id,
                "posture": arrival.posture,
            }),
        )
        .await?;
        events::emit(
            state,
            &arrival.movement.company_id,
            "AGENT_LOCATION_CHANGED",
            Some(&arrival.agent_id),
            Some(&arrival.movement.id),
            json!({
                "locationId": arrival.to_location_id,
                "roomId": arrival.room_id,
            }),
        )
        .await?;
        if arrival.posture == "SITTING" {
            events::emit(
                state,
                &arrival.movement.company_id,
                "AGENT_SAT_DOWN",
                Some(&arrival.agent_id),
                Some(&arrival.movement.id),
                json!({ "locationId": arrival.to_location_id }),
            )
            .await?;
        }
        meetings::on_agent_arrived(state, &arrival).await?;
        a2a::on_agent_arrived(state, &arrival).await?;
        offboarding::on_agent_arrived(state, &arrival).await?;
    }
    Ok(())
}
