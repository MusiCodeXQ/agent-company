use axum::{
    extract::{ws::WebSocketUpgrade, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde_json::json;

use crate::{
    app_state::AppState,
    db, events, llm, model_profiles,
    models::{
        CompanySnapshot, EmployeeModelProfileInput, EventsQuery, MeetingInput, MessageHistoryQuery,
        MessageInput, ModelProfileInput, ModelProfileUpdate, MoveAgentInput, WebSocketQuery,
    },
    office, spatial, websocket,
};

const COMPANY_ID: &str = "company-01";

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/company/snapshot", get(snapshot))
        .route("/api/bootstrap", get(snapshot))
        .route("/api/events", get(event_list))
        .route(
            "/api/model-profiles",
            get(list_model_profiles).post(create_model_profile),
        )
        .route(
            "/api/model-profiles/{profile_id}",
            put(update_model_profile),
        )
        .route(
            "/api/model-profiles/{profile_id}/test",
            post(test_model_profile),
        )
        .route(
            "/api/companies/{company_id}/messages",
            post(post_company_message),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/messages",
            get(employee_messages).post(post_agent_message),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/model-profile",
            put(set_employee_model_profile),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/messages/read",
            post(mark_employee_messages_read),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/memory",
            get(employee_memory),
        )
        .route("/api/companies/{company_id}/jobs", get(company_jobs))
        .route("/api/companies/{company_id}/meetings", post(create_meeting))
        .route(
            "/api/companies/{company_id}/meetings/{meeting_id}/finish",
            post(finish_meeting),
        )
        .route(
            "/api/companies/{company_id}/agents/{agent_id}/move",
            post(move_agent),
        )
        .route("/ws", get(websocket_upgrade))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "status": "ok",
        "mode": "persistent-agent-runtime",
        "service": "ai-company-server"
    }))
}

async fn snapshot(State(state): State<AppState>) -> Result<Json<CompanySnapshot>, ApiError> {
    let employees = db::employees(&state.pool, COMPANY_ID).await?;
    let office_state = employees
        .iter()
        .map(|employee| crate::models::OfficeActorState {
            employee_id: employee.id.clone(),
            current_state: employee.current_state.clone(),
            destination: employee.current_destination.clone(),
            activity: employee.activity.clone(),
            movement: employee.movement.clone(),
            location_id: employee.location_id.clone(),
            x: employee.x,
            y: employee.y,
            room_id: employee.room_id.clone(),
            device: employee.device.clone(),
            posture: employee.posture.clone(),
            presence: employee.presence.clone(),
        })
        .collect();
    Ok(Json(CompanySnapshot {
        company: db::company(&state.pool, COMPANY_ID).await?,
        employees,
        goals: db::goals(&state.pool, COMPANY_ID).await?,
        tasks: db::tasks(&state.pool, COMPANY_ID).await?,
        meetings: db::meetings(&state.pool, COMPANY_ID).await?,
        hiring_requests: Vec::new(),
        messages: db::messages(&state.pool, COMPANY_ID).await?,
        unread_counts: db::unread_counts(&state.pool, COMPANY_ID).await?,
        jobs: db::jobs(&state.pool, COMPANY_ID).await?,
        model_profiles: model_profiles::list(&state.pool, COMPANY_ID)
            .await
            .map_err(ApiError::from_anyhow)?,
        office: office::default_office(),
        office_state,
        active_movements: db::active_movements(&state.pool, COMPANY_ID).await?,
        token_usage: db::company_token_usage(&state.pool, COMPANY_ID).await?,
        server_time: Utc::now().to_rfc3339(),
        recent_events: events::recent(&state.pool, COMPANY_ID).await?,
    }))
}

