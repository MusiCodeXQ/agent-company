use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::json;

use crate::{app_state::AppState, db, events, meetings, models::MeetingInput, office, spatial};

/// Deterministic spatial commands understood by the Agent Worker.
///
/// These commands intentionally do not ask an LLM to choose coordinates. The
/// worker turns the user's natural-language intent into one of the same
/// persisted movement/meeting operations used by the REST API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpatialCommand {
    MoveToMeetingRoom,
    MoveToWorkstation,
    MoveToCoffeePoint,
    MoveToRestroom,
    MoveToEntrance,
    AllToMeetingRoom,
    EndMeeting,
    NotifyEveryoneToOffice,
}

impl SpatialCommand {
    pub fn name(&self) -> &'static str {
        match self {
            Self::MoveToMeetingRoom => "MOVE_TO_MEETING_ROOM",
            Self::MoveToWorkstation => "MOVE_TO_WORKSTATION",
            Self::MoveToCoffeePoint => "MOVE_TO_COFFEE_POINT",
            Self::MoveToRestroom => "MOVE_TO_RESTROOM",
            Self::MoveToEntrance => "MOVE_TO_ENTRANCE",
            Self::AllToMeetingRoom => "ALL_TO_MEETING_ROOM",
            Self::EndMeeting => "END_MEETING",
            Self::NotifyEveryoneToOffice => "NOTIFY_EVERYONE_TO_OFFICE",
        }
    }
}

#[derive(Debug)]
pub struct CommandExecution {
    pub reply: String,
}

/// Parse only explicit, high-signal spatial phrases. General conversation is
/// left to the configured model provider and keeps the existing runtime path.
pub fn parse(content: &str) -> Option<SpatialCommand> {
    let normalized = content
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let everyone = contains_any(&normalized, &["所有人", "所有员工", "大家", "全员"]);
    let meeting = contains_any(
        &normalized,
        &["会议室", "去开会", "去会议", "开会", "参加会议"],
    );
    let office = contains_any(
        &normalized,
        &[
            "办公室",
            "开放办公区",
            "回工位",
            "回到工位",
            "去工位",
            "工作位",
        ],
    );
    let end_meeting = contains_any(
        &normalized,
        &[
            "散会",
            "结束会议",
            "会议结束",
            "会议到此结束",
            "结束本次会议",
        ],
    );

    if end_meeting {
        return Some(SpatialCommand::EndMeeting);
    }
    if everyone && meeting {
        return Some(SpatialCommand::AllToMeetingRoom);
    }
    if everyone && office {
        return Some(SpatialCommand::NotifyEveryoneToOffice);
    }
    if meeting {
        return Some(SpatialCommand::MoveToMeetingRoom);
    }
    if office {
        return Some(SpatialCommand::MoveToWorkstation);
    }
    if contains_any(&normalized, &["茶水间", "咖啡", "喝水", "喝咖啡"]) {
        return Some(SpatialCommand::MoveToCoffeePoint);
    }
    if contains_any(
        &normalized,
        &["洗手间", "卫生间", "厕所", "上厕所", "去方便"],
    ) {
        return Some(SpatialCommand::MoveToRestroom);
    }
    if contains_any(&normalized, &["门口", "入口", "公司门"])
        && contains_any(&normalized, &["去", "到", "前往", "站"])
    {
        return Some(SpatialCommand::MoveToEntrance);
    }
    None
}

