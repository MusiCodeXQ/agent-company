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
        CompanySnapshot, EmployeeModelProfileInput, EventsQuery, FloorInput, FloorUpdateInput,
        HireCandidateInput, MeetingInput, MessageHistoryQuery, MessageInput, ModelProfileInput,
        ModelProfileUpdate, MoveAgentInput, TerminationInput, WebSocketQuery, WorkstationInput,
    },
    offboarding, office, openclaw, organization, spatial, websocket,
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
        .route("/api/openclaw/status", get(openclaw_status))
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
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/termination-preview",
            get(termination_preview),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/terminations",
            get(employee_terminations),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/terminate",
            post(terminate_employee),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/runtime/provision",
            post(provision_employee_runtime),
        )
        .route("/api/companies/{company_id}/jobs", get(company_jobs))
        .route(
            "/api/companies/{company_id}/floors",
            get(list_floors).post(create_floor),
        )
        .route(
            "/api/companies/{company_id}/floors/{floor_id}",
            put(update_floor).delete(delete_floor),
        )
        .route(
            "/api/companies/{company_id}/floors/{floor_id}/workstations",
            get(list_workstations).post(create_workstation),
        )
        .route(
            "/api/companies/{company_id}/hiring-requests",
            get(list_hiring_requests),
        )
        .route(
            "/api/companies/{company_id}/hiring-requests/{request_id}/hire",
            post(hire_candidate),
        )
        .route("/api/companies/{company_id}/meetings", post(create_meeting))
        .route(
            "/api/companies/{company_id}/meetings/{meeting_id}/finish",
            post(finish_meeting),
        )
        .route(
            "/api/companies/{company_id}/meetings/{meeting_id}/messages",
            post(post_meeting_message),
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

#[derive(Debug, serde::Deserialize)]
struct SnapshotQuery {
    #[serde(rename = "floorId")]
    floor_id: Option<String>,
}

async fn snapshot(
    State(state): State<AppState>,
    Query(query): Query<SnapshotQuery>,
) -> Result<Json<CompanySnapshot>, ApiError> {
    let employees = db::employees(&state.pool, COMPANY_ID).await?;
    let floors = organization::floors(&state.pool, COMPANY_ID).await?;
    let active_floor_id = query
        .floor_id
        .filter(|floor_id| floors.iter().any(|floor| floor.id == *floor_id))
        .or_else(|| floors.first().map(|floor| floor.id.clone()))
        .ok_or_else(|| ApiError::unprocessable("company has no floor"))?;
    let active_floor = floors
        .iter()
        .find(|floor| floor.id == active_floor_id)
        .ok_or_else(|| ApiError::unprocessable("active floor was not found"))?;
    let active_floor_name = active_floor.name.clone();
    let active_workstations =
        organization::workstations(&state.pool, COMPANY_ID, &active_floor_id).await?;
    let workstation_layout = active_workstations
        .iter()
        .cloned()
        .map(|item| office::WorkstationLayoutInput {
            id: item.id,
            location_id: item.location_id,
            assigned_employee_id: item.assigned_employee_id,
            room_id: item.room_id,
            x: item.x,
            y: item.y,
        })
        .collect::<Vec<_>>();
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
        floors,
        active_floor_id: active_floor_id.clone(),
        workstations: active_workstations,
        hiring_requests: organization::hiring_requests(&state.pool, COMPANY_ID).await?,
        messages: db::messages(&state.pool, COMPANY_ID).await?,
        unread_counts: db::unread_counts(&state.pool, COMPANY_ID).await?,
        jobs: db::jobs(&state.pool, COMPANY_ID).await?,
        model_profiles: model_profiles::list(&state.pool, COMPANY_ID)
            .await
            .map_err(ApiError::from_anyhow)?,
        office: office::office_for_floor(&active_floor_id, &active_floor_name, &workstation_layout),
        office_state,
        active_movements: db::active_movements(&state.pool, COMPANY_ID).await?,
        token_usage: db::company_token_usage(&state.pool, COMPANY_ID).await?,
        server_time: Utc::now().to_rfc3339(),
        recent_events: events::recent(&state.pool, COMPANY_ID).await?,
        openclaw: openclaw::config_status(),
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

async fn openclaw_status() -> Result<Json<crate::models::OpenClawStatus>, ApiError> {
    Ok(Json(openclaw::health().await))
}

async fn termination_preview(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<Json<crate::models::TerminationPreview>, ApiError> {
    Ok(Json(
        offboarding::preview(&state.pool, &company_id, &employee_id)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn employee_terminations(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<Json<Vec<crate::models::TerminationRecordSummary>>, ApiError> {
    Ok(Json(
        offboarding::records(&state.pool, &company_id, &employee_id)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn terminate_employee(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
    Json(input): Json<TerminationInput>,
) -> Result<Json<crate::models::TerminationRecordSummary>, ApiError> {
    let record = offboarding::start(
        &state,
        &company_id,
        &employee_id,
        "user",
        None,
        input.reason.as_deref().unwrap_or("用户发起的离职流程"),
        input.notes.as_deref().unwrap_or(""),
        input.task_action.as_deref().unwrap_or("REASSIGN"),
        input.workspace_action.as_deref().unwrap_or("ARCHIVE"),
    )
    .await
    .map_err(ApiError::from_anyhow)?;
    Ok(Json(record))
}

async fn provision_employee_runtime(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<Json<crate::models::Employee>, ApiError> {
    let employee = db::employees(&state.pool, &company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| ApiError::bad_request("employee was not found"))?;
    if employee.employment_status != "ACTIVE" {
        return Err(ApiError::bad_request(
            "only active employees can be provisioned",
        ));
    }
    let provision = openclaw::provision_employee(&employee)
        .await
        .map_err(ApiError::from_anyhow)?;
    db::set_openclaw_runtime_binding(
        &state.pool,
        &company_id,
        &employee_id,
        &provision.runtime_agent_id,
        &provision.workspace_path,
        provision.agent_dir.as_deref(),
    )
    .await?;
    events::emit(
        &state,
        &company_id,
        "runtime.binding.provisioned",
        Some(&employee_id),
        None,
        json!({
            "employeeId": employee_id,
            "runtimeType": "OPENCLAW",
            "runtimeAgentId": provision.runtime_agent_id,
            "workspacePath": provision.workspace_path
        }),
    )
    .await?;
    let employee = db::employees(&state.pool, &company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| ApiError::bad_request("employee disappeared after provisioning"))?;
    Ok(Json(employee))
}

async fn post_company_message(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Json(input): Json<MessageInput>,
) -> Result<impl IntoResponse, ApiError> {
    let employee_id = if let Some(employee_id) = input.employee_id {
        employee_id
    } else {
        let employees = db::employees(&state.pool, &company_id).await?;
        employees
            .iter()
            .find(|employee| {
                employee.id == "supervisor-01" && employee.employment_status == "ACTIVE"
            })
            .or_else(|| {
                employees.iter().find(|employee| {
                    employee.id == "hr-01" && employee.employment_status == "ACTIVE"
                })
            })
            .map(|employee| employee.id.clone())
            .ok_or_else(|| {
                ApiError::bad_request("company has no employee to receive this message")
            })?
    };
    queue_message(&state, &company_id, &employee_id, input.content.trim()).await
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
    let employee = db::employees(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| ApiError::bad_request("employee was not found"))?;
    if employee.employment_status != "ACTIVE" {
        return Err(ApiError::bad_request(format!(
            "employee is not accepting new messages ({})",
            employee.employment_status
        )));
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

async fn list_floors(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
) -> Result<Json<Vec<crate::models::FloorSummary>>, ApiError> {
    Ok(Json(organization::floors(&state.pool, &company_id).await?))
}

async fn create_floor(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Json(input): Json<FloorInput>,
) -> Result<(StatusCode, Json<crate::models::FloorSummary>), ApiError> {
    let floor = organization::create_floor(
        &state.pool,
        &company_id,
        &input.name,
        input.description.as_deref(),
        input.mission.as_deref(),
    )
    .await
    .map_err(ApiError::from_anyhow)?;
    events::emit(
        &state,
        &company_id,
        "floor.created",
        Some("user"),
        Some(&floor.id),
        serde_json::json!({ "floor": floor }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(floor)))
}

async fn update_floor(
    State(state): State<AppState>,
    Path((company_id, floor_id)): Path<(String, String)>,
    Json(input): Json<FloorUpdateInput>,
) -> Result<Json<crate::models::FloorSummary>, ApiError> {
    let floor = organization::update_floor(
        &state.pool,
        &company_id,
        &floor_id,
        input.name.as_deref(),
        input.description.as_deref(),
        input.mission.as_deref(),
        input.responsibilities.as_deref(),
    )
    .await
    .map_err(ApiError::from_anyhow)?;
    events::emit(
        &state,
        &company_id,
        "floor.updated",
        Some("user"),
        Some(&floor.id),
        serde_json::json!({ "floor": floor }),
    )
    .await?;
    Ok(Json(floor))
}

async fn delete_floor(
    State(state): State<AppState>,
    Path((company_id, floor_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    organization::delete_floor(&state.pool, &company_id, &floor_id)
        .await
        .map_err(ApiError::from_anyhow)?;
    events::emit(
        &state,
        &company_id,
        "floor.deleted",
        Some("user"),
        Some(&floor_id),
        serde_json::json!({ "floorId": floor_id }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_workstations(
    State(state): State<AppState>,
    Path((company_id, floor_id)): Path<(String, String)>,
) -> Result<Json<Vec<crate::models::WorkstationSummary>>, ApiError> {
    Ok(Json(
        organization::workstations(&state.pool, &company_id, &floor_id).await?,
    ))
}

async fn create_workstation(
    State(state): State<AppState>,
    Path((company_id, floor_id)): Path<(String, String)>,
    Json(input): Json<WorkstationInput>,
) -> Result<(StatusCode, Json<crate::models::WorkstationSummary>), ApiError> {
    let workstation = organization::create_workstation(
        &state.pool,
        &company_id,
        &floor_id,
        input.x,
        input.y,
        input.room_id.as_deref(),
        input.orientation.as_deref(),
    )
    .await
    .map_err(ApiError::from_anyhow)?;
    events::emit(
        &state,
        &company_id,
        "workstation.created",
        Some("user"),
        Some(&workstation.id),
        serde_json::json!({ "workstation": workstation, "floorId": floor_id }),
    )
    .await?;
    Ok((StatusCode::CREATED, Json(workstation)))
}

async fn list_hiring_requests(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
) -> Result<Json<Vec<crate::models::HiringRequestSummary>>, ApiError> {
    Ok(Json(
        organization::hiring_requests(&state.pool, &company_id).await?,
    ))
}

async fn hire_candidate(
    State(state): State<AppState>,
    Path((company_id, request_id)): Path<(String, String)>,
    Json(input): Json<HireCandidateInput>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let draft = organization::hire_candidate(
        &state.pool,
        &company_id,
        &request_id,
        &input.candidate_id,
        input.workstation_id.as_deref(),
    )
    .await
    .map_err(ApiError::from_anyhow)?;
    db::ensure_employee_runtime_binding(&state.pool, &company_id, &draft.employee_id).await?;
    let employee = organization::employee_for_hire(&state.pool, &company_id, &draft.employee_id)
        .await
        .map_err(ApiError::from_anyhow)?;
    events::emit(
        &state,
        &company_id,
        "employee.hired",
        Some(&draft.employee_id),
        Some(&request_id),
        serde_json::json!({
            "employee": employee,
            "candidateId": input.candidate_id,
            "requestId": request_id,
            "floorId": draft.floor_id,
            "workstationId": draft.workstation_id,
            "onboardingLocationId": draft.location_id
        }),
    )
    .await?;
    let movement = match spatial::request_move(
        &state,
        &company_id,
        &draft.employee_id,
        &draft.workstation_id,
        "ONBOARDING",
    )
    .await
    {
        Ok(movement) => Some(movement),
        Err(error) => {
            tracing::warn!(%error, employee_id = %draft.employee_id, "new employee onboarding movement was not started");
            None
        }
    };
    events::emit(
        &state,
        &company_id,
        "employee.onboarding.started",
        Some(&draft.employee_id),
        Some(&request_id),
        serde_json::json!({
            "employeeId": draft.employee_id,
            "floorId": draft.floor_id,
            "workstationId": draft.workstation_id,
            "movementId": movement.as_ref().map(|item| item.id.clone())
        }),
    )
    .await?;
    Ok(Json(serde_json::json!({
        "employee": employee,
        "movement": movement,
        "floorId": draft.floor_id,
        "workstationId": draft.workstation_id,
        "status": if movement.is_some() { "ONBOARDING" } else { "HIRED_AT_ENTRANCE" }
    })))
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

async fn post_meeting_message(
    State(state): State<AppState>,
    Path((company_id, meeting_id)): Path<(String, String)>,
    Json(input): Json<MessageInput>,
) -> Result<impl IntoResponse, ApiError> {
    let content = input.content.trim();
    if content.is_empty() {
        return Err(ApiError::bad_request(
            "meeting message content cannot be empty",
        ));
    }
    let batch =
        db::create_meeting_message_jobs(&state.pool, &company_id, &meeting_id, content).await?;
    if batch.messages.is_empty() {
        return Err(ApiError::bad_request(
            "no Agent is currently seated at this meeting table",
        ));
    }

    for (message, job) in batch.messages.iter().zip(batch.jobs.iter()) {
        events::emit(
            &state,
            &company_id,
            "message.sent",
            Some("user"),
            Some(&job.id),
            json!({
                "messageId": message.id,
                "fromType": "user",
                "fromId": "user",
                "toType": "employee",
                "toId": message.to_id,
                "content": message.content,
                "jobId": job.id,
                "meetingId": meeting_id,
                "broadcastId": batch.broadcast_id
            }),
        )
        .await?;
        events::emit(
            &state,
            &company_id,
            "agent.job.created",
            Some(&job.employee_id),
            Some(&job.id),
            json!({
                "jobId": job.id,
                "jobType": job.job_type,
                "status": job.status,
                "employeeId": job.employee_id,
                "messageId": message.id,
                "meetingId": meeting_id,
                "broadcastId": batch.broadcast_id,
                "goalId": job.goal_id,
                "taskId": job.task_id
            }),
        )
        .await?;
    }
    state.worker_notify.notify_one();
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "meetingId": batch.meeting_id,
            "broadcastId": batch.broadcast_id,
            "messages": batch.messages,
            "jobIds": batch.jobs.iter().map(|job| job.id.clone()).collect::<Vec<_>>(),
            "status": "PENDING"
        })),
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
