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
    pub desk_id: Option<String>,
    pub hired_at: String,
    pub current_state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub assignee_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummary {
    pub id: String,
    pub title: String,
    pub meeting_type: String,
    pub room_id: String,
    pub participant_ids: Vec<String>,
    pub status: String,
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
pub struct BootstrapResponse {
    pub company: Company,
    pub employees: Vec<Employee>,
    pub tasks: Vec<TaskSummary>,
    pub meetings: Vec<MeetingSummary>,
    pub hiring_requests: Vec<Value>,
    pub office: OfficeLayout,
    pub recent_events: Vec<DomainEvent>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MessageInput {
    pub content: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EventsQuery {
    pub after: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct WebSocketQuery {
    #[serde(rename = "companyId")]
    pub company_id: Option<String>,
}
