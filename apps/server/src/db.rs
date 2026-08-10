use std::{collections::HashMap, str::FromStr, time::Duration};

use chrono::Utc;
use sqlx::sqlite::{SqliteJournalMode, SqlitePoolOptions};
use sqlx::{sqlite::SqliteConnectOptions, FromRow, Row, SqlitePool};
use uuid::Uuid;

use crate::appearance::create_appearance;
use crate::model_profiles;
use crate::models::{
    AgentJobSummary, AgentMemoryRecord, AgentModelConfig, AgentMovementSummary, Company, Employee,
    GoalSummary, MeetingParticipantSummary, MeetingSummary, MessageRecord, ModelProfileSummary,
    MovementPoint, PersonalityProfile, SkillProfile, TaskSummary, TokenUsageSummary,
};
use crate::office;

#[derive(Debug, FromRow)]
struct EmployeeRow {
    id: String,
    company_id: String,
    name: String,
    age: i64,
    gender: String,
    role: String,
    department: String,
    title: String,
    status: String,
    personality: String,
    skills: String,
    appearance: String,
    model_config: String,
    model_profile_id: Option<String>,
    desk_id: Option<String>,
    hired_at: String,
    current_state: String,
    current_destination: Option<String>,
    activity: String,
    movement: String,
    location_id: Option<String>,
    position_x: f64,
    position_y: f64,
    room_id: Option<String>,
    device: String,
    posture: String,
    presence: String,
    active_movement_id: Option<String>,
    unread_count: i64,
    input_tokens: i64,
    output_tokens: i64,
    cache_tokens: i64,
    total_tokens: i64,
}

#[derive(Debug, FromRow)]
struct CompanyRow {
    id: String,
    name: String,
    status: String,
    created_at: String,
}

pub struct LlmCallInput<'a> {
    pub company_id: &'a str,
    pub employee_id: &'a str,
    pub job_id: &'a str,
    pub model_profile_id: &'a str,
    pub model: &'a str,
    pub status: &'a str,
    pub usage: &'a TokenUsageSummary,
    pub latency_ms: i64,
    pub error: Option<&'a str>,
}

pub async fn initialize(
    database_url: &str,
) -> Result<SqlitePool, Box<dyn std::error::Error + Send + Sync>> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    seed_defaults(&pool).await?;
    Ok(pool)
}

