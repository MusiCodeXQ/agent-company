use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{FromRow, SqlitePool};

use crate::models::ContainerRuntimeStatus;

#[derive(Clone, Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SystemLogsQuery {
    pub limit: Option<u32>,
    pub level: Option<String>,
    pub source: Option<String>,
    pub employee_id: Option<String>,
    #[serde(default)]
    pub exception_only: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemLogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub employee_id: Option<String>,
    pub employee_name: Option<String>,
    pub message: String,
    pub details: Option<String>,
    pub job_id: Option<String>,
    pub runtime_run_id: Option<String>,
    pub event_type: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemLogsResponse {
    pub entries: Vec<SystemLogEntry>,
    pub container_runtime: ContainerRuntimeStatus,
}

pub fn sanitize_container_status(mut status: ContainerRuntimeStatus) -> ContainerRuntimeStatus {
    status.last_error = status.last_error.map(|error| redact_details(&error));
    status
}

#[derive(Debug, FromRow)]
struct EmployeeNameRow {
    id: String,
    name: String,
}

#[derive(Debug, FromRow)]
struct AgentJobLogRow {
    id: String,
    job_type: String,
    status: String,
    employee_id: String,
    runtime_run_id: Option<String>,
    result: Option<String>,
    error: Option<String>,
    payload: String,
    updated_at: String,
}

#[derive(Debug, FromRow)]
struct RuntimeRunLogRow {
    id: String,
    employee_id: String,
    agent_job_id: Option<String>,
    status: String,
    started_at: String,
    heartbeat_at: Option<String>,
    completed_at: Option<String>,
    stop_reason: Option<String>,
}

#[derive(Debug, FromRow)]
struct RuntimeEventLogRow {
    id: String,
    employee_id: Option<String>,
    run_id: Option<String>,
    event_type: String,
    payload: String,
    created_at: String,
}

#[derive(Debug, FromRow)]
struct LlmCallLogRow {
    id: String,
    employee_id: String,
    job_id: Option<String>,
    model: String,
    status: String,
    latency_ms: i64,
    error: Option<String>,
    created_at: String,
}

#[derive(Clone, Copy)]
enum RequestedLevel {
    Error,
    Warn,
    Info,
    Unknown,
}

fn requested_level(query: &SystemLogsQuery) -> Option<RequestedLevel> {
    query.level.as_deref().map(|level| {
        if level.eq_ignore_ascii_case("ERROR") {
            RequestedLevel::Error
        } else if level.eq_ignore_ascii_case("WARN") {
            RequestedLevel::Warn
        } else if level.eq_ignore_ascii_case("INFO") {
            RequestedLevel::Info
        } else {
            RequestedLevel::Unknown
        }
    })
}

fn source_enabled(query: &SystemLogsQuery, source: &str) -> bool {
    query
        .source
        .as_deref()
        .map(|requested| requested.eq_ignore_ascii_case(source))
        .unwrap_or(true)
}

