use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::office::OfficeLayout;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Company {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonalityProfile {
    pub openness: u8,
    pub conscientiousness: u8,
    pub extraversion: u8,
    pub agreeableness: u8,
    pub emotional_stability: u8,
    pub traits: Vec<String>,
    pub communication_style: String,
    pub decision_style: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillProfile {
    pub name: String,
    pub level: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceProfile {
    pub seed: u64,
    pub skin_tone: String,
    pub hair_style: String,
    pub hair_color: String,
    pub top_style: String,
    pub top_color: String,
    pub bottom_style: String,
    pub bottom_color: String,
    pub shoe_color: String,
    pub body_type: String,
    pub height_scale: f32,
    pub glasses: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beard_style: Option<String>,
    pub accessory: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelConfig {
    pub provider: String,
    pub model: String,
    pub mode: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsageSummary {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Employee {
    pub id: String,
    pub company_id: String,
    pub name: String,
    pub age: u8,
    pub gender: String,
    pub role: String,
    pub department: String,
    pub title: String,
    pub status: String,
    pub personality: PersonalityProfile,
    pub skills: Vec<SkillProfile>,
    pub appearance: AppearanceProfile,
    pub model_config: AgentModelConfig,
    pub model_profile_id: Option<String>,
    pub model_profile: Option<ModelProfileSummary>,
    pub desk_id: Option<String>,
    pub hired_at: String,
    pub current_state: String,
    pub current_destination: Option<Value>,
    /// Compatibility field for the first runtime milestone. New consumers should use
    /// the independent spatial/activity dimensions below.
    pub activity: String,
    pub movement: String,
    pub location_id: Option<String>,
    pub x: f32,
    pub y: f32,
    pub room_id: Option<String>,
    pub device: String,
    pub posture: String,
    pub presence: String,
    pub active_movement_id: Option<String>,
    pub unread_count: i64,
    pub token_usage: TokenUsageSummary,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub description: String,
    pub title: String,
    pub task_type: String,
    pub status: String,
    pub assignee_id: Option<String>,
    pub goal_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalSummary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    pub meeting_type: String,
    pub room_id: String,
    pub participant_ids: Vec<String>,
    pub participants: Vec<MeetingParticipantSummary>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingParticipantSummary {
    pub agent_id: String,
    pub seat_location_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfileSummary {
    pub id: String,
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub api_key_configured: bool,
    pub api_key_hint: Option<String>,
    pub model: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRecord {
    pub id: String,
    pub company_id: String,
    pub from_type: String,
    pub from_id: String,
    pub to_type: String,
    pub to_id: String,
    pub content: String,
    pub created_at: String,
    pub read_at: Option<String>,
    pub job_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMemoryRecord {
    pub id: String,
    pub employee_id: String,
    pub kind: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentJobSummary {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub employee_id: String,
    pub message_id: Option<String>,
    pub goal_id: Option<String>,
    pub task_id: Option<String>,
    pub attempts: i64,
    pub result: Option<String>,
    pub error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfficeActorState {
    pub employee_id: String,
    pub current_state: String,
    pub destination: Option<Value>,
    pub activity: String,
    pub movement: String,
    pub location_id: Option<String>,
    pub x: f32,
    pub y: f32,
    pub room_id: Option<String>,
    pub device: String,
    pub posture: String,
    pub presence: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MovementPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMovementSummary {
    pub id: String,
    pub company_id: String,
    pub agent_id: String,
    pub from_location_id: String,
    pub to_location_id: String,
    pub path: Vec<MovementPoint>,
    pub started_at: String,
    pub estimated_arrival_at: String,
    pub movement_status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomainEvent {
    pub event_id: String,
    pub company_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: String,
    pub actor_id: Option<String>,
    pub correlation_id: Option<String>,
    pub payload: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanySnapshot {
    pub company: Company,
    pub employees: Vec<Employee>,
    pub goals: Vec<GoalSummary>,
    pub tasks: Vec<TaskSummary>,
    pub meetings: Vec<MeetingSummary>,
    pub hiring_requests: Vec<Value>,
    pub messages: Vec<MessageRecord>,
    pub unread_counts: HashMap<String, i64>,
    pub jobs: Vec<AgentJobSummary>,
    pub model_profiles: Vec<ModelProfileSummary>,
    pub office: OfficeLayout,
    pub office_state: Vec<OfficeActorState>,
    pub active_movements: Vec<AgentMovementSummary>,
    pub token_usage: TokenUsageSummary,
    pub server_time: String,
    pub recent_events: Vec<DomainEvent>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MessageInput {
    pub content: String,
    pub employee_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfileInput {
    pub name: String,
    pub protocol: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProfileUpdate {
    pub name: Option<String>,
    pub protocol: Option<String>,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmployeeModelProfileInput {
    pub profile_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingInput {
    pub title: Option<String>,
    pub participant_ids: Vec<String>,
    pub meeting_type: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveAgentInput {
    pub destination_location_id: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EventsQuery {
    pub after: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageHistoryQuery {
    pub limit: Option<u32>,
    pub before: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WebSocketQuery {
    #[serde(rename = "companyId")]
    pub company_id: Option<String>,
}