pub async fn seed_defaults(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let office_layout = office::default_office();
    sqlx::query(
        "INSERT OR IGNORE INTO companies (id, name, status, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind("company-01")
    .bind("星河软件公司 · AI Runtime")
    .bind("active")
    .bind(&now)
    .execute(pool)
    .await?;

    sqlx::query(
        "UPDATE companies SET name = '星河软件公司 · AI Runtime'
         WHERE id = 'company-01' AND name = '星河软件公司 · Demo Office'",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO model_profiles
         (id, company_id, name, protocol, base_url, encrypted_api_key, api_key_hint,
          model, enabled, created_at, updated_at)
         VALUES ('mock-profile-01', ?, 'Mock Office Provider', 'mock', '', NULL, NULL,
                 'mock-office-v1', 1, ?, ?)",
    )
    .bind("company-01")
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    for employee in default_employees() {
        let personality =
            serde_json::to_string(&employee.personality).expect("personality is serializable");
        let skills = serde_json::to_string(&employee.skills).expect("skills are serializable");
        let appearance =
            serde_json::to_string(&employee.appearance).expect("appearance is serializable");
        let model_config =
            serde_json::to_string(&employee.model_config).expect("model config is serializable");
        sqlx::query(
            "INSERT OR IGNORE INTO employees
             (id, company_id, name, age, gender, role, department, title, status,
              personality, skills, appearance, model_config, model_profile_id,
              desk_id, hired_at, current_state)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&employee.id)
        .bind(&employee.company_id)
        .bind(&employee.name)
        .bind(i64::from(employee.age))
        .bind(&employee.gender)
        .bind(&employee.role)
        .bind(&employee.department)
        .bind(&employee.title)
        .bind(&employee.status)
        .bind(personality)
        .bind(skills)
        .bind(appearance)
        .bind(model_config)
        .bind("mock-profile-01")
        .bind(&employee.desk_id)
        .bind(&employee.hired_at)
        .bind(&employee.current_state)
        .execute(pool)
        .await?;
    }

    for location in &office_layout.locations {
        sqlx::query(
            "INSERT OR IGNORE INTO office_locations
             (id, company_id, name, type, room_id, x, y, created_at)
             VALUES (?, 'company-01', ?, ?, ?, ?, ?, ?)",
        )
        .bind(&location.id)
        .bind(&location.name)
        .bind(&location.location_type)
        .bind(&location.room_id)
        .bind(f64::from(location.x))
        .bind(f64::from(location.y))
        .bind(&now)
        .execute(pool)
        .await?;
    }

    for employee in default_employees() {
        let location_id = office::workstation_location_id(&employee.id);
        if let Some(location) = office_layout
            .locations
            .iter()
            .find(|item| item.id == location_id)
        {
            sqlx::query(
                "UPDATE employees
                 SET location_id = COALESCE(location_id, ?),
                     position_x = CASE WHEN position_x = 0 THEN ? ELSE position_x END,
                     position_y = CASE WHEN position_y = 0 THEN ? ELSE position_y END,
                     room_id = COALESCE(room_id, ?),
                     activity = CASE WHEN activity = 'IDLE' THEN 'IDLE' ELSE activity END,
                     movement = CASE WHEN movement = 'STATIONARY' THEN 'STATIONARY' ELSE movement END,
                     current_destination = CASE WHEN active_movement_id IS NULL THEN NULL ELSE current_destination END,
                     device = CASE WHEN device = '' THEN 'NONE' ELSE device END,
                     posture = CASE WHEN posture = '' THEN 'SITTING' ELSE posture END,
                     presence = CASE WHEN presence = '' THEN 'AVAILABLE' ELSE presence END
                 WHERE company_id = 'company-01' AND id = ?",
            )
            .bind(&location.id)
            .bind(f64::from(location.x))
            .bind(f64::from(location.y))
            .bind(&location.room_id)
            .bind(&employee.id)
            .execute(pool)
            .await?;
        }
    }

    sqlx::query(
        "UPDATE employees SET model_profile_id = 'mock-profile-01'
         WHERE company_id = 'company-01' AND model_profile_id IS NULL",
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO goals
         (id, company_id, title, description, status, created_at, updated_at)
         VALUES ('goal-office-runtime', ?, '让 Agent 公司持续运行',
                 '让消息、模型调用和员工状态在浏览器关闭后仍能继续运行。',
                 'ACTIVE', ?, ?)",
    )
    .bind("company-01")
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    sqlx::query(
        "INSERT OR IGNORE INTO tasks
         (id, company_id, title, description, type, status, assignee_id, depends_on,
          acceptance_criteria, created_at, updated_at, goal_id)
         VALUES ('task-runtime-message-loop', ?, '处理 Agent 消息并保存回复',
                 '由 Agent Worker 读取消息、调用绑定模型、保存回复和记忆。',
                 'backend', 'READY', 'backend-01', '[]',
                 '[\"消息可恢复\", \"回复写入数据库\", \"状态通过事件广播\"]', ?, ?,
                 'goal-office-runtime')",
    )
    .bind("company-01")
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    // Workstations and transit points are not long-lived exclusive seats.
    // Clear reservations left by an older runtime after the agent has already
    // arrived so a supervisor can visit that workstation and future moves do
    // not get blocked by stale reservations.
    sqlx::query(
        "DELETE FROM location_reservations
         WHERE company_id = 'company-01'
           AND location_id IN (
               SELECT id FROM office_locations
               WHERE company_id = 'company-01'
                 AND type NOT IN ('MEETING_SEAT', 'INTERVIEW_SEAT')
           )
           AND EXISTS (
               SELECT 1 FROM employees
               WHERE employees.company_id = location_reservations.company_id
                 AND employees.id = location_reservations.agent_id
                 AND employees.location_id = location_reservations.location_id
                 AND employees.active_movement_id IS NULL
           )",
    )
    .execute(pool)
    .await?;

    // A direct chat move may temporarily reserve a meeting/interview seat.
    // Once the agent is no longer there and no active workflow owns that
    // seat, the reservation must not survive a restart and block a meeting.
    sqlx::query(
        "DELETE FROM location_reservations
         WHERE company_id = 'company-01'
           AND NOT EXISTS (
               SELECT 1
               FROM meeting_participants mp
               JOIN meetings m ON m.id = mp.meeting_id
               WHERE m.company_id = location_reservations.company_id
                 AND m.status IN ('CREATED', 'WAITING_FOR_PARTICIPANTS', 'IN_PROGRESS')
                 AND mp.agent_id = location_reservations.agent_id
                 AND mp.seat_location_id = location_reservations.location_id
                 AND mp.status IN ('INVITED', 'ON_THE_WAY', 'ARRIVED', 'WAITING', 'JOINED')
           )
           AND NOT EXISTS (
               SELECT 1 FROM agent_movements am
               WHERE am.company_id = location_reservations.company_id
                 AND am.agent_id = location_reservations.agent_id
                 AND am.to_location_id = location_reservations.location_id
                 AND am.movement_status IN ('PENDING', 'MOVING')
           )
           AND NOT EXISTS (
               SELECT 1 FROM employees
               WHERE employees.company_id = location_reservations.company_id
                 AND employees.location_id = location_reservations.location_id
                 AND employees.active_movement_id IS NULL
           )",
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn company(pool: &SqlitePool, company_id: &str) -> Result<Company, sqlx::Error> {
    let row = sqlx::query_as::<_, CompanyRow>(
        "SELECT id, name, status, created_at FROM companies WHERE id = ?",
    )
    .bind(company_id)
    .fetch_one(pool)
    .await?;
    Ok(Company {
        id: row.id,
        name: row.name,
        status: row.status,
        created_at: row.created_at,
    })
}

pub async fn record_llm_call(
    pool: &SqlitePool,
    input: LlmCallInput<'_>,
) -> Result<String, sqlx::Error> {
    let id = format!("llm-call-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO llm_calls
         (id, company_id, employee_id, job_id, model_profile_id, model, status,
          input_tokens, output_tokens, cache_tokens, total_tokens, latency_ms, error, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(input.company_id)
    .bind(input.employee_id)
    .bind(input.job_id)
    .bind(input.model_profile_id)
    .bind(input.model)
    .bind(input.status)
    .bind(input.usage.input_tokens)
    .bind(input.usage.output_tokens)
    .bind(input.usage.cache_tokens)
    .bind(input.usage.total_tokens)
    .bind(input.latency_ms)
    .bind(input.error)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn company_token_usage(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<TokenUsageSummary, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COALESCE(SUM(input_tokens), 0) AS input_tokens,
                COALESCE(SUM(output_tokens), 0) AS output_tokens,
                COALESCE(SUM(cache_tokens), 0) AS cache_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens
         FROM llm_calls WHERE company_id = ? AND status = 'COMPLETED'",
    )
    .bind(company_id)
    .fetch_one(pool)
    .await?;
    token_usage_from_row(&row)
}

pub async fn employee_token_usage(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<TokenUsageSummary, sqlx::Error> {
    let row = sqlx::query(
        "SELECT COALESCE(SUM(input_tokens), 0) AS input_tokens,
                COALESCE(SUM(output_tokens), 0) AS output_tokens,
                COALESCE(SUM(cache_tokens), 0) AS cache_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens
         FROM llm_calls
         WHERE company_id = ? AND employee_id = ? AND status = 'COMPLETED'",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_one(pool)
    .await?;
    token_usage_from_row(&row)
}

fn token_usage_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<TokenUsageSummary, sqlx::Error> {
    Ok(TokenUsageSummary {
        input_tokens: row.try_get("input_tokens")?,
        output_tokens: row.try_get("output_tokens")?,
        cache_tokens: row.try_get("cache_tokens")?,
        total_tokens: row.try_get("total_tokens")?,
    })
}

pub async fn employees(pool: &SqlitePool, company_id: &str) -> Result<Vec<Employee>, sqlx::Error> {
    let profiles = model_profiles::list(pool, company_id)
        .await
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?
        .into_iter()
        .map(|profile| (profile.id.clone(), profile))
        .collect::<HashMap<_, _>>();
    let rows = sqlx::query_as::<_, EmployeeRow>(
        "SELECT id, company_id, name, age, gender, role, department, title, status,
                personality, skills, appearance, model_config, model_profile_id, desk_id,
                hired_at, current_state, current_destination,
                activity, movement, location_id, position_x, position_y, room_id,
                device, posture, presence, active_movement_id,
                (SELECT COUNT(*) FROM messages m
                 WHERE m.company_id = employees.company_id
                   AND m.from_type = 'employee'
                   AND m.from_id = employees.id
                   AND m.to_type = 'user'
                   AND m.to_id = 'user'
                   AND m.read_at IS NULL) AS unread_count,
                COALESCE(usage.input_tokens, 0) AS input_tokens,
                COALESCE(usage.output_tokens, 0) AS output_tokens,
                COALESCE(usage.cache_tokens, 0) AS cache_tokens,
                COALESCE(usage.total_tokens, 0) AS total_tokens
         FROM employees
         LEFT JOIN (
             SELECT employee_id,
                    SUM(input_tokens) AS input_tokens,
                    SUM(output_tokens) AS output_tokens,
                    SUM(cache_tokens) AS cache_tokens,
                    SUM(total_tokens) AS total_tokens
             FROM llm_calls
             WHERE company_id = ? AND status = 'COMPLETED'
             GROUP BY employee_id
         ) usage ON usage.employee_id = employees.id
         WHERE employees.company_id = ? ORDER BY hired_at, id",
    )
    .bind(company_id)
    .bind(company_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| employee_from_row(row, &profiles))
        .collect())
}

pub async fn set_employee_model_profile(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
    profile_id: Option<&str>,
) -> Result<(), sqlx::Error> {
    if let Some(profile_id) = profile_id {
        sqlx::query("SELECT id FROM model_profiles WHERE id = ? AND company_id = ?")
            .bind(profile_id)
            .bind(company_id)
            .fetch_one(pool)
            .await?;
    }
    sqlx::query(
        "UPDATE employees SET model_profile_id = ?
         WHERE id = ? AND company_id = ?",
    )
    .bind(profile_id)
    .bind(employee_id)
    .bind(company_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn tasks(pool: &SqlitePool, company_id: &str) -> Result<Vec<TaskSummary>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, title, description, type, status, assignee_id, goal_id
         FROM tasks WHERE company_id = ? ORDER BY created_at",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(TaskSummary {
                id: row.try_get("id")?,
                description: row.try_get("description")?,
                title: row.try_get("title")?,
                task_type: row.try_get("type")?,
                status: row.try_get("status")?,
                assignee_id: row.try_get("assignee_id")?,
                goal_id: row.try_get("goal_id")?,
            })
        })
        .collect()
}

pub async fn meetings(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<Vec<MeetingSummary>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, title, type, room_id, participant_ids, status
         FROM meetings WHERE company_id = ? ORDER BY updated_at DESC",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    let mut meetings = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id")?;
        let participant_ids =
            serde_json::from_str::<Vec<String>>(&row.try_get::<String, _>("participant_ids")?)
                .unwrap_or_default();
        let participant_rows = sqlx::query(
            "SELECT agent_id, seat_location_id, status
             FROM meeting_participants WHERE meeting_id = ? ORDER BY agent_id",
        )
        .bind(&id)
        .fetch_all(pool)
        .await?;
        let participants = participant_rows
            .into_iter()
            .map(|participant| {
                Ok(MeetingParticipantSummary {
                    agent_id: participant.try_get("agent_id")?,
                    seat_location_id: participant.try_get("seat_location_id")?,
                    status: participant.try_get("status")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        meetings.push(MeetingSummary {
            id,
            title: row.try_get("title")?,
            meeting_type: row.try_get("type")?,
            room_id: row.try_get("room_id")?,
            participant_ids,
            participants,
            status: row.try_get("status")?,
        });
    }
    Ok(meetings)
}

pub async fn goals(pool: &SqlitePool, company_id: &str) -> Result<Vec<GoalSummary>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, title, description, status FROM goals
         WHERE company_id = ? ORDER BY created_at",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(GoalSummary {
                id: row.try_get("id")?,
                title: row.try_get("title")?,
                description: row.try_get("description")?,
                status: row.try_get("status")?,
            })
        })
        .collect()
}