fn source_level_predicate(query: &SystemLogsQuery, source: &str) -> Option<&'static str> {
    if query.exception_only {
        return match source {
            "JOB" => Some("status IN ('FAILED', 'WAITING', 'CANCELLED')"),
            "RUNTIME" => Some("status IN ('FAILED', 'WAITING')"),
            "RUNTIME_EVENT" => Some(
                "(lower(event_type) LIKE '%.failed' OR lower(event_type) = 'run.failed' OR lower(event_type) = 'tool.failed' OR lower(event_type) LIKE '%warning%' OR lower(event_type) LIKE '%.warn%' OR lower(event_type) LIKE '%approval.required%' OR lower(event_type) LIKE '%timeout%')",
            ),
            "MODEL" => Some("status = 'FAILED'"),
            _ => None,
        };
    }

    match (source, requested_level(query)) {
        (_, None | Some(RequestedLevel::Unknown)) => None,
        ("JOB", Some(RequestedLevel::Error)) => Some("status = 'FAILED'"),
        ("JOB", Some(RequestedLevel::Warn)) => Some("status IN ('WAITING', 'CANCELLED')"),
        ("JOB", Some(RequestedLevel::Info)) => {
            Some("status NOT IN ('FAILED', 'WAITING', 'CANCELLED')")
        }
        ("RUNTIME", Some(RequestedLevel::Error)) => Some("status = 'FAILED'"),
        ("RUNTIME", Some(RequestedLevel::Warn)) => Some("status = 'WAITING'"),
        ("RUNTIME", Some(RequestedLevel::Info)) => Some("status NOT IN ('FAILED', 'WAITING')"),
        ("RUNTIME_EVENT", Some(RequestedLevel::Error)) => Some(
            "(lower(event_type) LIKE '%.failed' OR lower(event_type) = 'run.failed' OR lower(event_type) = 'tool.failed')",
        ),
        ("RUNTIME_EVENT", Some(RequestedLevel::Warn)) => Some(
            "(lower(event_type) LIKE '%warning%' OR lower(event_type) LIKE '%.warn%' OR lower(event_type) LIKE '%approval.required%' OR lower(event_type) LIKE '%timeout%')",
        ),
        ("RUNTIME_EVENT", Some(RequestedLevel::Info)) => Some(
            "NOT (lower(event_type) LIKE '%.failed' OR lower(event_type) = 'run.failed' OR lower(event_type) = 'tool.failed' OR lower(event_type) LIKE '%warning%' OR lower(event_type) LIKE '%.warn%' OR lower(event_type) LIKE '%approval.required%' OR lower(event_type) LIKE '%timeout%')",
        ),
        ("MODEL", Some(RequestedLevel::Error)) => Some("status = 'FAILED'"),
        ("MODEL", Some(RequestedLevel::Warn)) => Some("0"),
        ("MODEL", Some(RequestedLevel::Info)) => Some("status != 'FAILED'"),
        _ => None,
    }
}

fn source_query(base: &str, order: &str, employee_filter: bool, predicate: Option<&str>) -> String {
    let mut statement = String::from(base);
    if employee_filter {
        statement.push_str(" AND employee_id = ?");
    }
    if let Some(predicate) = predicate {
        statement.push_str(" AND ");
        statement.push_str(predicate);
    }
    statement.push(' ');
    statement.push_str(order);
    statement.push_str(" LIMIT ?");
    statement
}