pub async fn execute(
    state: &AppState,
    company_id: &str,
    actor_id: &str,
    command: SpatialCommand,
) -> Result<CommandExecution> {
    let command_name = command.name();
    let (reply, details) = match command {
        SpatialCommand::MoveToMeetingRoom => {
            let movement =
                request_first_available_meeting_seat(state, company_id, actor_id).await?;
            (
                format!(
                    "收到，我现在沿办公室路线前往会议室，目标座位是 {}。",
                    movement.to_location_id
                ),
                json!({ "movementId": movement.id, "destinationLocationId": movement.to_location_id }),
            )
        }
        SpatialCommand::MoveToWorkstation => {
            let employee = employee(state, company_id, actor_id).await?;
            let workstation =
                spatial::employee_workstation_location(state, company_id, &employee).await?;
            if employee.location_id.as_deref() == Some(workstation.as_str())
                && employee.active_movement_id.is_none()
            {
                (
                    "开放办公区就是办公室，我已经在自己的工位。".to_string(),
                    json!({ "destinationLocationId": workstation, "alreadyThere": true }),
                )
            } else {
                let movement = spatial::request_move(
                    state,
                    company_id,
                    actor_id,
                    &workstation,
                    "CHAT_MOVE_TO_WORKSTATION",
                )
                .await?;
                (
                    format!("好的，我现在回到自己的工位：{}。", workstation),
                    json!({ "movementId": movement.id, "destinationLocationId": workstation }),
                )
            }
        }
        SpatialCommand::MoveToCoffeePoint => {
            let movement = spatial::request_move(
                state,
                company_id,
                actor_id,
                "coffee_point_01",
                "CHAT_MOVE_TO_COFFEE_POINT",
            )
            .await?;
            (
                "好的，我去茶水间的咖啡点。".to_string(),
                json!({ "movementId": movement.id, "destinationLocationId": movement.to_location_id }),
            )
        }
        SpatialCommand::MoveToRestroom => {
            let movement = spatial::request_move(
                state,
                company_id,
                actor_id,
                "toilet_entrance",
                "CHAT_MOVE_TO_RESTROOM",
            )
            .await?;
            (
                "好的，我会沿通行路线前往洗手间入口。".to_string(),
                json!({ "movementId": movement.id, "destinationLocationId": movement.to_location_id }),
            )
        }
        SpatialCommand::MoveToEntrance => {
            let movement = spatial::request_move(
                state,
                company_id,
                actor_id,
                "office_entrance",
                "CHAT_MOVE_TO_ENTRANCE",
            )
            .await?;
            (
                "好的，我现在前往公司入口。".to_string(),
                json!({ "movementId": movement.id, "destinationLocationId": movement.to_location_id }),
            )
        }
        SpatialCommand::AllToMeetingRoom => {
            let employees = db::employees(&state.pool, company_id).await?;
            let participant_ids = employees
                .iter()
                .map(|employee| employee.id.clone())
                .collect::<Vec<_>>();
            let meeting = meetings::create(
                state,
                company_id,
                MeetingInput {
                    title: Some("全员会议".to_string()),
                    participant_ids,
                    meeting_type: Some("TEAM".to_string()),
                },
            )
            .await?;
            (
                format!(
                    "已通知全员前往会议室，会议 {} 已创建。大家会沿各自路线到达，最后一人到达后才开始。",
                    meeting.title
                ),
                json!({ "meetingId": meeting.id, "participantCount": meeting.participant_ids.len() }),
            )
        }
        SpatialCommand::EndMeeting => {
            let meeting = db::meetings(&state.pool, company_id)
                .await?
                .into_iter()
                .find(|meeting| {
                    matches!(
                        meeting.status.as_str(),
                        "CREATED" | "WAITING_FOR_PARTICIPANTS" | "IN_PROGRESS"
                    )
                });
            if let Some(meeting) = meeting {
                let participant_count = meeting.participant_ids.len();
                let finished = meetings::finish(state, company_id, &meeting.id).await?;
                (
                    format!(
                        "已宣布散会，{} 位参会者会沿各自路线回到工位。到达时间由后端 Worker 持续处理。",
                        participant_count
                    ),
                    json!({ "meetingId": finished.id, "participantCount": participant_count }),
                )
            } else {
                (
                    "当前没有进行中的会议，大家不需要执行散会移动。".to_string(),
                    json!({ "alreadyFinished": true }),
                )
            }
        }
        SpatialCommand::NotifyEveryoneToOffice => {
            let employees = db::employees(&state.pool, company_id).await?;
            let employee_count = employees.len();
            let state_for_round = state.clone();
            let company_id_for_round = company_id.to_string();
            let actor_id_for_round = actor_id.to_string();
            tokio::spawn(async move {
                if let Err(error) = supervisor_notification_round(
                    &state_for_round,
                    &company_id_for_round,
                    &actor_id_for_round,
                )
                .await
                {
                    tracing::warn!(%error, actor_id = %actor_id_for_round, "supervisor office notification round failed");
                    let _ = events::emit(
                        &state_for_round,
                        &company_id_for_round,
                        "AGENT_SPATIAL_COMMAND_FAILED",
                        Some(&actor_id_for_round),
                        None,
                        json!({
                            "command": SpatialCommand::NotifyEveryoneToOffice.name(),
                            "error": error.to_string(),
                        }),
                    )
                    .await;
                }
            });
            (
                format!(
                    "开放办公区就是当前办公室。已安排我逐个走到 {} 位同事的工位通知；已经在自己工位的人不需要被瞬移或重复移动。若要看到所有人真正走动，请说‘让所有人去会议室’。",
                    employee_count.saturating_sub(1)
                ),
                json!({ "targetRoomId": "open-office", "employeeCount": employee_count }),
            )
        }
    };

    if let Err(error) = events::emit(
        state,
        company_id,
        "AGENT_SPATIAL_COMMAND_EXECUTED",
        Some(actor_id),
        None,
        json!({ "command": command_name, "details": details }),
    )
    .await
    {
        tracing::warn!(%error, command = command_name, "unable to persist spatial command event");
    }

    Ok(CommandExecution { reply })
}

