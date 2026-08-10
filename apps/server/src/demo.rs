use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::json;
use uuid::Uuid;

use crate::{app_state::AppState, db, events};

pub async fn start(state: AppState, company_id: String) -> bool {
    if state.demo_running.swap(true, Ordering::AcqRel) {
        return false;
    }
    let running = state.demo_running.clone();
    tokio::spawn(async move {
        if let Err(error) = run(&state, &company_id).await {
            tracing::error!(%error, "mock office event failed");
            let _ = events::emit(
                &state,
                &company_id,
                "agent.error",
                None,
                None,
                json!({ "message": error.to_string(), "mode": "mock" }),
            )
            .await;
        }
        running.store(false, Ordering::Release);
    });
    true
}

async fn run(state: &AppState, company_id: &str) -> Result<(), sqlx::Error> {
    let correlation_id = format!("demo-{}", Uuid::new_v4());
    let meeting_id = format!("meeting-{}", Uuid::new_v4());
    db::insert_demo_meeting(&state.pool, company_id, &meeting_id).await?;

    events::emit(
        state,
        company_id,
        "demo.started",
        Some("system"),
        Some(&correlation_id),
        json!({ "title": "Backend meeting loop", "employeeId": "backend-01" }),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "message.received",
        Some("product-01"),
        Some(&correlation_id),
        json!({
            "messageId": format!("msg-{}", Uuid::new_v4()),
            "fromType": "employee",
            "fromId": "product-01",
            "toType": "employee",
            "toId": "backend-01",
            "content": "顾言，来会议室确认实时通信 API 的边界。"
        }),
    )
    .await?;
    events::transition(
        state,
        company_id,
        "backend-01",
        "checking_phone",
        Some("task-demo-meeting"),
        Some(&correlation_id),
    )
    .await?;

    tokio::time::sleep(Duration::from_millis(1_400)).await;
    events::transition(
        state,
        company_id,
        "backend-01",
        "thinking",
        Some("task-demo-meeting"),
        Some(&correlation_id),
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(900)).await;

    events::emit(
        state,
        company_id,
        "meeting.created",
        Some("product-01"),
        Some(&correlation_id),
        json!({
            "meetingId": meeting_id,
            "title": "实时通信边界同步",
            "roomId": "meeting-a",
            "participantIds": ["backend-01", "product-01", "supervisor-01"]
        }),
    )
    .await?;
    events::transition(
        state,
        company_id,
        "backend-01",
        "walking",
        Some("task-demo-meeting"),
        Some(&correlation_id),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "employee.destination.changed",
        Some("backend-01"),
        Some(&correlation_id),
        json!({ "destination": { "kind": "room", "id": "meeting-a" } }),
    )
    .await?;

    tokio::time::sleep(Duration::from_millis(3_200)).await;
    events::transition(
        state,
        company_id,
        "backend-01",
        "meeting",
        Some("task-demo-meeting"),
        Some(&correlation_id),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "meeting.started",
        Some("product-01"),
        Some(&correlation_id),
        json!({ "meetingId": meeting_id, "roomId": "meeting-a" }),
    )
    .await?;
    tokio::time::sleep(Duration::from_millis(2_600)).await;
    events::emit(
        state,
        company_id,
        "meeting.finished",
        Some("product-01"),
        Some(&correlation_id),
        json!({ "meetingId": meeting_id, "decisions": ["先冻结事件协议，再拆分实现任务"] }),
    )
    .await?;
    events::transition(
        state,
        company_id,
        "backend-01",
        "walking",
        Some("task-demo-coding"),
        Some(&correlation_id),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "employee.destination.changed",
        Some("backend-01"),
        Some(&correlation_id),
        json!({ "destination": { "kind": "desk", "id": "desk-backend-01" } }),
    )
    .await?;

    tokio::time::sleep(Duration::from_millis(3_000)).await;
    events::transition(
        state,
        company_id,
        "backend-01",
        "coding",
        Some("task-demo-coding"),
        Some(&correlation_id),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "task.state.changed",
        Some("backend-01"),
        Some(&correlation_id),
        json!({ "taskId": "task-demo-coding", "from": "meeting", "to": "in_progress" }),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "demo.finished",
        Some("system"),
        Some(&correlation_id),
        json!({ "result": "backend returned to desk and started coding" }),
    )
    .await?;
    Ok(())
}
