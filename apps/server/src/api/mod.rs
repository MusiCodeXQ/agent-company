pub(crate) mod websocket;

use axum::{
    extract::{ws::WebSocketUpgrade, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde_json::json;

use crate::{
    agent_runtime::{container_runtime, llm, openclaw},
    app_state::AppState,
    application::{a2a, offboarding, organization},
    domain::{
        models::{
            A2aFaceToFaceSessionInput, A2aSendMessageInput, CompanySnapshot,
            EmployeeModelProfileInput, EventsQuery, FloorInput, FloorUpdateInput,
            HireCandidateInput, MeetingInput, MessageHistoryQuery, MessageInput, ModelProfileInput,
            ModelProfileUpdate, MoveAgentInput, RuntimeProvisionInput, TerminationInput,
            WebSocketQuery, WorkstationInput,
        },
        office,
    },
    infrastructure::{db, events, model_profiles, runtime_store, system_logs},
    office::navigation as spatial,
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
            "/api/container-runtime/status",
            get(container_runtime_status),
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
            "/api/companies/{company_id}/employees/{employee_id}/termination/cancel",
            post(cancel_employee_termination),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/runtime/provision",
            post(provision_employee_runtime),
        )
        .route(
            "/api/companies/{company_id}/employees/{employee_id}/runtime/workspace",
            post(provision_employee_workspace),
        )
        .route("/api/companies/{company_id}/jobs", get(company_jobs))
        .route(
            "/api/companies/{company_id}/system-logs",
            get(company_system_logs),
        )
        .route(
            "/api/companies/{company_id}/runtime/approvals",
            get(list_runtime_tool_approvals),
        )
        .route(
            "/api/companies/{company_id}/runtime/approvals/{approval_id}/decision",
            post(decide_runtime_tool_approval),
        )
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
        .route(
            "/api/companies/{company_id}/hiring-requests/{request_id}/refresh",
            post(refresh_candidates),
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
        .route(
            "/api/companies/{company_id}/a2a/face-to-face-sessions",
            post(create_a2a_face_to_face_session),
        )
        .route(
            "/api/companies/{company_id}/a2a/face-to-face-sessions/{session_id}",
            get(get_a2a_face_to_face_session).delete(close_a2a_face_to_face_session),
        )
        .route(
            "/api/companies/{company_id}/a2a/communications",
            get(list_a2a_communications),
        )
        .route(
            "/api/companies/{company_id}/a2a/communications/{session_id}",
            get(get_a2a_communication),
        )
        .route(
            "/api/companies/{company_id}/a2a/agents/{agent_id}/.well-known/agent-card.json",
            get(get_a2a_agent_card),
        )
        .route(
            "/api/companies/{company_id}/a2a/agents/{agent_id}/message:send",
            post(send_a2a_message),
        )
        .route(
            "/api/companies/{company_id}/a2a/tasks/{*task_operation}",
            get(get_a2a_task).post(cancel_a2a_task),
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
        .map(|employee| crate::domain::models::OfficeActorState {
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
        container_runtime: system_logs::sanitize_container_status(
            state.container_engine.status().await,
        ),
        environment_suggestions: runtime_store::environment_suggestions(&state.pool, COMPANY_ID)
            .await?,
    }))
}

async fn event_list(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Vec<crate::domain::models::DomainEvent>>, ApiError> {
    Ok(Json(
        events::after(&state.pool, COMPANY_ID, query.after.as_deref()).await?,
    ))
}

async fn list_model_profiles(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::domain::models::ModelProfileSummary>>, ApiError> {
    Ok(Json(
        model_profiles::list(&state.pool, COMPANY_ID)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn create_model_profile(
    State(state): State<AppState>,
    Json(input): Json<ModelProfileInput>,
) -> Result<(StatusCode, Json<crate::domain::models::ModelProfileSummary>), ApiError> {
    let profile = model_profiles::create(&state.pool, COMPANY_ID, input)
        .await
        .map_err(ApiError::from_anyhow)?;
    Ok((StatusCode::CREATED, Json(profile)))
}

async fn update_model_profile(
    State(state): State<AppState>,
    Path(profile_id): Path<String>,
    Json(input): Json<ModelProfileUpdate>,
) -> Result<Json<crate::domain::models::ModelProfileSummary>, ApiError> {
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

async fn openclaw_status() -> Result<Json<crate::domain::models::OpenClawStatus>, ApiError> {
    Ok(Json(openclaw::health().await))
}

async fn container_runtime_status(
    State(state): State<AppState>,
) -> Json<crate::domain::models::ContainerRuntimeStatus> {
    Json(system_logs::sanitize_container_status(
        state.container_engine.status().await,
    ))
}

async fn termination_preview(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<Json<crate::domain::models::TerminationPreview>, ApiError> {
    Ok(Json(
        offboarding::preview(&state.pool, &company_id, &employee_id)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn employee_terminations(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<Json<Vec<crate::domain::models::TerminationRecordSummary>>, ApiError> {
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
) -> Result<Json<crate::domain::models::TerminationRecordSummary>, ApiError> {
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

async fn cancel_employee_termination(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<Json<crate::domain::models::Employee>, ApiError> {
    let employee = offboarding::cancel(&state, &company_id, &employee_id)
        .await
        .map_err(ApiError::from_anyhow)?;
    Ok(Json(employee))
}

async fn provision_employee_runtime(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
) -> Result<Json<crate::domain::models::Employee>, ApiError> {
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
    if employee
        .runtime_binding
        .as_ref()
        .is_some_and(|binding| binding.runtime_type == "ADK_RUST")
    {
        container_runtime::archive_employee(&state, &company_id, &employee_id)
            .await
            .map_err(ApiError::from_anyhow)?;
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

async fn provision_employee_workspace(
    State(state): State<AppState>,
    Path((company_id, employee_id)): Path<(String, String)>,
    Json(input): Json<RuntimeProvisionInput>,
) -> Result<Json<crate::domain::models::RuntimeWorkspaceSummary>, ApiError> {
    if input
        .runtime_type
        .as_deref()
        .is_some_and(|value| !value.eq_ignore_ascii_case("ADK_RUST"))
    {
        return Err(ApiError::bad_request(
            "workspace endpoint only supports ADK_RUST",
        ));
    }
    if let Some(binding) = db::employees(&state.pool, &company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .and_then(|employee| employee.runtime_binding)
        .filter(|binding| binding.runtime_type == "OPENCLAW")
    {
        let session_key = format!("agent:{}:main", binding.runtime_agent_id);
        if let Err(error) = openclaw::archive_agent(&binding.runtime_agent_id, &session_key).await {
            tracing::warn!(%error, employee_id, "cannot archive OpenClaw agent while switching to ADK Rust");
        }
    }
    let workspace = container_runtime::provision_employee(&state, &company_id, &employee_id)
        .await
        .map_err(ApiError::from_anyhow)?;
    events::emit(
        &state,
        &company_id,
        "runtime.workspace.provisioned",
        Some(&employee_id),
        None,
        json!({
            "employeeId": employee_id,
            "runtimeType": "ADK_RUST",
            "hostRootPath": workspace.host_root_path,
            "workspacePath": workspace.workspace_path,
            "statePath": workspace.state_path,
            "environmentPath": workspace.environment_path,
            "artifactsPath": workspace.artifacts_path
        }),
    )
    .await?;
    Ok(Json(workspace))
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
                employee.is_floor_administrator && employee.employment_status == "ACTIVE"
            })
            .or_else(|| {
                employees.iter().find(|employee| {
                    employee.id == "supervisor-01" && employee.employment_status == "ACTIVE"
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
) -> Result<Json<crate::domain::models::Employee>, ApiError> {
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
) -> Result<Json<Vec<crate::domain::models::MessageRecord>>, ApiError> {
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
) -> Result<Json<Vec<crate::domain::models::AgentMemoryRecord>>, ApiError> {
    Ok(Json(
        db::memory(&state.pool, &company_id, &employee_id).await?,
    ))
}

async fn company_jobs(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
) -> Result<Json<Vec<crate::domain::models::AgentJobSummary>>, ApiError> {
    Ok(Json(db::jobs(&state.pool, &company_id).await?))
}

async fn company_system_logs(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Query(query): Query<system_logs::SystemLogsQuery>,
) -> Result<Json<system_logs::SystemLogsResponse>, ApiError> {
    let page = system_logs::collect_page(&state.pool, &company_id, &query).await?;
    Ok(Json(system_logs::SystemLogsResponse {
        entries: page.entries,
        container_runtime: system_logs::sanitize_container_status(
            state.container_engine.status().await,
        ),
        page: page.page,
        page_size: page.page_size,
        has_more: page.has_more,
    }))
}

async fn list_runtime_tool_approvals(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
) -> Result<Json<Vec<crate::domain::models::RuntimeToolApprovalSummary>>, ApiError> {
    Ok(Json(
        runtime_store::pending_tool_approvals(&state.pool, &company_id).await?,
    ))
}

async fn decide_runtime_tool_approval(
    State(state): State<AppState>,
    Path((company_id, approval_id)): Path<(String, String)>,
    Json(input): Json<crate::domain::models::RuntimeToolApprovalDecision>,
) -> Result<Json<crate::domain::models::RuntimeToolApprovalSummary>, ApiError> {
    let decision = match input.decision.to_ascii_uppercase().as_str() {
        "APPROVE" | "APPROVED" => "APPROVED",
        "DENY" | "DENIED" => "DENIED",
        _ => return Err(ApiError::bad_request("decision must be APPROVE or DENY")),
    };
    let approval = runtime_store::decide_tool_approval(
        &state.pool,
        &company_id,
        &approval_id,
        decision,
        "user",
        input.reason.as_deref(),
    )
    .await?
    .ok_or_else(|| ApiError::bad_request("approval was not pending or was not found"))?;
    events::emit(
        &state,
        &company_id,
        "runtime.approval.decided",
        Some(&approval.employee_id),
        Some(&approval.run_id),
        json!({
            "approvalId": approval.id,
            "jobId": approval.agent_job_id,
            "employeeId": approval.employee_id,
            "toolName": approval.tool_name,
            "status": approval.status,
            "decidedBy": approval.decided_by,
        }),
    )
    .await?;
    state.worker_notify.notify_one();
    Ok(Json(approval))
}

async fn list_floors(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
) -> Result<Json<Vec<crate::domain::models::FloorSummary>>, ApiError> {
    Ok(Json(organization::floors(&state.pool, &company_id).await?))
}

async fn create_floor(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Json(input): Json<FloorInput>,
) -> Result<(StatusCode, Json<crate::domain::models::FloorSummary>), ApiError> {
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
) -> Result<Json<crate::domain::models::FloorSummary>, ApiError> {
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
) -> Result<Json<Vec<crate::domain::models::WorkstationSummary>>, ApiError> {
    Ok(Json(
        organization::workstations(&state.pool, &company_id, &floor_id).await?,
    ))
}

async fn create_workstation(
    State(state): State<AppState>,
    Path((company_id, floor_id)): Path<(String, String)>,
    Json(input): Json<WorkstationInput>,
) -> Result<(StatusCode, Json<crate::domain::models::WorkstationSummary>), ApiError> {
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
) -> Result<Json<Vec<crate::domain::models::HiringRequestSummary>>, ApiError> {
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

async fn refresh_candidates(
    State(state): State<AppState>,
    Path((company_id, request_id)): Path<(String, String)>,
) -> Result<Json<crate::domain::models::HiringRequestSummary>, ApiError> {
    let template = organization::refresh_candidates(&state.pool, &company_id, &request_id)
        .await
        .map_err(ApiError::from_anyhow)?;
    let generation = match organization::personalize_hiring_candidates(
        &state.pool,
        &company_id,
        &request_id,
        template.requester_id.as_deref(),
        None,
    )
    .await
    {
        Ok(generation) => generation,
        Err(_error) => {
            tracing::warn!(
                request_id,
                fallback_reason = organization::FALLBACK_GENERATION_SERVICE_UNAVAILABLE,
                "HTTP hiring refresh fell back to committed deterministic candidates"
            );
            organization::HiringPersonalizationResult::fallback(
                template,
                organization::FALLBACK_GENERATION_SERVICE_UNAVAILABLE,
            )
        }
    };
    let request = generation.request.clone();
    events::emit(
        &state,
        &company_id,
        "hiring.candidates.refreshed",
        request.requester_id.as_deref(),
        Some(&request_id),
        serde_json::json!({
            "request": request.clone(),
            "generationMode": generation.generation_mode,
            "fallbackReason": generation.fallback_reason,
        }),
    )
    .await?;
    Ok(Json(request))
}

async fn create_meeting(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Json(input): Json<MeetingInput>,
) -> Result<(StatusCode, Json<crate::domain::models::MeetingSummary>), ApiError> {
    let meeting = crate::application::meetings::create(&state, &company_id, input)
        .await
        .map_err(ApiError::from_anyhow)?;
    Ok((StatusCode::CREATED, Json(meeting)))
}

async fn finish_meeting(
    State(state): State<AppState>,
    Path((company_id, meeting_id)): Path<(String, String)>,
) -> Result<Json<crate::domain::models::MeetingSummary>, ApiError> {
    Ok(Json(
        crate::application::meetings::finish(&state, &company_id, &meeting_id)
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
) -> Result<
    (
        StatusCode,
        Json<crate::domain::models::AgentMovementSummary>,
    ),
    ApiError,
> {
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

async fn create_a2a_face_to_face_session(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Json(input): Json<A2aFaceToFaceSessionInput>,
) -> Result<
    (
        StatusCode,
        Json<crate::domain::models::A2aFaceToFaceSessionCreated>,
    ),
    ApiError,
> {
    let created = a2a::create_face_to_face_session(&state, &company_id, input)
        .await
        .map_err(ApiError::from_anyhow)?;
    Ok((StatusCode::ACCEPTED, Json(created)))
}

async fn get_a2a_face_to_face_session(
    State(state): State<AppState>,
    Path((company_id, session_id)): Path<(String, String)>,
) -> Result<Json<crate::domain::models::A2aFaceToFaceSession>, ApiError> {
    Ok(Json(
        a2a::session(&state.pool, &company_id, &session_id)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn close_a2a_face_to_face_session(
    State(state): State<AppState>,
    Path((company_id, session_id)): Path<(String, String)>,
) -> Result<Json<crate::domain::models::A2aFaceToFaceSession>, ApiError> {
    Ok(Json(
        a2a::close_session(&state, &company_id, &session_id, "closed by API")
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn list_a2a_communications(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Query(query): Query<crate::domain::models::A2aHistoryQuery>,
) -> Result<Json<Vec<crate::domain::models::A2aFaceToFaceSession>>, ApiError> {
    Ok(Json(
        a2a::list_communications(
            &state.pool,
            &company_id,
            query.limit.unwrap_or(40),
            query.before.as_deref(),
        )
        .await
        .map_err(ApiError::from_anyhow)?,
    ))
}

async fn get_a2a_communication(
    State(state): State<AppState>,
    Path((company_id, session_id)): Path<(String, String)>,
) -> Result<Json<crate::domain::models::A2aCommunicationDetail>, ApiError> {
    Ok(Json(
        a2a::communication_detail(&state.pool, &company_id, &session_id)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn get_a2a_agent_card(
    State(state): State<AppState>,
    Path((company_id, agent_id)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<a2a::AgentCard>, ApiError> {
    let scheme = header_value(&headers, "x-forwarded-proto").unwrap_or("http");
    let host = header_value(&headers, "host").unwrap_or("localhost:3001");
    let service_url = format!("{scheme}://{host}/api/companies/{company_id}/a2a/agents/{agent_id}");
    Ok(Json(
        a2a::agent_card(&state, &company_id, &agent_id, &service_url)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn send_a2a_message(
    State(state): State<AppState>,
    Path((company_id, agent_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(input): Json<A2aSendMessageInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let version = header_value(&headers, "a2a-version").unwrap_or(a2a::PROTOCOL_VERSION);
    if version != a2a::PROTOCOL_VERSION {
        return Err(ApiError::bad_request(format!(
            "unsupported A2A version {version}; supported version is {}",
            a2a::PROTOCOL_VERSION
        )));
    }
    let sender_id = header_value(&headers, "x-a2a-sender-id")
        .ok_or_else(|| ApiError::bad_request("X-A2A-Sender-Id is required"))?;
    let capability_token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::bad_request("Bearer face-to-face capability token is required"))?;
    let task = a2a::send_message(
        &state,
        &company_id,
        &agent_id,
        sender_id,
        capability_token,
        input,
    )
    .await
    .map_err(ApiError::from_anyhow)?;
    Ok((StatusCode::ACCEPTED, Json(task)))
}

async fn get_a2a_task(
    State(state): State<AppState>,
    Path((company_id, task_operation)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    if task_operation.contains(':') {
        return Err(ApiError::bad_request("invalid A2A task path"));
    }
    let task_id = task_operation;
    let (actor_id, capability_token) = a2a_credentials(&headers)?;
    a2a::authorize_task(
        &state.pool,
        &company_id,
        &task_id,
        actor_id,
        capability_token,
    )
    .await
    .map_err(ApiError::from_anyhow)?;
    Ok(Json(
        a2a::get_task(&state.pool, &company_id, &task_id)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

async fn cancel_a2a_task(
    State(state): State<AppState>,
    Path((company_id, task_operation)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, ApiError> {
    let task_id = task_operation
        .strip_suffix(":cancel")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ApiError::bad_request("A2A POST task operation must end with :cancel"))?;
    let (actor_id, capability_token) = a2a_credentials(&headers)?;
    Ok(Json(
        a2a::cancel_task(&state, &company_id, task_id, actor_id, capability_token)
            .await
            .map_err(ApiError::from_anyhow)?,
    ))
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|value| value.to_str().ok())
}

fn a2a_credentials(headers: &HeaderMap) -> Result<(&str, &str), ApiError> {
    let actor_id = header_value(headers, "x-a2a-sender-id")
        .ok_or_else(|| ApiError::bad_request("X-A2A-Sender-Id is required"))?;
    let capability_token = headers
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::bad_request("Bearer face-to-face capability token is required"))?;
    Ok((actor_id, capability_token))
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
