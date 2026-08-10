use axum::{
    extract::{ws::WebSocketUpgrade, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use crate::{
    app_state::AppState,
    db, demo, events,
    models::{BootstrapResponse, EventsQuery, MessageInput, WebSocketQuery},
    office, websocket,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/events", get(event_list))
        .route("/api/companies/{company_id}/messages", post(post_message))
        .route("/api/companies/{company_id}/demo", post(start_demo))
        .route("/ws", get(websocket_upgrade))
        .with_state(state)
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok", "mode": "mock", "service": "ai-company-server" }))
}

async fn bootstrap(State(state): State<AppState>) -> Result<Json<BootstrapResponse>, ApiError> {
    let company_id = "company-01";
    Ok(Json(BootstrapResponse {
        company: db::company(&state.pool, company_id).await?,
        employees: db::employees(&state.pool, company_id).await?,
        tasks: db::tasks(&state.pool, company_id).await?,
        meetings: db::meetings(&state.pool, company_id).await?,
        hiring_requests: Vec::new(),
        office: office::default_office(),
        recent_events: events::recent(&state.pool, company_id).await?,
    }))
}

async fn event_list(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<Json<Vec<crate::models::DomainEvent>>, ApiError> {
    Ok(Json(
        events::after(&state.pool, "company-01", query.after.as_deref()).await?,
    ))
}

async fn post_message(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
    Json(input): Json<MessageInput>,
) -> Result<impl IntoResponse, ApiError> {
    let content = input.content.trim();
    if content.is_empty() {
        return Err(ApiError::bad_request("message content cannot be empty"));
    }
    let event = events::emit(
        &state,
        &company_id,
        "message.sent",
        Some("user"),
        None,
        json!({
            "fromType": "user",
            "fromId": "user",
            "toType": "company",
            "toId": company_id,
            "content": content
        }),
    )
    .await?;
    Ok((StatusCode::ACCEPTED, Json(event)))
}

async fn start_demo(
    State(state): State<AppState>,
    Path(company_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let started = demo::start(state, company_id).await;
    Ok((
        if started {
            StatusCode::ACCEPTED
        } else {
            StatusCode::CONFLICT
        },
        Json(json!({ "started": started })),
    ))
}

async fn websocket_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(query): Query<WebSocketQuery>,
) -> impl IntoResponse {
    let company_id = query.company_id.unwrap_or_else(|| "company-01".to_string());
    ws.on_upgrade(move |socket| websocket::handle_socket(socket, state, company_id))
}

#[derive(Debug)]
struct ApiError {
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