pub async fn collect(
    pool: &SqlitePool,
    company_id: &str,
    query: &SystemLogsQuery,
) -> Result<Vec<SystemLogEntry>, sqlx::Error> {
    let employee_names =
        sqlx::query_as::<_, EmployeeNameRow>("SELECT id, name FROM employees WHERE company_id = ?")
            .bind(company_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| (row.id, row.name))
            .collect::<HashMap<_, _>>();

    // Keep a small per-source cushion for the final merge while applying known
    // level predicates in SQL before this limit.
    let source_limit = source_limit(query.limit);
    let mut entries = Vec::new();

    if source_enabled(query, "JOB") {
        let statement = source_query(
            "SELECT id, job_type, status, employee_id, runtime_run_id, result, error, payload, updated_at FROM agent_jobs WHERE company_id = ?",
            "ORDER BY updated_at DESC, id DESC",
            query.employee_id.is_some(),
            source_level_predicate(query, "JOB"),
        );
        let jobs = sqlx::query_as::<_, AgentJobLogRow>(&statement).bind(company_id);
        let jobs = if let Some(employee_id) = query.employee_id.as_deref() {
            jobs.bind(employee_id)
        } else {
            jobs
        }
        .bind(source_limit)
        .fetch_all(pool)
        .await?;
        for row in jobs {
            let level = job_level(&row.status).to_string();
            let details = row
                .error
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .or(row.result.as_deref())
                .filter(|value| !value.trim().is_empty())
                .map(redact_details)
                .or_else(|| non_empty_details(&row.payload));
            let entry = SystemLogEntry {
                id: format!("job:{}", row.id),
                timestamp: row.updated_at,
                level,
                source: "JOB".to_string(),
                employee_id: Some(row.employee_id.clone()),
                employee_name: employee_names.get(&row.employee_id).cloned(),
                message: format!("Job {} · {}", row.status, row.job_type),
                details,
                job_id: Some(row.id),
                runtime_run_id: row.runtime_run_id,
                event_type: None,
                model: None,
            };
            push_if_matches(&mut entries, entry, query);
        }
    }

    if source_enabled(query, "RUNTIME") {
        let statement = source_query(
            "SELECT id, employee_id, agent_job_id, status, started_at, heartbeat_at, completed_at, stop_reason FROM runtime_runs WHERE company_id = ?",
            "ORDER BY COALESCE(completed_at, heartbeat_at, started_at) DESC, id DESC",
            query.employee_id.is_some(),
            source_level_predicate(query, "RUNTIME"),
        );
        let runs = sqlx::query_as::<_, RuntimeRunLogRow>(&statement).bind(company_id);
        let runs = if let Some(employee_id) = query.employee_id.as_deref() {
            runs.bind(employee_id)
        } else {
            runs
        }
        .bind(source_limit)
        .fetch_all(pool)
        .await?;
        for row in runs {
            let timestamp = row
                .completed_at
                .or(row.heartbeat_at)
                .unwrap_or(row.started_at);
            let level = runtime_level(&row.status).to_string();
            let details = row
                .stop_reason
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .map(redact_details);
            let entry = SystemLogEntry {
                id: format!("runtime:{}", row.id),
                timestamp,
                level,
                source: "RUNTIME".to_string(),
                employee_id: Some(row.employee_id.clone()),
                employee_name: employee_names.get(&row.employee_id).cloned(),
                message: format!("Runtime run {}", row.status),
                details,
                job_id: row.agent_job_id,
                runtime_run_id: Some(row.id),
                event_type: None,
                model: None,
            };
            push_if_matches(&mut entries, entry, query);
        }
    }

    if source_enabled(query, "RUNTIME_EVENT") {
        let statement = source_query(
            "SELECT id, employee_id, run_id, event_type, payload, created_at FROM runtime_events WHERE company_id = ?",
            "ORDER BY created_at DESC, id DESC",
            query.employee_id.is_some(),
            source_level_predicate(query, "RUNTIME_EVENT"),
        );
        let events = sqlx::query_as::<_, RuntimeEventLogRow>(&statement).bind(company_id);
        let events = if let Some(employee_id) = query.employee_id.as_deref() {
            events.bind(employee_id)
        } else {
            events
        }
        .bind(source_limit)
        .fetch_all(pool)
        .await?;
        for row in events {
            let level = runtime_event_level(&row.event_type).to_string();
            let employee_name = row
                .employee_id
                .as_deref()
                .and_then(|id| employee_names.get(id))
                .cloned();
            let entry = SystemLogEntry {
                id: format!("runtime-event:{}", row.id),
                timestamp: row.created_at,
                level,
                source: "RUNTIME_EVENT".to_string(),
                employee_id: row.employee_id,
                employee_name,
                message: format!("Runtime event · {}", row.event_type),
                details: non_empty_details(&row.payload),
                job_id: None,
                runtime_run_id: row.run_id,
                event_type: Some(row.event_type),
                model: None,
            };
            push_if_matches(&mut entries, entry, query);
        }
    }

    if source_enabled(query, "MODEL") {
        let statement = source_query(
            "SELECT id, employee_id, job_id, model, status, latency_ms, error, created_at FROM llm_calls WHERE company_id = ?",
            "ORDER BY created_at DESC, id DESC",
            query.employee_id.is_some(),
            source_level_predicate(query, "MODEL"),
        );
        let calls = sqlx::query_as::<_, LlmCallLogRow>(&statement).bind(company_id);
        let calls = if let Some(employee_id) = query.employee_id.as_deref() {
            calls.bind(employee_id)
        } else {
            calls
        }
        .bind(source_limit)
        .fetch_all(pool)
        .await?;
        for row in calls {
            let level = if row.status.eq_ignore_ascii_case("FAILED") {
                "ERROR"
            } else {
                "INFO"
            };
            let employee_name = employee_names.get(&row.employee_id).cloned();
            let entry = SystemLogEntry {
                id: format!("llm:{}", row.id),
                timestamp: row.created_at,
                level: level.to_string(),
                source: "MODEL".to_string(),
                employee_id: Some(row.employee_id),
                employee_name,
                message: format!("Model call · {} · {} ms", row.model, row.latency_ms),
                details: row
                    .error
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                    .map(redact_details),
                job_id: row.job_id,
                runtime_run_id: None,
                event_type: None,
                model: Some(row.model),
            };
            push_if_matches(&mut entries, entry, query);
        }
    }

    entries.sort_by(|left, right| {
        right
            .timestamp
            .cmp(&left.timestamp)
            .then_with(|| right.id.cmp(&left.id))
    });
    entries.dedup_by(|left, right| left.id == right.id);
    entries.truncate(effective_limit(query.limit));
    Ok(entries)
}