pub async fn messages(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<Vec<MessageRecord>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, company_id, from_type, from_id, to_type, to_id, content,
                created_at, read_at, job_id
         FROM messages
         WHERE company_id = ?
         ORDER BY created_at DESC, id DESC
         LIMIT 500",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    let mut messages = rows
        .into_iter()
        .map(|row| {
            Ok(MessageRecord {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                from_type: row.try_get("from_type")?,
                from_id: row.try_get("from_id")?,
                to_type: row.try_get("to_type")?,
                to_id: row.try_get("to_id")?,
                content: row.try_get("content")?,
                created_at: row.try_get("created_at")?,
                read_at: row.try_get("read_at")?,
                job_id: row.try_get("job_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    messages.reverse();
    Ok(messages)
}

pub async fn messages_for_employee(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<Vec<MessageRecord>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, company_id, from_type, from_id, to_type, to_id, content,
                created_at, read_at, job_id
         FROM messages
         WHERE company_id = ?
           AND ((from_id = ? AND to_type = 'user') OR (from_type = 'user' AND to_id = ?))
         ORDER BY created_at DESC, id DESC
         LIMIT 200",
    )
    .bind(company_id)
    .bind(employee_id)
    .bind(employee_id)
    .fetch_all(pool)
    .await?;
    let mut messages = rows
        .into_iter()
        .map(|row| {
            Ok(MessageRecord {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                from_type: row.try_get("from_type")?,
                from_id: row.try_get("from_id")?,
                to_type: row.try_get("to_type")?,
                to_id: row.try_get("to_id")?,
                content: row.try_get("content")?,
                created_at: row.try_get("created_at")?,
                read_at: row.try_get("read_at")?,
                job_id: row.try_get("job_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    messages.reverse();
    Ok(messages)
}

pub async fn messages_for_employee_page(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
    limit: u32,
    before: Option<&str>,
) -> Result<Vec<MessageRecord>, sqlx::Error> {
    let limit = i64::from(limit.clamp(1, 100));
    let rows = match before {
        Some(before) => {
            sqlx::query(
                "SELECT id, company_id, from_type, from_id, to_type, to_id, content,
                        created_at, read_at, job_id
                 FROM messages
                 WHERE company_id = ?
                   AND ((from_id = ? AND to_type = 'user') OR (from_type = 'user' AND to_id = ?))
                   AND created_at < ?
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?",
            )
            .bind(company_id)
            .bind(employee_id)
            .bind(employee_id)
            .bind(before)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query(
                "SELECT id, company_id, from_type, from_id, to_type, to_id, content,
                        created_at, read_at, job_id
                 FROM messages
                 WHERE company_id = ?
                   AND ((from_id = ? AND to_type = 'user') OR (from_type = 'user' AND to_id = ?))
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?",
            )
            .bind(company_id)
            .bind(employee_id)
            .bind(employee_id)
            .bind(limit)
            .fetch_all(pool)
            .await?
        }
    };

    let mut messages = rows
        .into_iter()
        .map(|row| {
            Ok(MessageRecord {
                id: row.try_get("id")?,
                company_id: row.try_get("company_id")?,
                from_type: row.try_get("from_type")?,
                from_id: row.try_get("from_id")?,
                to_type: row.try_get("to_type")?,
                to_id: row.try_get("to_id")?,
                content: row.try_get("content")?,
                created_at: row.try_get("created_at")?,
                read_at: row.try_get("read_at")?,
                job_id: row.try_get("job_id")?,
            })
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;
    messages.reverse();
    Ok(messages)
}

pub async fn unread_counts(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<HashMap<String, i64>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT from_id, COUNT(*) AS unread_count FROM messages
         WHERE company_id = ? AND from_type = 'employee' AND to_type = 'user'
           AND to_id = 'user' AND read_at IS NULL GROUP BY from_id",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| Ok((row.try_get("from_id")?, row.try_get("unread_count")?)))
        .collect()
}

pub async fn mark_messages_read(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE messages SET read_at = ?
         WHERE company_id = ? AND from_type = 'employee' AND from_id = ?
           AND to_type = 'user' AND to_id = 'user' AND read_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(company_id)
    .bind(employee_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn memory(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<Vec<AgentMemoryRecord>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, employee_id, kind, content, created_at, updated_at
         FROM agent_memory WHERE company_id = ? AND employee_id = ?
         ORDER BY updated_at DESC LIMIT 100",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(AgentMemoryRecord {
                id: row.try_get("id")?,
                employee_id: row.try_get("employee_id")?,
                kind: row.try_get("kind")?,
                content: row.try_get("content")?,
                created_at: row.try_get("created_at")?,
                updated_at: row.try_get("updated_at")?,
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct MovementArrival {
    pub movement: AgentMovementSummary,
    pub agent_id: String,
    pub to_location_id: String,
    pub room_id: Option<String>,
    pub posture: String,
}

#[derive(Debug, FromRow)]
struct AgentMovementRow {
    id: String,
    company_id: String,
    agent_id: String,
    from_location_id: String,
    to_location_id: String,
    path: String,
    started_at: String,
    estimated_arrival_at: String,
    movement_status: String,
    created_at: String,
    completed_at: Option<String>,
}

pub async fn active_movements(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<Vec<AgentMovementSummary>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AgentMovementRow>(
        "SELECT id, company_id, agent_id, from_location_id, to_location_id, path,
                started_at, estimated_arrival_at, movement_status, created_at, completed_at
         FROM agent_movements
         WHERE company_id = ? AND movement_status IN ('PENDING', 'MOVING')
         ORDER BY created_at, id",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(movement_from_row).collect()
}

pub async fn pending_movement_ids(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id FROM agent_movements WHERE movement_status = 'PENDING'
         ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(|row| row.try_get("id")).collect()
}

pub async fn pending_or_active_movement_for_agent(
    pool: &SqlitePool,
    company_id: &str,
    agent_id: &str,
) -> Result<Option<AgentMovementSummary>, sqlx::Error> {
    let row = sqlx::query_as::<_, AgentMovementRow>(
        "SELECT id, company_id, agent_id, from_location_id, to_location_id, path,
                started_at, estimated_arrival_at, movement_status, created_at, completed_at
         FROM agent_movements
         WHERE company_id = ? AND agent_id = ? AND movement_status IN ('PENDING', 'MOVING')
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(agent_id)
    .fetch_optional(pool)
    .await?;
    row.map(movement_from_row).transpose()
}

#[allow(clippy::too_many_arguments)]
pub async fn create_movement(
    pool: &SqlitePool,
    company_id: &str,
    agent_id: &str,
    from_location_id: &str,
    to_location_id: &str,
    path: &[MovementPoint],
    started_at: &str,
    estimated_arrival_at: &str,
    reserved_until: &str,
    reason: &str,
) -> Result<AgentMovementSummary, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "DELETE FROM location_reservations
         WHERE company_id = ? AND reserved_until <= ?",
    )
    .bind(company_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let target_exists =
        sqlx::query("SELECT id FROM office_locations WHERE company_id = ? AND id = ?")
            .bind(company_id)
            .bind(to_location_id)
            .fetch_optional(&mut *tx)
            .await?
            .is_some();
    if !target_exists {
        return Err(sqlx::Error::Protocol(format!(
            "destination location does not exist: {to_location_id}"
        )));
    }

    if let Some(row) = sqlx::query(
        "SELECT agent_id FROM location_reservations
         WHERE company_id = ? AND location_id = ?",
    )
    .bind(company_id)
    .bind(to_location_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        let reserved_agent: String = row.try_get("agent_id")?;
        if reserved_agent != agent_id {
            return Err(sqlx::Error::Protocol(format!(
                "destination location is reserved: {to_location_id}"
            )));
        }
    }

    // Leaving a transient location releases this agent's old reservation.
    // Active meeting/interview workflows keep their seat reservation until
    // the workflow explicitly changes participant status.
    sqlx::query(
        "DELETE FROM location_reservations
         WHERE company_id = ? AND location_id = ? AND agent_id = ?
           AND NOT EXISTS (
               SELECT 1
               FROM meeting_participants mp
               JOIN meetings m ON m.id = mp.meeting_id
               WHERE m.company_id = location_reservations.company_id
                 AND m.status IN ('CREATED', 'WAITING_FOR_PARTICIPANTS', 'IN_PROGRESS')
                 AND mp.agent_id = location_reservations.agent_id
                 AND mp.seat_location_id = location_reservations.location_id
                 AND mp.status IN ('INVITED', 'ON_THE_WAY', 'ARRIVED', 'WAITING', 'JOINED')
           )",
    )
    .bind(company_id)
    .bind(from_location_id)
    .bind(agent_id)
    .execute(&mut *tx)
    .await?;

    let id = format!("movement-{}", Uuid::new_v4());
    let path_json = serde_json::to_string(path)
        .map_err(|error| sqlx::Error::Protocol(format!("serialize movement path: {error}")))?;
    sqlx::query(
        "INSERT INTO location_reservations
         (id, company_id, location_id, agent_id, reserved_until, reason, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(company_id, location_id) DO UPDATE SET
           agent_id = excluded.agent_id,
           reserved_until = excluded.reserved_until,
           reason = excluded.reason",
    )
    .bind(format!("reservation-{company_id}-{to_location_id}"))
    .bind(company_id)
    .bind(to_location_id)
    .bind(agent_id)
    .bind(reserved_until)
    .bind(reason)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO agent_movements
         (id, company_id, agent_id, from_location_id, to_location_id, path,
          started_at, estimated_arrival_at, movement_status, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'PENDING', ?)",
    )
    .bind(&id)
    .bind(company_id)
    .bind(agent_id)
    .bind(from_location_id)
    .bind(to_location_id)
    .bind(&path_json)
    .bind(started_at)
    .bind(estimated_arrival_at)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE employees
         SET movement = 'PREPARING', posture = 'STANDING', active_movement_id = ?,
             current_destination = ?
         WHERE company_id = ? AND id = ?",
    )
    .bind(&id)
    .bind(serde_json::json!({ "kind": "location", "id": to_location_id }).to_string())
    .bind(company_id)
    .bind(agent_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(AgentMovementSummary {
        id,
        company_id: company_id.to_string(),
        agent_id: agent_id.to_string(),
        from_location_id: from_location_id.to_string(),
        to_location_id: to_location_id.to_string(),
        path: path.to_vec(),
        started_at: started_at.to_string(),
        estimated_arrival_at: estimated_arrival_at.to_string(),
        movement_status: "PENDING".to_string(),
        created_at: now,
        completed_at: None,
    })
}

pub async fn start_movement(
    pool: &SqlitePool,
    movement_id: &str,
) -> Result<Option<AgentMovementSummary>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query_as::<_, AgentMovementRow>(
        "SELECT id, company_id, agent_id, from_location_id, to_location_id, path,
                started_at, estimated_arrival_at, movement_status, created_at, completed_at
         FROM agent_movements WHERE id = ? AND movement_status = 'PENDING'",
    )
    .bind(movement_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    sqlx::query(
        "UPDATE agent_movements SET movement_status = 'MOVING' WHERE id = ? AND movement_status = 'PENDING'",
    )
    .bind(movement_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE employees SET movement = 'WALKING', posture = 'WALKING'
         WHERE id = ? AND company_id = ?",
    )
    .bind(&row.agent_id)
    .bind(&row.company_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let mut movement = movement_from_row(row)?;
    movement.movement_status = "MOVING".to_string();
    Ok(Some(movement))
}

pub async fn due_movements(pool: &SqlitePool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id FROM agent_movements
         WHERE movement_status = 'MOVING' AND estimated_arrival_at <= ?
         ORDER BY estimated_arrival_at, created_at",
    )
    .bind(Utc::now().to_rfc3339())
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(|row| row.try_get("id")).collect()
}

pub async fn complete_movement(
    pool: &SqlitePool,
    movement_id: &str,
) -> Result<Option<MovementArrival>, sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    let row = sqlx::query_as::<_, AgentMovementRow>(
        "SELECT id, company_id, agent_id, from_location_id, to_location_id, path,
                started_at, estimated_arrival_at, movement_status, created_at, completed_at
         FROM agent_movements WHERE id = ? AND movement_status = 'MOVING'",
    )
    .bind(movement_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let target = sqlx::query(
        "SELECT type, room_id, x, y FROM office_locations WHERE company_id = ? AND id = ?",
    )
    .bind(&row.company_id)
    .bind(&row.to_location_id)
    .fetch_one(&mut *tx)
    .await?;
    let location_type: String = target.try_get("type")?;
    let posture = if matches!(
        location_type.as_str(),
        "WORKSTATION" | "MEETING_SEAT" | "INTERVIEW_SEAT"
    ) {
        "SITTING"
    } else {
        "STANDING"
    };
    let room_id: Option<String> = target.try_get("room_id")?;
    let x: f64 = target.try_get("x")?;
    let y: f64 = target.try_get("y")?;
    sqlx::query(
        "UPDATE agent_movements SET movement_status = 'ARRIVED', completed_at = ?
         WHERE id = ? AND movement_status = 'MOVING'",
    )
    .bind(&now)
    .bind(movement_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE employees
         SET location_id = ?, position_x = ?, position_y = ?, room_id = ?,
             movement = 'ARRIVED', posture = ?, active_movement_id = NULL,
             current_destination = NULL
         WHERE id = ? AND company_id = ?",
    )
    .bind(&row.to_location_id)
    .bind(x)
    .bind(y)
    .bind(&room_id)
    .bind(posture)
    .bind(&row.agent_id)
    .bind(&row.company_id)
    .execute(&mut *tx)
    .await?;
    if !matches!(location_type.as_str(), "MEETING_SEAT" | "INTERVIEW_SEAT") {
        sqlx::query(
            "DELETE FROM location_reservations
             WHERE company_id = ? AND location_id = ? AND agent_id = ?",
        )
        .bind(&row.company_id)
        .bind(&row.to_location_id)
        .bind(&row.agent_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    let mut movement = movement_from_row(row)?;
    movement.movement_status = "ARRIVED".to_string();
    movement.completed_at = Some(now);
    Ok(Some(MovementArrival {
        agent_id: movement.agent_id.clone(),
        to_location_id: movement.to_location_id.clone(),
        room_id,
        posture: posture.to_string(),
        movement,
    }))
}

pub async fn release_location_reservation(
    pool: &SqlitePool,
    company_id: &str,
    location_id: &str,
    agent_id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM location_reservations
         WHERE company_id = ? AND location_id = ? AND agent_id = ?",
    )
    .bind(company_id)
    .bind(location_id)
    .bind(agent_id)
    .execute(pool)
    .await?;
    Ok(())
}

fn movement_from_row(row: AgentMovementRow) -> Result<AgentMovementSummary, sqlx::Error> {
    let path = serde_json::from_str::<Vec<MovementPoint>>(&row.path)
        .map_err(|error| sqlx::Error::Protocol(format!("invalid movement path: {error}")))?;
    Ok(AgentMovementSummary {
        id: row.id,
        company_id: row.company_id,
        agent_id: row.agent_id,
        from_location_id: row.from_location_id,
        to_location_id: row.to_location_id,
        path,
        started_at: row.started_at,
        estimated_arrival_at: row.estimated_arrival_at,
        movement_status: row.movement_status,
        created_at: row.created_at,
        completed_at: row.completed_at,
    })
}

#[derive(Clone, Debug)]
pub struct AgentJob {
    pub id: String,
    pub company_id: String,
    pub job_type: String,
    pub status: String,
    pub employee_id: String,
    pub goal_id: Option<String>,
    pub task_id: Option<String>,
    pub payload: String,
    pub attempts: i64,
}

#[derive(Debug, FromRow)]
struct AgentJobRow {
    id: String,
    company_id: String,
    job_type: String,
    status: String,
    employee_id: String,
    message_id: Option<String>,
    goal_id: Option<String>,
    task_id: Option<String>,
    payload: String,
    result: Option<String>,
    error: Option<String>,
    attempts: i64,
    available_at: String,
    created_at: String,
    updated_at: String,
}

pub async fn jobs(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<Vec<AgentJobSummary>, sqlx::Error> {
    let rows = sqlx::query_as::<_, AgentJobRow>(
        "SELECT id, company_id, job_type, status, employee_id, message_id, goal_id, task_id,
                payload, result, error, attempts, available_at, created_at, updated_at
         FROM agent_jobs WHERE company_id = ? ORDER BY created_at DESC LIMIT 100",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(job_summary).collect())
}

pub async fn create_message_job(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
    content: &str,
) -> Result<(MessageRecord, AgentJob), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT id FROM employees WHERE company_id = ? AND id = ?")
        .bind(company_id)
        .bind(employee_id)
        .fetch_one(&mut *tx)
        .await?;
    let task = sqlx::query(
        "SELECT id, goal_id FROM tasks WHERE company_id = ? AND assignee_id = ?
         AND status IN ('READY', 'IN_PROGRESS', 'PENDING') ORDER BY created_at LIMIT 1",
    )
    .bind(company_id)
    .bind(employee_id)
    .fetch_optional(&mut *tx)
    .await?;
    let task_id = task
        .as_ref()
        .and_then(|row| row.try_get::<Option<String>, _>("id").ok())
        .flatten();
    let goal_id = task
        .as_ref()
        .and_then(|row| row.try_get::<Option<String>, _>("goal_id").ok())
        .flatten();
    let now = Utc::now().to_rfc3339();
    let message_id = format!("msg-{}", Uuid::new_v4());
    let job_id = format!("job-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO messages
         (id, company_id, from_type, from_id, to_type, to_id, content, created_at, job_id)
         VALUES (?, ?, 'user', 'user', 'employee', ?, ?, ?, ?)",
    )
    .bind(&message_id)
    .bind(company_id)
    .bind(employee_id)
    .bind(content)
    .bind(&now)
    .bind(&job_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO agent_jobs
         (id, company_id, job_type, status, employee_id, message_id, goal_id, task_id,
          payload, attempts, available_at, created_at, updated_at)
         VALUES (?, ?, 'MESSAGE_RESPONSE', 'PENDING', ?, ?, ?, ?, ?, 0, ?, ?, ?)",
    )
    .bind(&job_id)
    .bind(company_id)
    .bind(employee_id)
    .bind(&message_id)
    .bind(&goal_id)
    .bind(&task_id)
    .bind(serde_json::json!({ "messageId": message_id.clone(), "content": content }).to_string())
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((
        MessageRecord {
            id: message_id.clone(),
            company_id: company_id.to_string(),
            from_type: "user".to_string(),
            from_id: "user".to_string(),
            to_type: "employee".to_string(),
            to_id: employee_id.to_string(),
            content: content.to_string(),
            created_at: now.clone(),
            read_at: None,
            job_id: Some(job_id.clone()),
        },
        AgentJob {
            id: job_id,
            company_id: company_id.to_string(),
            job_type: "MESSAGE_RESPONSE".to_string(),
            status: "PENDING".to_string(),
            employee_id: employee_id.to_string(),
            goal_id,
            task_id,
            payload: serde_json::json!({ "content": content }).to_string(),
            attempts: 0,
        },
    ))
}

pub async fn recover_running_jobs(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE agent_jobs SET status = 'PENDING', updated_at = ?
         WHERE status IN ('RUNNING', 'WAITING')",
    )
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn claim_next_job(pool: &SqlitePool) -> Result<Option<AgentJob>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row = sqlx::query_as::<_, AgentJobRow>(
        "SELECT id, company_id, job_type, status, employee_id, message_id, goal_id, task_id,
                payload, result, error, attempts, available_at, created_at, updated_at
         FROM agent_jobs WHERE status = 'PENDING' AND available_at <= ?
         ORDER BY created_at LIMIT 1",
    )
    .bind(Utc::now().to_rfc3339())
    .fetch_optional(&mut *tx)
    .await?;
    let Some(row) = row else {
        tx.commit().await?;
        return Ok(None);
    };
    let result = sqlx::query(
        "UPDATE agent_jobs SET status = 'RUNNING', attempts = attempts + 1, updated_at = ?
         WHERE id = ? AND status = 'PENDING'",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(&row.id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    Ok(Some(AgentJob {
        id: row.id,
        company_id: row.company_id,
        job_type: row.job_type,
        status: "RUNNING".to_string(),
        employee_id: row.employee_id,
        goal_id: row.goal_id,
        task_id: row.task_id,
        payload: row.payload,
        attempts: row.attempts + 1,
    }))
}

pub async fn finish_job(
    pool: &SqlitePool,
    job_id: &str,
    status: &str,
    result: Option<&str>,
    error: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE agent_jobs SET status = ?, result = ?, error = ?, updated_at = ? WHERE id = ?",
    )
    .bind(status)
    .bind(result)
    .bind(error)
    .bind(Utc::now().to_rfc3339())
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn save_memory(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
    content: &str,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO agent_memory
         (id, company_id, employee_id, kind, content, created_at, updated_at)
         VALUES (?, ?, ?, 'conversation_summary', ?, ?, ?)",
    )
    .bind(format!("memory-{}", Uuid::new_v4()))
    .bind(company_id)
    .bind(employee_id)
    .bind(content)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn insert_agent_reply(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
    content: &str,
    job_id: &str,
) -> Result<MessageRecord, sqlx::Error> {
    let id = format!("msg-{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO messages
         (id, company_id, from_type, from_id, to_type, to_id, content, created_at, job_id)
         VALUES (?, ?, 'employee', ?, 'user', 'user', ?, ?, ?)",
    )
    .bind(&id)
    .bind(company_id)
    .bind(employee_id)
    .bind(content)
    .bind(&now)
    .bind(job_id)
    .execute(pool)
    .await?;
    Ok(MessageRecord {
        id,
        company_id: company_id.to_string(),
        from_type: "employee".to_string(),
        from_id: employee_id.to_string(),
        to_type: "user".to_string(),
        to_id: "user".to_string(),
        content: content.to_string(),
        created_at: now,
        read_at: None,
        job_id: Some(job_id.to_string()),
    })
}

pub async fn agent_reply_for_job(
    pool: &SqlitePool,
    job_id: &str,
) -> Result<Option<MessageRecord>, sqlx::Error> {
    let row = sqlx::query(
        "SELECT id, company_id, from_type, from_id, to_type, to_id, content,
                created_at, read_at, job_id
         FROM messages
         WHERE job_id = ? AND from_type = 'employee' AND to_type = 'user'
         ORDER BY created_at LIMIT 1",
    )
    .bind(job_id)
    .fetch_optional(pool)
    .await?;
    row.map(|row| {
        Ok(MessageRecord {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            from_type: row.try_get("from_type")?,
            from_id: row.try_get("from_id")?,
            to_type: row.try_get("to_type")?,
            to_id: row.try_get("to_id")?,
            content: row.try_get("content")?,
            created_at: row.try_get("created_at")?,
            read_at: row.try_get("read_at")?,
            job_id: row.try_get("job_id")?,
        })
    })
    .transpose()
}

fn job_summary(row: AgentJobRow) -> AgentJobSummary {
    let _ = (&row.company_id, &row.payload, &row.available_at);
    AgentJobSummary {
        id: row.id,
        job_type: row.job_type,
        status: row.status,
        employee_id: row.employee_id,
        message_id: row.message_id,
        goal_id: row.goal_id,
        task_id: row.task_id,
        attempts: row.attempts,
        result: row.result,
        error: row.error,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn employee_from_row(
    row: EmployeeRow,
    profiles: &HashMap<String, ModelProfileSummary>,
) -> Employee {
    Employee {
        id: row.id,
        company_id: row.company_id,
        name: row.name,
        age: row.age.clamp(0, 255) as u8,
        gender: row.gender,
        role: row.role,
        department: row.department,
        title: row.title,
        status: row.status,
        personality: serde_json::from_str(&row.personality).unwrap_or_default(),
        skills: serde_json::from_str(&row.skills).unwrap_or_default(),
        appearance: serde_json::from_str(&row.appearance).unwrap_or_default(),
        model_config: serde_json::from_str(&row.model_config).unwrap_or_default(),
        model_profile: row
            .model_profile_id
            .as_ref()
            .and_then(|profile_id| profiles.get(profile_id).cloned()),
        model_profile_id: row.model_profile_id,
        desk_id: row.desk_id,
        hired_at: row.hired_at,
        current_state: row.current_state,
        current_destination: row
            .current_destination
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok()),
        activity: row.activity,
        movement: row.movement,
        location_id: row.location_id,
        x: row.position_x as f32,
        y: row.position_y as f32,
        room_id: row.room_id,
        device: row.device,
        posture: row.posture,
        presence: row.presence,
        active_movement_id: row.active_movement_id,
        unread_count: row.unread_count,
        token_usage: TokenUsageSummary {
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
            cache_tokens: row.cache_tokens,
            total_tokens: row.total_tokens,
        },
    }
}

struct EmployeeSeed {
    id: String,
    company_id: String,
    name: String,
    age: u8,
    gender: String,
    role: String,
    department: String,
    title: String,
    status: String,
    personality: PersonalityProfile,
    skills: Vec<SkillProfile>,
    appearance: crate::models::AppearanceProfile,
    model_config: AgentModelConfig,
    desk_id: Option<String>,
    hired_at: String,
    current_state: String,
}

fn default_employees() -> Vec<EmployeeSeed> {
    let now = Utc::now().to_rfc3339();
    vec![
        seed(
            "supervisor-01",
            "林屿",
            36,
            "male",
            "主管 / Supervisor",
            "管理层",
            "Supervisor",
            "冷静、克制、结果导向",
            &["调度", "仲裁", "验收", "Review"],
            1001,
            "desk-supervisor-01",
            &now,
        ),
        seed(
            "hr-01",
            "苏晴",
            31,
            "female",
            "HR",
            "People",
            "HR & 招聘",
            "外向、细致、善于观察",
            &["招聘", "面试组织", "人员档案"],
            1002,
            "desk-hr-01",
            &now,
        ),
        seed(
            "product-01",
            "周启",
            33,
            "male",
            "产品经理",
            "Product",
            "Product Manager",
            "好奇、结构化、表达直接",
            &["PRD", "需求拆解", "验收标准"],
            1003,
            "desk-product-01",
            &now,
        ),
        seed(
            "ui-01",
            "沈知夏",
            28,
            "female",
            "UI/UX 设计师",
            "Design",
            "UI/UX Designer",
            "开放、审美敏锐、温和",
            &["页面结构", "交互", "视觉规范"],
            1004,
            "desk-ui-01",
            &now,
        ),
        seed(
            "frontend-01",
            "陈默",
            30,
            "male",
            "前端工程师",
            "Engineering",
            "Frontend Engineer",
            "专注、务实、低社交",
            &["Web", "React", "UI 代码实现"],
            1005,
            "desk-frontend-01",
            &now,
        ),
        seed(
            "backend-01",
            "顾言",
            34,
            "male",
            "后端工程师",
            "Engineering",
            "Backend Engineer",
            "严谨、谨慎、系统思维",
            &["API", "Rust", "数据库", "服务端代码"],
            1006,
            "desk-backend-01",
            &now,
        ),
        seed(
            "qa-01",
            "唐宁",
            27,
            "female",
            "测试工程师",
            "Quality",
            "QA Engineer",
            "怀疑精神强、耐心",
            &["测试", "回归", "缺陷验证"],
            1007,
            "desk-qa-01",
            &now,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn seed(
    id: &str,
    name: &str,
    age: u8,
    gender: &str,
    role: &str,
    department: &str,
    title: &str,
    traits: &str,
    skill_names: &[&str],
    appearance_seed: u64,
    desk_id: &str,
    hired_at: &str,
) -> EmployeeSeed {
    let openness = 62 + (appearance_seed as u8 % 25);
    let conscientiousness = 70 + (appearance_seed as u8 % 20);
    let skills = skill_names
        .iter()
        .enumerate()
        .map(|(index, skill)| SkillProfile {
            name: (*skill).to_string(),
            level: 78 + ((appearance_seed as usize + index * 7) % 20) as u8,
        })
        .collect();
    EmployeeSeed {
        id: id.to_string(),
        company_id: "company-01".to_string(),
        name: name.to_string(),
        age,
        gender: gender.to_string(),
        role: role.to_string(),
        department: department.to_string(),
        title: title.to_string(),
        status: "online".to_string(),
        personality: PersonalityProfile {
            openness,
            conscientiousness,
            extraversion: 45 + (appearance_seed as u8 % 40),
            agreeableness: 58 + (appearance_seed as u8 % 30),
            emotional_stability: 68 + (appearance_seed as u8 % 25),
            traits: traits.split('、').map(str::to_string).collect(),
            communication_style: "先确认约束，再给出可执行结论".to_string(),
            decision_style: "依据证据和验收条件推进".to_string(),
        },
        skills,
        appearance: create_appearance(appearance_seed, gender, age),
        model_config: AgentModelConfig {
            provider: "mock".to_string(),
            model: "mock-office-v1".to_string(),
            mode: "mock".to_string(),
        },
        desk_id: Some(desk_id.to_string()),
        hired_at: hired_at.to_string(),
        current_state: "IDLE".to_string(),
    }
}