async fn request_first_available_meeting_seat(
    state: &AppState,
    company_id: &str,
    actor_id: &str,
) -> Result<crate::models::AgentMovementSummary> {
    let layout = office::default_office();
    let mut last_reservation_error = None;
    for location in layout
        .locations
        .iter()
        .filter(|location| location.location_type == "MEETING_SEAT")
    {
        match spatial::request_move(
            state,
            company_id,
            actor_id,
            &location.id,
            "CHAT_MOVE_TO_MEETING_ROOM",
        )
        .await
        {
            Ok(movement) => return Ok(movement),
            Err(error)
                if error
                    .to_string()
                    .contains("destination location is reserved") =>
            {
                last_reservation_error = Some(error);
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_reservation_error.unwrap_or_else(|| anyhow!("no meeting seat is configured")))
}

async fn supervisor_notification_round(
    state: &AppState,
    company_id: &str,
    supervisor_id: &str,
) -> Result<()> {
    let employees = db::employees(&state.pool, company_id).await?;
    let supervisor = employees
        .iter()
        .find(|employee| employee.id == supervisor_id)
        .ok_or_else(|| anyhow!("supervisor/actor {supervisor_id} was not found"))?;
    let supervisor_workstation =
        spatial::employee_workstation_location(state, company_id, supervisor).await?;

    for recipient in employees
        .iter()
        .filter(|employee| employee.id != supervisor_id)
    {
        let target = spatial::employee_workstation_location(state, company_id, recipient).await?;
        if let Some(movement) = move_if_needed(
            state,
            company_id,
            supervisor_id,
            &target,
            "SUPERVISOR_FACE_TO_FACE_NOTIFICATION",
        )
        .await?
        {
            wait_until_arrived(state, company_id, supervisor_id, &target).await?;
            db::release_location_reservation(&state.pool, company_id, &target, supervisor_id)
                .await?;
            tracing::debug!(movement_id = %movement.id, recipient_id = %recipient.id, "supervisor arrived at employee workstation");
        }

        events::emit(
            state,
            company_id,
            "AGENT_FACE_TO_FACE_MESSAGE",
            Some(supervisor_id),
            None,
            json!({
                "recipientId": recipient.id,
                "content": "请回到办公室并在自己的工位工作。",
                "locationId": target,
            }),
        )
        .await?;

        let current_recipient = employee(state, company_id, &recipient.id).await?;
        let recipient_workstation =
            spatial::employee_workstation_location(state, company_id, &current_recipient).await?;
        if current_recipient.location_id.as_deref() != Some(recipient_workstation.as_str())
            && current_recipient.active_movement_id.is_none()
        {
            let _ = spatial::request_move(
                state,
                company_id,
                &recipient.id,
                &recipient_workstation,
                "SUPERVISOR_RETURN_TO_OFFICE",
            )
            .await;
        }
    }

    if let Some(movement) = move_if_needed(
        state,
        company_id,
        supervisor_id,
        &supervisor_workstation,
        "SUPERVISOR_RETURN_TO_WORKSTATION",
    )
    .await?
    {
        wait_until_arrived(state, company_id, supervisor_id, &supervisor_workstation).await?;
        db::release_location_reservation(
            &state.pool,
            company_id,
            &supervisor_workstation,
            supervisor_id,
        )
        .await?;
        tracing::debug!(movement_id = %movement.id, "supervisor returned to workstation");
    }
    Ok(())
}

async fn move_if_needed(
    state: &AppState,
    company_id: &str,
    agent_id: &str,
    destination: &str,
    reason: &str,
) -> Result<Option<crate::models::AgentMovementSummary>> {
    let employee = employee(state, company_id, agent_id).await?;
    if employee.location_id.as_deref() == Some(destination) && employee.active_movement_id.is_none()
    {
        return Ok(None);
    }
    spatial::request_move(state, company_id, agent_id, destination, reason)
        .await
        .map(Some)
}

async fn employee(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
) -> Result<crate::models::Employee> {
    db::employees(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| anyhow!("employee {employee_id} was not found"))
}

async fn wait_until_arrived(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
    location_id: &str,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    loop {
        let employee = employee(state, company_id, employee_id).await?;
        if employee.location_id.as_deref() == Some(location_id)
            && employee.active_movement_id.is_none()
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(anyhow!(
                "timed out waiting for agent {employee_id} to arrive"
            ));
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::{parse, SpatialCommand};

    #[test]
    fn parses_single_agent_meeting_command() {
        assert_eq!(
            parse("请你现在去会议室"),
            Some(SpatialCommand::MoveToMeetingRoom)
        );
    }

    #[test]
    fn parses_broadcast_meeting_command() {
        assert_eq!(
            parse("让所有人去会议室开会"),
            Some(SpatialCommand::AllToMeetingRoom)
        );
    }

    #[test]
    fn keeps_office_broadcast_semantics_explicit() {
        assert_eq!(
            parse("让大家回办公室"),
            Some(SpatialCommand::NotifyEveryoneToOffice)
        );
        assert_eq!(
            parse("让所有人去办公室"),
            Some(SpatialCommand::NotifyEveryoneToOffice)
        );
    }

    #[test]
    fn parses_end_meeting_commands_before_workstation_broadcasts() {
        assert_eq!(parse("散会"), Some(SpatialCommand::EndMeeting));
        assert_eq!(
            parse("散会，所有人回到工位"),
            Some(SpatialCommand::EndMeeting)
        );
    }

    #[test]
    fn parses_restroom_commands() {
        assert_eq!(
            parse("你去一下洗手间"),
            Some(SpatialCommand::MoveToRestroom)
        );
        assert_eq!(parse("我需要去厕所"), Some(SpatialCommand::MoveToRestroom));
    }
}