fn push_if_matches(
    entries: &mut Vec<SystemLogEntry>,
    entry: SystemLogEntry,
    query: &SystemLogsQuery,
) {
    if query
        .level
        .as_deref()
        .is_some_and(|level| !entry.level.eq_ignore_ascii_case(level))
    {
        return;
    }
    if query
        .source
        .as_deref()
        .is_some_and(|source| !entry.source.eq_ignore_ascii_case(source))
    {
        return;
    }
    if query
        .employee_id
        .as_deref()
        .is_some_and(|employee_id| entry.employee_id.as_deref() != Some(employee_id))
    {
        return;
    }
    if query.exception_only && !matches!(entry.level.as_str(), "ERROR" | "WARN") {
        return;
    }
    entries.push(entry);
}

fn effective_limit(limit: Option<u32>) -> usize {
    limit.unwrap_or(100).clamp(1, 500) as usize
}

fn source_limit(limit: Option<u32>) -> i64 {
    (effective_limit(limit).saturating_mul(4).clamp(100, 2_000)) as i64
}

fn job_level(status: &str) -> &'static str {
    match status.to_ascii_uppercase().as_str() {
        "FAILED" => "ERROR",
        "WAITING" | "CANCELLED" => "WARN",
        _ => "INFO",
    }
}

fn runtime_level(status: &str) -> &'static str {
    if status.eq_ignore_ascii_case("FAILED") {
        "ERROR"
    } else if status.eq_ignore_ascii_case("WAITING") {
        "WARN"
    } else {
        "INFO"
    }
}

fn runtime_event_level(event_type: &str) -> &'static str {
    let normalized = event_type.to_ascii_lowercase();
    if normalized.ends_with(".failed") || normalized == "run.failed" || normalized == "tool.failed"
    {
        "ERROR"
    } else if normalized.contains("warning")
        || normalized.contains(".warn")
        || normalized.contains("approval.required")
        || normalized.contains("timeout")
    {
        "WARN"
    } else {
        "INFO"
    }
}

fn non_empty_details(raw: &str) -> Option<String> {
    if raw.trim().is_empty() || raw.trim() == "{}" || raw.trim() == "null" {
        return None;
    }
    Some(redact_details(raw))
}

fn redact_details(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(mut value) => {
            redact_value(&mut value);
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| redact_text(raw))
        }
        Err(_) => redact_text(raw),
    }
}