async fn event_list(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Vec<crate::models::DomainEvent>>, ApiError> {
    Ok(Json(
        events::after(&state.pool, COMPANY_ID, query.after.as_deref()).await?,
    ))
}

async fn list_model_profiles(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::ModelProfileSummary>>, ApiError> {
    Ok(Json(
        model_profiles::list(&state.pool, COMPANY_ID)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn create_model_profile(
    State(state): State<AppState>,
    Json(input): Json<ModelProfileInput>,
) -> Result<(StatusCode, Json<crate::models::ModelProfileSummary>), ApiError> {
    let profile = model_profiles::create(&state.pool, COMPANY_ID, input)
        .await
        .map_err(ApiError::from_anyhow)?;
    Ok((StatusCode::CREATED, Json(profile)))
}

async fn update_model_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Json(input): Json<ModelProfileUpdate>,
) -> Result<Json<crate::models::ModelProfileSummary>, ApiError> {
    Ok(Json(
        model_profiles::update(&state.pool, &profile_id, input)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn test_model_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let profile = model_profiles::get_credentials(&state.pool, &profile_id)
        .await
        .map_err(ApiError::from_anyhow)?;
    match llm::test_connection(&profile).await {
        Ok(message) => Ok(Json(json!({ "ok": true, "message": message }))),
        Err(error) => Err(ApiError::unprocessable(error.to_string())),
    }
}

async fn post_company_message(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Json(input): Json<MessageInput>,
) -> Result<impl IntoResponse, ApiError> {
    let employee_id = input.employee_id.as_deref().unwrap_or("supervisor-01");
    queue_message(&state, &company_id, employee_id, input.content.trim()).await
}

async fn post_agent_message(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
    Json(input): Json<MessageInput>,
) -> Result<impl IntoResponse, ApiError> {
    queue_message(&state, &company_id, &employee_id, input.content.trim()).await
}

async fn set_employee_model_profile(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
    Json(input): Json<EmployeeModelProfileInput>,
) -> Result<Json<crate::models::Employee>, ApiError> {
    db::set_employee_model_profile(
        &state.pool,
        &company_id,
        &employee_id,
        input.profile_id.as_deref(),
    )
    .await?;
    let employee = db::employees(&state.pool, &company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| ApiError::bad_request("employee was not found"))?;
    events::emit(
        &state,
        &company_id,
        "agent.model.profile.changed",
        Some(&employee_id),
        None,
        json!({ "employeeId": employee_id, "modelProfileId": employee.model_profile_id }),
    )
    .await?;
    Ok(Json(employee))
}

async fn queue_message(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
    content: &str,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    if content.is_empty() {
        return Err(ApiError::bad_request("message content cannot be empty"));
    }
    let (message, job) =
        db::create_message_job(&state.pool, company_id, employee_id, content).await?;
    events::emit(
        state,
        company_id,
        "message.sent",
        Some("user"),
        Some(&job.id),
        json!({
            "messageId": message.id.clone(),
            "fromType": "user",
            "fromId": "user",
            "toType": "employee",
            "toId": employee_id,
            "content": content,
            "jobId": job.id.clone()
        }),
    )
    .await?;
    events::emit(
        state,
        company_id,
        "agent.job.created",
        Some(employee_id),
        Some(&job.id),
        json!({
            "jobId": job.id.clone(),
            "jobType": job.job_type.clone(),
            "status": job.status.clone(),
            "employeeId": employee_id,
            "messageId": message.id.clone(),
            "goalId": job.goal_id.clone(),
            "taskId": job.task_id.clone()
        }),
    )
    .await?;
    state.worker_notify.notify_one();
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({ "message": message, "jobId": job.id, "status": job.status })),
    ))
}

async fn employee_messages(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
    Query(query): Query<MessageHistoryQuery>,
) -> Result<Json<Vec<crate::models::MessageRecord>>, ApiError> {
    Ok(Json(
        db::messages_for_employee_page(
            &state.pool,
            &company_id,
            &employee_id,
            query.limit.unwrap_or(40),
            query.before.as_deref(),
        )
        .await?,
    ))
}

async fn mark_employee_messages_read(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    db::mark_messages_read(&state.pool, &company_id, &employee_id).await?;
    events::emit(
        &state,
        &company_id,
        "message.read",
        Some("user"),
        None,
        json!({ "employeeId": employee_id }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn employee_memory(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<Json<Vec<crate::models::AgentMemoryRecord>>, ApiError> {
    Ok(Json(
        db::memory(&state.pool, &company_id, &employee_id).await?,
    ))
}

async fn company_jobs(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
) -> Result<Json<Vec<crate::models::AgentJobSummary>>, ApiError> {
    Ok(Json(db::jobs(&state.pool, &company_id).await?))
}

async fn create_meeting(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Json(input): Json<MeetingInput>,
) -> Result<(StatusCode, Json<crate::models::MeetingSummary>), ApiError> {
    let meeting = crate::meetings::create(&state, &company_id, input)
        .await
        .map_err(ApiError::from_anyhow)?;
    Ok((StatusCode::CREATED, Json(meeting)))
}

async fn finish_meeting(
    State(state): State<AppState>,
    Path((company_id, meeting_id)): Path<(String, String)>,
) -> Result<Json<crate::models::MeetingSummary>, ApiError> {
    Ok(Json(
        crate::meetings::finish(&state, &company_id, &meeting_id)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn move_agent(
    State(state): State<AppState>,
    Path((company_id, agent_id)): Path<(String, String)>,
    Json(input): Json<MoveAgentInput>,
) -> Result<(StatusCode, Json<crate::models::AgentMovementSummary>), ApiError> {
    let movement = spatial::request_move(
        &state,
        &company_id,
        &agent_id,
        &input.destination_location_id,
        input.reason.as_deref().unwrap_or("API"),
    )
    .await
    .map_err(ApiError::from_anyhow)?;
    Ok((StatusCode::ACCEPTED, Json(movement)))
}

async fn websocket_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WebSocketQuery>,
) -> impl IntoResponse {
    let company_id = query.company_id.unwrap_or_else(|| COMPANY_ID.to_string());
    ws.on_upgrade(move |socket| websocket::handle_socket(socket, state, company_id))
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn unprocessable(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: message.into(),
        }
    }

    fn from_anyhow(error: anyhow::Error) -> Self {
        tracing::error!(%error, "api service error");
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "api database error");
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "database operation failed".to_string(),
        }
    }
}
