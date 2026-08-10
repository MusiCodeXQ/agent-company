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
    pub employment_status: String,
    pub personality: PersonalityProfile,
    pub skills: Vec<SkillProfile>,
    pub appearance: AppearanceProfile,
    pub model_config: AgentModelConfig,
    pub model_profile_id: Option<String>,
    pub model_profile: Option<ModelProfileSummary>,
    pub runtime_binding: Option<RuntimeBindingSummary>,
    pub desk_id: Option<String>,
    pub floor_id: Option<String>,
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
    pub previous_assignee_id: Option<String>,
    pub handover_status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeBindingSummary {
    pub id: String,
    pub employee_id: String,
    pub runtime_type: String,
    pub runtime_agent_id: String,
    pub runtime_profile_id: Option<String>,
    pub workspace_path: String,
    pub agent_dir: Option<String>,
    pub provision_status: String,
    pub runtime_status: String,
    pub last_health_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawStatus {
    pub enabled: bool,
    pub configured: bool,
    pub gateway_url: String,
    pub reachable: bool,
    pub protocol: Option<i64>,
    pub version: Option<String>,
    pub last_error: Option<String>,
    pub checked_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminationTaskSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub goal_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminationPreview {
    pub employee: Employee,
    pub active_task_count: i64,
    pub running_job_count: i64,
    pub active_goal_count: i64,
    pub active_meeting_count: i64,
    pub artifact_count: i64,
    pub workspace_path: Option<String>,
    pub workspace_status: String,
    pub runtime_type: String,
    pub runtime_status: String,
    pub workstation_id: Option<String>,
    pub unread_count: i64,
    pub tasks: Vec<TerminationTaskSummary>,
    pub active_termination_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminationRecordSummary {
    pub id: String,
    pub company_id: String,
    pub employee_id: String,
    pub initiated_by: String,
    pub reason: String,
    pub notes: String,
    pub status: String,
    pub workspace_action: String,
    pub task_handover_status: String,
    pub runtime_shutdown_status: String,
    pub workstation_release_status: String,
    pub created_at: String,
    pub effective_at: Option<String>,
    pub completed_at: Option<String>,
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
    pub meeting_id: Option<String>,
    pub broadcast_id: Option<String>,
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
    pub meeting_id: Option<String>,
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
pub struct FloorSummary {
    pub id: String,
    pub company_id: String,
    pub floor_number: i64,
    pub name: String,
    pub description: String,
    pub mission: String,
    pub responsibilities: Vec<String>,
    pub roles: Vec<FloorRoleSummary>,
    pub status: String,
    pub employee_count: i64,
    pub workstation_count: i64,
    pub available_workstations: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorRoleSummary {
    pub id: String,
    pub name: String,
    pub responsibilities: Vec<String>,
    pub skills: Vec<String>,
    pub preferences: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkstationSummary {
    pub id: String,
    pub company_id: String,
    pub floor_id: String,
    pub location_id: String,
    pub room_id: Option<String>,
    pub x: f32,
    pub y: f32,
    pub orientation: String,
    pub status: String,
    pub assigned_employee_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HiringPositionSummary {
    pub id: String,
    pub role_definition_id: Option<String>,
    pub title: String,
    pub headcount: i64,
    pub requirements: Vec<String>,
    pub preferences: Vec<String>,
    pub status: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateSummary {
    pub id: String,
    pub hiring_request_id: String,
    pub position_id: String,
    pub name: String,
    pub age: u8,
    pub gender: String,
    pub title: String,
    pub personality: PersonalityProfile,
    pub skills: Vec<SkillProfile>,
    pub work_style: String,
    pub strengths: Vec<String>,
    pub weaknesses: Vec<String>,
    pub expertise: Vec<String>,
    pub expected_model: String,
    pub agent_runtime: String,
    pub appearance: AppearanceProfile,
    pub match_score: i64,
    pub interview_evaluation: String,
    pub status: String,
    pub hired_employee_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HiringRequestSummary {
    pub id: String,
    pub company_id: String,
    pub floor_id: String,
    pub requester_id: Option<String>,
    pub summary: String,
    pub status: String,
    pub source_message_id: Option<String>,
    pub positions: Vec<HiringPositionSummary>,
    pub candidates: Vec<CandidateSummary>,
    pub created_at: String,
    pub updated_at: String,
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
    pub floors: Vec<FloorSummary>,
    pub active_floor_id: String,
    pub workstations: Vec<WorkstationSummary>,
    pub hiring_requests: Vec<HiringRequestSummary>,
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
    pub openclaw: OpenClawStatus,
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
#[serde(rename_all = "camelCase")]
pub struct FloorInput {
    pub name: String,
    pub description: Option<String>,
    pub mission: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorUpdateInput {
    pub name: Option<String>,
    pub description: Option<String>,
    pub mission: Option<String>,
    pub responsibilities: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkstationInput {
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub room_id: Option<String>,
    pub orientation: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HireCandidateInput {
    pub candidate_id: String,
    pub workstation_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminationInput {
    pub reason: Option<String>,
    pub notes: Option<String>,
    pub task_action: Option<String>,
    pub workspace_action: Option<String>,
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