fn redact_value(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object.iter_mut() {
                if is_sensitive_key(key) {
                    *value = Value::String("[REDACTED]".to_string());
                } else {
                    redact_value(value);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_value),
        Value::String(text) => {
            let original = text.clone();
            if let Ok(mut nested @ (Value::Object(_) | Value::Array(_))) =
                serde_json::from_str::<Value>(&original)
            {
                redact_value(&mut nested);
                *text = serde_json::to_string(&nested).unwrap_or_else(|_| redact_text(&original));
            } else {
                *text = redact_text(text);
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "key"
            | "apikey"
            | "authorization"
            | "auth"
            | "bearer"
            | "clientsecret"
            | "cookie"
            | "credential"
            | "password"
            | "privatekey"
            | "refreshtoken"
            | "secret"
            | "sessiontoken"
            | "token"
            | "accesstoken"
            | "idtoken"
            | "registryauth"
            | "sig"
            | "signature"
            | "xamzcredential"
            | "xamzsignature"
    ) || normalized.contains("authorization")
        || normalized.contains("registryauth")
        || normalized.contains("signature")
        || normalized.contains("secret")
        || normalized.contains("credential")
        || normalized.ends_with("auth")
        || normalized.ends_with("password")
        || normalized.ends_with("apikey")
}

fn redact_text(raw: &str) -> String {
    let result = redact_url_userinfo(raw);
    let result = redact_header_values(&result);
    let result = redact_query_parameters(&result);
    let mut result = result;
    for marker in [
        "Bearer ",
        "api_key=",
        "apikey=",
        "access_token=",
        "id_token=",
        "signature=",
        "sig=",
        "x-amz-signature=",
        "x-amz-credential=",
        "credential=",
        "registryAuth=",
        "token=",
        "password=",
        "secret=",
    ] {
        let marker_lower = marker.to_ascii_lowercase();
        let mut offset = 0;
        while offset < result.len() {
            let lower = result.to_ascii_lowercase();
            let Some(index) = lower[offset..].find(&marker_lower) else {
                break;
            };
            let start = offset + index + marker.len();
            let end = result[start..]
                .find(|character: char| {
                    character.is_whitespace() || matches!(character, ',' | ';' | '&' | '"' | '\'')
                })
                .map(|relative| start + relative)
                .unwrap_or(result.len());
            result.replace_range(start..end, "[REDACTED]");
            offset = start + "[REDACTED]".len();
            if offset >= result.len() {
                break;
            }
        }
    }
    result
}

fn redact_url_userinfo(raw: &str) -> String {
    let mut result = raw.to_string();
    let mut search = 0;
    while let Some(relative) = result[search..].find("://") {
        let scheme_end = search + relative;
        let userinfo_start = scheme_end + 3;
        let mut cursor = userinfo_start;
        let mut at = None;
        while cursor < result.len() {
            let character = result[cursor..].chars().next().unwrap_or_default();
            if character == '@' {
                at = Some(cursor);
                break;
            }
            if character.is_whitespace() || matches!(character, '/' | '?' | '#' | '"' | '\\' | '>')
            {
                break;
            }
            cursor += character.len_utf8();
        }
        if let Some(at) = at {
            if at > userinfo_start {
                result.replace_range(userinfo_start..at, "[REDACTED]");
                search = userinfo_start + "[REDACTED]@".len();
                continue;
            }
        }
        search = userinfo_start;
    }
    result
}

fn redact_header_values(raw: &str) -> String {
    let mut result = raw.to_string();
    for marker in ["Proxy-Authorization:", "X-Registry-Auth:", "Authorization:"] {
        let mut search = 0;
        while let Some(relative) = find_ascii_case_insensitive(&result, marker, search) {
            let marker_start = relative;
            let boundary_ok = marker_start == 0
                || result[..marker_start]
                    .chars()
                    .next_back()
                    .is_some_and(|character| {
                        !character.is_ascii_alphanumeric() && character != '_'
                    });
            if !boundary_ok {
                search = marker_start + marker.len();
                continue;
            }
            let value_start = marker_start + marker.len();
            let value_start = value_start
                + result[value_start..]
                    .chars()
                    .take_while(|character| character.is_whitespace())
                    .map(char::len_utf8)
                    .sum::<usize>();
            let opening_quote = result[value_start..]
                .chars()
                .next()
                .filter(|character| matches!(character, '"' | '\''));
            let content_start = value_start + opening_quote.map(char::len_utf8).unwrap_or(0);
            let value_end = result[content_start..]
                .find(|character: char| {
                    character == '\n'
                        || character == '\r'
                        || character == ','
                        || character == ';'
                        || opening_quote == Some(character)
                })
                .map(|relative| content_start + relative)
                .unwrap_or(result.len());
            if content_start < value_end {
                result.replace_range(content_start..value_end, "[REDACTED]");
                search = content_start + "[REDACTED]".len();
            } else {
                search = value_start;
            }
        }
    }
    result
}

fn redact_query_parameters(raw: &str) -> String {
    let mut result = raw.to_string();
    let mut cursor = 0;
    while cursor < result.len() {
        let character = result[cursor..].chars().next().unwrap_or_default();
        if !matches!(character, '?' | '&' | ';') {
            cursor += character.len_utf8();
            continue;
        }
        let key_start = cursor + character.len_utf8();
        let Some(key_end_relative) = result[key_start..].find(|value: char| {
            value == '=' || matches!(value, '&' | ';' | '#' | '"' | '\'') || value.is_whitespace()
        }) else {
            break;
        };
        let key_end = key_start + key_end_relative;
        if result[key_end..].starts_with('=') && is_sensitive_query_key(&result[key_start..key_end])
        {
            let value_start = key_end + 1;
            let opening_quote = result[value_start..]
                .chars()
                .next()
                .filter(|character| matches!(character, '"' | '\''));
            let content_start = value_start + opening_quote.map(char::len_utf8).unwrap_or(0);
            let value_end = result[content_start..]
                .find(|value: char| {
                    value.is_whitespace()
                        || matches!(value, '&' | ';' | '#' | '"' | '\'' | ',' | ')')
                        || opening_quote == Some(value)
                })
                .map(|relative| content_start + relative)
                .unwrap_or(result.len());
            if content_start < value_end {
                result.replace_range(content_start..value_end, "[REDACTED]");
                cursor = content_start + "[REDACTED]".len();
                continue;
            }
        }
        cursor = key_end;
    }
    result
}

fn is_sensitive_query_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "key"
            | "apikey"
            | "token"
            | "accesstoken"
            | "idtoken"
            | "signature"
            | "sig"
            | "xamzsignature"
            | "xamzcredential"
            | "credential"
            | "password"
            | "secret"
            | "registryauth"
            | "authorization"
            | "auth"
    ) || normalized.contains("authorization")
        || normalized.contains("signature")
        || normalized.contains("credential")
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str, from: usize) -> Option<usize> {
    haystack[from..]
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
        .map(|relative| from + relative)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_and_defaults_limit() {
        assert_eq!(effective_limit(None), 100);
        assert_eq!(effective_limit(Some(0)), 1);
        assert_eq!(effective_limit(Some(700)), 500);
        assert_eq!(source_limit(Some(500)), 2_000);
    }

    #[test]
    fn maps_failures_and_waiting_states() {
        assert_eq!(job_level("FAILED"), "ERROR");
        assert_eq!(job_level("WAITING"), "WARN");
        assert_eq!(job_level("RUNNING"), "INFO");
        assert_eq!(runtime_event_level("tool.failed"), "ERROR");
        assert_eq!(runtime_event_level("approval.required"), "WARN");
    }

    #[test]
    fn recursively_redacts_secret_fields_and_text() {
        let value = redact_details(
            r#"{"arguments":{"apiKey":"top-secret","secretValue":"hidden","registryAuth":"registry-secret","nested":[{"password":"pw"}]},"message":"Bearer abc123"}"#,
        );
        assert!(!value.contains("top-secret"));
        assert!(!value.contains("hidden"));
        assert!(!value.contains("registry-secret"));
        assert!(!value.contains("abc123"));
        assert!(value.contains("[REDACTED]"));
    }

    #[test]
    fn redacts_url_headers_and_presigned_query_values() {
        let value = redact_details(
            "https://alice:password@example.test/path?foo=ok&Signature=signature-value&x-amz-credential=credential-value\nProxy-Authorization: proxy-secret\nX-Registry-Auth: registry-header\nAuthorization: Bearer header-secret",
        );
        assert!(value.contains("https://[REDACTED]@example.test"));
        assert!(value.contains("Signature=[REDACTED]"));
        assert!(value.contains("x-amz-credential=[REDACTED]"));
        assert!(value.contains("Proxy-Authorization: [REDACTED]"));
        assert!(value.contains("X-Registry-Auth: [REDACTED]"));
        assert!(value.contains("Authorization: [REDACTED]"));
        assert!(!value.contains("password@example"));
        assert!(!value.contains("signature-value"));
        assert!(!value.contains("proxy-secret"));
        assert!(!value.contains("registry-header"));
        assert!(!value.contains("header-secret"));
    }

    #[test]
    fn exception_only_filters_before_truncation() {
        let query = SystemLogsQuery {
            exception_only: true,
            ..SystemLogsQuery::default()
        };
        let mut entries = Vec::new();
        push_if_matches(
            &mut entries,
            SystemLogEntry {
                id: "info".to_string(),
                timestamp: String::new(),
                level: "INFO".to_string(),
                source: "JOB".to_string(),
                employee_id: None,
                employee_name: None,
                message: String::new(),
                details: None,
                job_id: None,
                runtime_run_id: None,
                event_type: None,
                model: None,
            },
            &query,
        );
        assert!(entries.is_empty());
    }

    #[test]
    fn sanitizes_container_status_errors() {
        let status = sanitize_container_status(ContainerRuntimeStatus {
            enabled: true,
            available: false,
            engine_version: None,
            default_image: "image".to_string(),
            last_error: Some("Authorization: Bearer container-secret".to_string()),
        });
        assert_eq!(status.default_image, "image");
        assert_eq!(
            status.last_error.as_deref(),
            Some("Authorization: [REDACTED]")
        );
    }

    #[tokio::test]
    async fn exception_only_pushes_source_predicate_before_limit() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database should open");
        sqlx::query("CREATE TABLE employees (id TEXT, company_id TEXT, name TEXT)")
            .execute(&pool)
            .await
            .expect("employees table should be created");
        sqlx::query(
            "CREATE TABLE agent_jobs (
                id TEXT, company_id TEXT, job_type TEXT, status TEXT, employee_id TEXT,
                runtime_run_id TEXT, result TEXT, error TEXT, payload TEXT, updated_at TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("agent jobs table should be created");
        sqlx::query(
            "INSERT INTO employees (id, company_id, name) VALUES
             ('employee-1', 'company-1', '目标员工'),
             ('employee-2', 'company-1', '其他员工')",
        )
        .execute(&pool)
        .await
        .expect("employee should be inserted");

        let mut transaction = pool.begin().await.expect("transaction should begin");
        for index in 0..2_001 {
            sqlx::query(
                "INSERT INTO agent_jobs
                 (id, company_id, job_type, status, employee_id, payload, updated_at)
                 VALUES (?, 'company-1', 'MESSAGE_RESPONSE', 'COMPLETED', 'employee-2', '{}', ?)",
            )
            .bind(format!("info-{index}"))
            .bind(format!("2026-08-12T{:04}:00:00Z", index % 24_00))
            .execute(&mut *transaction)
            .await
            .expect("informational job should be inserted");
        }
        sqlx::query(
            "INSERT INTO agent_jobs
             (id, company_id, job_type, status, employee_id, error, payload, updated_at)
             VALUES ('failed-old', 'company-1', 'MESSAGE_RESPONSE', 'FAILED', 'employee-1', 'registry timeout', '{}', '2020-01-01T00:00:00Z')",
        )
        .execute(&mut *transaction)
        .await
        .expect("failed job should be inserted");
        transaction
            .commit()
            .await
            .expect("transaction should commit");

        let entries = collect(
            &pool,
            "company-1",
            &SystemLogsQuery {
                limit: Some(1),
                source: Some("JOB".to_string()),
                employee_id: Some("employee-1".to_string()),
                ..SystemLogsQuery::default()
            },
        )
        .await
        .expect("system logs should aggregate");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "job:failed-old");
        assert_eq!(entries[0].level, "ERROR");

        let exception_entries = collect(
            &pool,
            "company-1",
            &SystemLogsQuery {
                limit: Some(1),
                source: Some("JOB".to_string()),
                exception_only: true,
                ..SystemLogsQuery::default()
            },
        )
        .await
        .expect("exception-only logs should aggregate");
        assert_eq!(exception_entries.len(), 1);
        assert_eq!(exception_entries[0].id, "job:failed-old");
    }
}
