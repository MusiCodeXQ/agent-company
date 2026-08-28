use std::{collections::HashSet, time::Instant};

use anyhow::{anyhow, Result};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    agent_runtime::llm::{self, ChatMessage},
    domain::{
        appearance::create_appearance,
        models::{
            CandidateSummary, Employee, FloorRoleSummary, FloorSummary, HiringPositionSummary,
            HiringRequestSummary, PersonalityProfile, SkillProfile, WorkstationSummary,
        },
        office,
    },
    infrastructure::model_profiles,
};

#[derive(Clone, Debug)]
pub struct HiringCreation {
    pub request: HiringRequestSummary,
    pub floor_id: String,
}

#[derive(Clone, Debug)]
pub struct HiringPersonalizationResult {
    pub request: HiringRequestSummary,
    pub generation_mode: String,
    pub fallback_reason: Option<String>,
}

pub(crate) const FALLBACK_ADMINISTRATOR_UNAVAILABLE: &str = "administrator_unavailable";
pub(crate) const FALLBACK_MODEL_CONFIG_UNAVAILABLE: &str = "administrator_model_config_unavailable";
pub(crate) const FALLBACK_MODEL_REQUEST_FAILED: &str = "administrator_model_request_failed";
pub(crate) const FALLBACK_USAGE_RECORD_FAILED: &str = "administrator_model_usage_record_failed";
pub(crate) const FALLBACK_MODEL_OUTPUT_INVALID: &str = "administrator_model_output_invalid";
pub(crate) const FALLBACK_GENERATION_SERVICE_UNAVAILABLE: &str = "generation_service_unavailable";

impl HiringPersonalizationResult {
    pub fn mock(request: HiringRequestSummary) -> Self {
        Self {
            request,
            generation_mode: "mock".to_string(),
            fallback_reason: None,
        }
    }

    pub fn fallback(request: HiringRequestSummary, reason: &str) -> Self {
        Self {
            request,
            generation_mode: "mock_fallback".to_string(),
            fallback_reason: Some(reason.to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct HireDraft {
    pub employee_id: String,
    pub floor_id: String,
    pub workstation_id: String,
    pub location_id: String,
}

#[derive(Clone, Debug)]
pub struct AdministratorPatrolResult {
    pub reply: String,
    pub risky_job_ids: Vec<String>,
    pub cancelled_job_ids: Vec<String>,
}

pub fn administrator_patrol_intent(content: &str) -> bool {
    [
        "巡查",
        "死循环",
        "任务死锁",
        "卡住",
        "token消耗",
        "Token消耗",
        "谁在消耗",
    ]
    .iter()
    .any(|marker| content.contains(marker))
}

pub fn administrator_intervention_intent(content: &str) -> bool {
    ["解除死循环", "停止卡住", "取消异常", "解决死锁", "中止异常"]
        .iter()
        .any(|marker| content.contains(marker))
}

pub async fn administrator_patrol(
    pool: &SqlitePool,
    company_id: &str,
    floor_id: &str,
    current_job_id: &str,
    intervene: bool,
) -> Result<AdministratorPatrolResult> {
    let employees = crate::infrastructure::db::employees(pool, company_id)
        .await?
        .into_iter()
        .filter(|employee| employee.floor_id.as_deref() == Some(floor_id))
        .filter(|employee| employee.employment_status == "ACTIVE")
        .collect::<Vec<_>>();
    let employee_ids = employees
        .iter()
        .map(|employee| employee.id.as_str())
        .collect::<HashSet<_>>();
    let cutoff = Utc::now() - Duration::minutes(15);
    let jobs = crate::infrastructure::db::jobs(pool, company_id).await?;
    let risky_jobs = jobs
        .iter()
        .filter(|job| employee_ids.contains(job.employee_id.as_str()))
        .filter(|job| {
            job.id != current_job_id && matches!(job.status.as_str(), "RUNNING" | "WAITING")
        })
        .filter(|job| {
            job.attempts >= 3
                || DateTime::parse_from_rfc3339(&job.updated_at)
                    .map(|updated_at| updated_at.with_timezone(&Utc) < cutoff)
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let risky_job_ids = risky_jobs
        .iter()
        .map(|job| job.id.clone())
        .collect::<Vec<_>>();
    let mut cancelled_job_ids = Vec::new();
    if intervene {
        let now = Utc::now().to_rfc3339();
        for job in &risky_jobs {
            let changed = sqlx::query(
                "UPDATE agent_jobs
                 SET status = 'CANCELLED', error = 'administrator intervention: suspected dead loop or stalled job', updated_at = ?
                 WHERE id = ? AND company_id = ? AND status IN ('RUNNING', 'WAITING')",
            )
            .bind(&now)
            .bind(&job.id)
            .bind(company_id)
            .execute(pool)
            .await?
            .rows_affected();
            if changed > 0 {
                cancelled_job_ids.push(job.id.clone());
            }
        }
    }
    let mut employee_lines = Vec::new();
    for employee in &employees {
        let usage =
            crate::infrastructure::db::employee_token_usage(pool, company_id, &employee.id).await?;
        let active_jobs = jobs
            .iter()
            .filter(|job| job.employee_id == employee.id)
            .filter(|job| matches!(job.status.as_str(), "PENDING" | "RUNNING" | "WAITING"))
            .count();
        employee_lines.push(format!(
            "{}：{} token（in {} / out {} / cache {}），活跃 Job {}",
            employee.name,
            usage.total_tokens,
            usage.input_tokens,
            usage.output_tokens,
            usage.cache_tokens,
            active_jobs
        ));
    }
    let conclusion = if risky_job_ids.is_empty() {
        "当前没有发现重试过多或超过 15 分钟未更新的活跃 Job。".to_string()
    } else if intervene {
        format!(
            "发现 {} 个疑似卡死 Job，已取消 {} 个；员工会等待澄清后的任务再继续。",
            risky_job_ids.len(),
            cancelled_job_ids.len()
        )
    } else {
        format!(
            "发现 {} 个疑似卡死 Job。若要立即干预，请明确说“解除死循环并停止卡住任务”。",
            risky_job_ids.len()
        )
    };
    Ok(AdministratorPatrolResult {
        reply: format!(
            "本层巡查结果：\n{}\n{}",
            employee_lines.join("\n"),
            conclusion
        ),
        risky_job_ids,
        cancelled_job_ids,
    })
}

pub async fn floors(pool: &SqlitePool, company_id: &str) -> Result<Vec<FloorSummary>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT f.id, f.company_id, f.floor_number, f.name, f.description, f.mission,
                f.responsibilities, f.status, f.created_at, f.updated_at,
                (SELECT COUNT(*) FROM employees e
                 WHERE e.company_id = f.company_id AND e.floor_id = f.id
                   AND e.employment_status != 'TERMINATED') AS employee_count,
                (SELECT COUNT(*) FROM workstations w
                 WHERE w.company_id = f.company_id AND w.floor_id = f.id) AS workstation_count,
                (SELECT COUNT(*) FROM workstations w
                 WHERE w.company_id = f.company_id AND w.floor_id = f.id
                   AND w.status = 'AVAILABLE') AS available_workstations
         FROM floors f
         WHERE f.company_id = ?
         ORDER BY f.floor_number, f.id",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let floor_id: String = row.try_get("id")?;
        let role_rows = sqlx::query(
            "SELECT id, name, responsibilities, skills, preferences
             FROM role_definitions WHERE company_id = ? AND floor_id = ? ORDER BY created_at, id",
        )
        .bind(company_id)
        .bind(&floor_id)
        .fetch_all(pool)
        .await?;
        let roles = role_rows
            .into_iter()
            .map(|role| {
                Ok(FloorRoleSummary {
                    id: role.try_get("id")?,
                    name: role.try_get("name")?,
                    responsibilities: json_vec(role.try_get("responsibilities")?),
                    skills: json_vec(role.try_get("skills")?),
                    preferences: json_vec(role.try_get("preferences")?),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        result.push(floor_from_row(row, roles)?);
    }
    Ok(result)
}

pub async fn floor(pool: &SqlitePool, company_id: &str, floor_id: &str) -> Result<FloorSummary> {
    floors(pool, company_id)
        .await?
        .into_iter()
        .find(|item| item.id == floor_id)
        .ok_or_else(|| anyhow!("floor was not found: {floor_id}"))
}

pub async fn workstations(
    pool: &SqlitePool,
    company_id: &str,
    floor_id: &str,
) -> Result<Vec<WorkstationSummary>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, company_id, floor_id, location_id, room_id, position_x, position_y,
                orientation, status, assigned_employee_id, created_at, updated_at
         FROM workstations
         WHERE company_id = ? AND floor_id = ?
         ORDER BY position_y, position_x, id",
    )
    .bind(company_id)
    .bind(floor_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(workstation_from_row).collect()
}

pub async fn create_floor(
    pool: &SqlitePool,
    company_id: &str,
    name: &str,
    description: Option<&str>,
    mission: Option<&str>,
) -> Result<FloorSummary> {
    let name = clean_required(name, "floor name")?;
    let mut tx = pool.begin().await?;
    let floor_number: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(floor_number), 0) + 1 FROM floors WHERE company_id = ?",
    )
    .bind(company_id)
    .fetch_one(&mut *tx)
    .await?;
    let floor_id = format!("floor-{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    let description = description.unwrap_or("").trim();
    let mission = mission.unwrap_or("").trim();
    sqlx::query(
        "INSERT INTO floors
         (id, company_id, floor_number, name, description, mission, responsibilities,
          status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, '[]', 'ACTIVE', ?, ?)",
    )
    .bind(&floor_id)
    .bind(company_id)
    .bind(floor_number)
    .bind(&name)
    .bind(description)
    .bind(mission)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for location in office::floor_locations(&floor_id) {
        sqlx::query(
            "INSERT INTO office_locations
             (id, company_id, name, type, room_id, x, y, floor_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&location.id)
        .bind(company_id)
        .bind(&location.name)
        .bind(&location.location_type)
        .bind(&location.room_id)
        .bind(f64::from(location.x))
        .bind(f64::from(location.y))
        .bind(&floor_id)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    floor(pool, company_id, &floor_id).await
}

pub async fn ensure_company_administrator(pool: &SqlitePool) -> Result<()> {
    let rows = sqlx::query(
        "SELECT c.id AS company_id, f.id, f.floor_number, f.name
         FROM companies c
         JOIN floors f ON f.id = (
           SELECT first_floor.id FROM floors first_floor
           WHERE first_floor.company_id = c.id AND first_floor.status = 'ACTIVE'
           ORDER BY first_floor.floor_number, first_floor.id LIMIT 1
         )
         WHERE NOT EXISTS (
           SELECT 1 FROM employees e
           WHERE e.company_id = c.id
             AND e.is_floor_administrator = 1 AND e.employment_status = 'ACTIVE'
         ) ORDER BY c.id",
    )
    .fetch_all(pool)
    .await?;
    for row in rows {
        let company_id: String = row.try_get("company_id")?;
        let floor_id: String = row.try_get("id")?;
        let floor_number: i64 = row.try_get("floor_number")?;
        let floor_name: String = row.try_get("name")?;
        let now = Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        for location in office::floor_locations(&floor_id) {
            sqlx::query(
                "INSERT OR IGNORE INTO office_locations
                 (id, company_id, name, type, room_id, x, y, floor_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&location.id)
            .bind(&company_id)
            .bind(&location.name)
            .bind(&location.location_type)
            .bind(&location.room_id)
            .bind(f64::from(location.x))
            .bind(f64::from(location.y))
            .bind(&floor_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "DELETE FROM role_definitions WHERE company_id = ? AND floor_id = ? AND name = 'HR'",
        )
        .bind(&company_id)
        .bind(&floor_id)
        .execute(&mut *tx)
        .await?;
        let administrator_id = insert_floor_administrator(
            &mut tx,
            &company_id,
            &floor_id,
            floor_number,
            &floor_name,
            &now,
        )
        .await?;
        tx.commit().await?;
        crate::infrastructure::db::ensure_employee_runtime_binding(
            pool,
            &company_id,
            &administrator_id,
        )
        .await?;
    }
    Ok(())
}

pub async fn delete_floor(pool: &SqlitePool, company_id: &str, floor_id: &str) -> Result<()> {
    floor(pool, company_id, floor_id).await?;
    if floor_id == "floor-01" {
        return Err(anyhow!("基础办公层不能删除"));
    }
    let active_regular_employees: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM employees
         WHERE company_id = ? AND floor_id = ? AND employment_status = 'ACTIVE'
           AND is_floor_administrator = 0",
    )
    .bind(company_id)
    .bind(floor_id)
    .fetch_one(pool)
    .await?;
    if active_regular_employees > 0 {
        return Err(anyhow!("楼层仍有员工，先完成调岗或离职后再删除"));
    }
    let active_hiring: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM hiring_requests
         WHERE company_id = ? AND floor_id = ? AND status NOT IN ('CLOSED', 'CANCELLED')",
    )
    .bind(company_id)
    .bind(floor_id)
    .fetch_one(pool)
    .await?;
    if active_hiring > 0 {
        return Err(anyhow!("楼层仍有招聘请求，先关闭招聘请求后再删除"));
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE employees
         SET employment_status = 'TERMINATED', status = 'offline', presence = 'OFFLINE',
             floor_id = NULL, desk_id = NULL
         WHERE company_id = ? AND floor_id = ? AND is_floor_administrator = 1",
    )
    .bind(company_id)
    .bind(floor_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM role_definitions WHERE company_id = ? AND floor_id = ?")
        .bind(company_id)
        .bind(floor_id)
        .execute(&mut *tx)
        .await?;
    let deleted = sqlx::query("DELETE FROM floors WHERE company_id = ? AND id = ?")
        .bind(company_id)
        .bind(floor_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        tx.rollback().await?;
        return Err(anyhow!("floor was not found: {floor_id}"));
    }
    tx.commit().await?;
    Ok(())
}

pub async fn update_floor(
    pool: &SqlitePool,
    company_id: &str,
    floor_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    mission: Option<&str>,
    responsibilities: Option<&[String]>,
) -> Result<FloorSummary> {
    let current = floor(pool, company_id, floor_id).await?;
    let next_name = match name {
        Some(value) => clean_required(value, "floor name")?,
        None => current.name,
    };
    let next_description = description.unwrap_or(&current.description);
    let next_mission = mission.unwrap_or(&current.mission);
    let next_responsibilities = responsibilities
        .map(serde_json::to_string)
        .transpose()?
        .unwrap_or(serde_json::to_string(&current.responsibilities)?);
    sqlx::query(
        "UPDATE floors SET name = ?, description = ?, mission = ?, responsibilities = ?,
         updated_at = ? WHERE company_id = ? AND id = ?",
    )
    .bind(next_name)
    .bind(next_description.trim())
    .bind(next_mission.trim())
    .bind(next_responsibilities)
    .bind(Utc::now().to_rfc3339())
    .bind(company_id)
    .bind(floor_id)
    .execute(pool)
    .await?;
    floor(pool, company_id, floor_id).await
}

pub async fn create_workstation(
    pool: &SqlitePool,
    company_id: &str,
    floor_id: &str,
    x: Option<f32>,
    y: Option<f32>,
    room_id: Option<&str>,
    orientation: Option<&str>,
) -> Result<WorkstationSummary> {
    floor(pool, company_id, floor_id).await?;
    let existing = workstations(pool, company_id, floor_id).await?;
    let index = existing.len();
    let position_x = x.unwrap_or(330.0 + (index % 4) as f32 * 205.0);
    let position_y = y.unwrap_or(180.0 + (index / 4) as f32 * 270.0);
    if !(80.0..=1350.0).contains(&position_x) || !(80.0..=700.0).contains(&position_y) {
        return Err(anyhow!("workstation position is outside the office"));
    }
    let room_id = room_id
        .map(str::to_string)
        .unwrap_or_else(|| office::floor_room_id(floor_id, "open-office"));
    let id = format!("workstation-{}", Uuid::new_v4());
    let location_id = id.clone();
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO office_locations
         (id, company_id, name, type, room_id, x, y, floor_id, created_at)
         VALUES (?, ?, ?, 'WORKSTATION', ?, ?, ?, ?, ?)",
    )
    .bind(&location_id)
    .bind(company_id)
    .bind(format!("{} 工位", floor_id))
    .bind(&room_id)
    .bind(f64::from(position_x))
    .bind(f64::from(position_y))
    .bind(floor_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO workstations
         (id, company_id, floor_id, location_id, room_id, position_x, position_y,
          orientation, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'AVAILABLE', ?, ?)",
    )
    .bind(&id)
    .bind(company_id)
    .bind(floor_id)
    .bind(&location_id)
    .bind(&room_id)
    .bind(f64::from(position_x))
    .bind(f64::from(position_y))
    .bind(orientation.unwrap_or("DOWN"))
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    workstations(pool, company_id, floor_id)
        .await?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| anyhow!("workstation disappeared after creation"))
}

pub async fn workstation_layout_inputs(
    pool: &SqlitePool,
    company_id: &str,
    floor_id: &str,
) -> Result<Vec<office::WorkstationLayoutInput>, sqlx::Error> {
    Ok(workstations(pool, company_id, floor_id)
        .await?
        .into_iter()
        .map(|item| office::WorkstationLayoutInput {
            id: item.id,
            location_id: item.location_id,
            assigned_employee_id: item.assigned_employee_id,
            room_id: item.room_id,
            x: item.x,
            y: item.y,
        })
        .collect())
}

pub async fn hiring_requests(
    pool: &SqlitePool,
    company_id: &str,
) -> Result<Vec<HiringRequestSummary>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, company_id, floor_id, requester_id, summary, status,
                source_message_id, created_at, updated_at
         FROM hiring_requests WHERE company_id = ? ORDER BY created_at DESC",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let request_id: String = row.try_get("id")?;
        let position_rows = sqlx::query(
            "SELECT id, role_definition_id, title, headcount, requirements, preferences, status
             FROM hiring_positions WHERE hiring_request_id = ? ORDER BY created_at",
        )
        .bind(&request_id)
        .fetch_all(pool)
        .await?;
        let candidate_rows = sqlx::query(
            "SELECT id, hiring_request_id, position_id, name, age, gender, title,
                    personality, skills, work_style, strengths, weaknesses, expertise,
                    expected_model, agent_runtime, appearance, match_score,
                    interview_evaluation, status, hired_employee_id, created_at, updated_at
             FROM candidates WHERE company_id = ? AND hiring_request_id = ?
             ORDER BY position_id, match_score DESC, created_at",
        )
        .bind(company_id)
        .bind(&request_id)
        .fetch_all(pool)
        .await?;
        let positions = position_rows
            .into_iter()
            .map(position_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let candidates = candidate_rows
            .into_iter()
            .map(candidate_from_row)
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        result.push(HiringRequestSummary {
            id: request_id,
            company_id: row.try_get("company_id")?,
            floor_id: row.try_get("floor_id")?,
            requester_id: row.try_get("requester_id")?,
            summary: row.try_get("summary")?,
            status: row.try_get("status")?,
            source_message_id: row.try_get("source_message_id")?,
            positions,
            candidates,
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(result)
}

pub async fn create_hiring_from_message(
    pool: &SqlitePool,
    company_id: &str,
    requester_id: &str,
    source_message_id: Option<&str>,
    content: &str,
) -> Result<HiringCreation> {
    if let Some(source_message_id) = source_message_id {
        if let Some(existing) = hiring_requests(pool, company_id)
            .await?
            .into_iter()
            .find(|request| request.source_message_id.as_deref() == Some(source_message_id))
        {
            return Ok(HiringCreation {
                floor_id: existing.floor_id.clone(),
                request: existing,
            });
        }
    }
    let floor = resolve_floor(pool, company_id, content).await?;
    let position_specs = parse_position_specs(content);
    if position_specs.is_empty() {
        return Err(anyhow!("无法从招聘需求中识别岗位和人数"));
    }
    let now = Utc::now().to_rfc3339();
    let request_id = format!("hiring-request-{}", Uuid::new_v4());
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO hiring_requests
         (id, company_id, floor_id, requester_id, summary, status, source_message_id,
          created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 'CANDIDATES_READY', ?, ?, ?)",
    )
    .bind(&request_id)
    .bind(company_id)
    .bind(&floor.id)
    .bind(requester_id)
    .bind(content.trim())
    .bind(source_message_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    for (position_index, spec) in position_specs.iter().enumerate() {
        let role_id = find_or_create_role(&mut tx, company_id, &floor.id, spec, &now).await?;
        let position_id = format!("hiring-position-{}", Uuid::new_v4());
        sqlx::query(
            "INSERT INTO hiring_positions
             (id, hiring_request_id, role_definition_id, title, headcount, requirements,
              preferences, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'CANDIDATES_READY', ?, ?)",
        )
        .bind(&position_id)
        .bind(&request_id)
        .bind(&role_id)
        .bind(&spec.title)
        .bind(i64::from(spec.headcount))
        .bind(serde_json::to_string(&spec.requirements)?)
        .bind(serde_json::to_string(&spec.preferences)?)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        insert_candidate_set(
            &mut tx,
            company_id,
            &request_id,
            &position_id,
            spec,
            position_index,
            &now,
        )
        .await?;
    }
    tx.commit().await?;
    let request = hiring_requests(pool, company_id)
        .await?
        .into_iter()
        .find(|item| item.id == request_id)
        .ok_or_else(|| anyhow!("hiring request disappeared after creation"))?;
    Ok(HiringCreation {
        request,
        floor_id: floor.id,
    })
}

pub async fn refresh_candidates(
    pool: &SqlitePool,
    company_id: &str,
    request_id: &str,
) -> Result<HiringRequestSummary> {
    let mut tx = pool.begin().await?;
    let positions = sqlx::query(
        "SELECT id, title, headcount, requirements, preferences
         FROM hiring_positions WHERE hiring_request_id = ? ORDER BY created_at, id",
    )
    .bind(request_id)
    .fetch_all(&mut *tx)
    .await?;
    if positions.is_empty() {
        return Err(anyhow!("hiring request has no positions"));
    }
    let belongs: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM hiring_requests WHERE id = ? AND company_id = ?")
            .bind(request_id)
            .bind(company_id)
            .fetch_one(&mut *tx)
            .await?;
    if belongs == 0 {
        return Err(anyhow!("hiring request was not found"));
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE candidates SET status = 'REJECTED', updated_at = ?
         WHERE company_id = ? AND hiring_request_id = ? AND status = 'AVAILABLE'",
    )
    .bind(&now)
    .bind(company_id)
    .bind(request_id)
    .execute(&mut *tx)
    .await?;
    for (index, row) in positions.into_iter().enumerate() {
        let spec = PositionSpec {
            title: row.try_get("title")?,
            headcount: row.try_get::<i64, _>("headcount")?.clamp(1, 20) as u8,
            requirements: json_vec(row.try_get("requirements")?),
            preferences: json_vec(row.try_get("preferences")?),
        };
        let position_id: String = row.try_get("id")?;
        insert_candidate_set(
            &mut tx,
            company_id,
            request_id,
            &position_id,
            &spec,
            index,
            &now,
        )
        .await?;
    }
    sqlx::query(
        "UPDATE hiring_requests SET status = 'CANDIDATES_READY', updated_at = ? WHERE id = ?",
    )
    .bind(&now)
    .bind(request_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    hiring_requests(pool, company_id)
        .await?
        .into_iter()
        .find(|request| request.id == request_id)
        .ok_or_else(|| anyhow!("hiring request disappeared after refresh"))
}

/// Generate the final candidate profiles through the company administrator's
/// configured model. Both the chat-triggered flow and the HTTP refresh flow
/// call this function so Mock and real-model behavior cannot drift apart.
///
/// `job_id` is supplied by the background worker when the request came from a
/// persisted Agent job. HTTP refreshes intentionally leave it empty because
/// `llm_calls.job_id` is nullable; the call is still attributed to the active
/// administrator and its model profile.
pub async fn personalize_hiring_candidates(
    pool: &sqlx::SqlitePool,
    company_id: &str,
    request_id: &str,
    preferred_administrator_id: Option<&str>,
    job_id: Option<&str>,
) -> Result<HiringPersonalizationResult> {
    let request = hiring_requests(pool, company_id)
        .await?
        .into_iter()
        .find(|request| request.id == request_id)
        .ok_or_else(|| anyhow!("hiring request was not found"))?;
    let administrator = match active_company_administrator(
        pool,
        company_id,
        preferred_administrator_id.or(request.requester_id.as_deref()),
    )
    .await
    {
        Ok(administrator) => administrator,
        Err(_error) => {
            tracing::warn!(
                request_id,
                fallback_reason = FALLBACK_ADMINISTRATOR_UNAVAILABLE,
                "administrator model candidate personalization fell back to deterministic candidates"
            );
            return Ok(HiringPersonalizationResult::fallback(
                request,
                FALLBACK_ADMINISTRATOR_UNAVAILABLE,
            ));
        }
    };
    let Some(profile_id) = administrator.model_profile_id.as_deref() else {
        return Ok(HiringPersonalizationResult::mock(request));
    };
    let profile = match model_profiles::get_credentials(pool, profile_id).await {
        Ok(profile) => profile,
        Err(_error) => {
            tracing::warn!(
                request_id,
                fallback_reason = FALLBACK_MODEL_CONFIG_UNAVAILABLE,
                "administrator model candidate personalization fell back to deterministic candidates"
            );
            return Ok(HiringPersonalizationResult::fallback(
                request,
                FALLBACK_MODEL_CONFIG_UNAVAILABLE,
            ));
        }
    };
    if profile.protocol.eq_ignore_ascii_case("mock") {
        return Ok(HiringPersonalizationResult::mock(request));
    }

    let available_candidates = request
        .candidates
        .iter()
        .filter(|candidate| candidate.status == "AVAILABLE")
        .collect::<Vec<_>>();
    let candidate_template = serde_json::to_string(&available_candidates)?;
    let system_prompt = hiring_personalization_system_prompt();
    let user_prompt = format!(
        "招聘需求：{}\n请重写以下候选人模板，每个岗位严格保留三位候选人：\n{}",
        request.summary, candidate_template
    );
    let started = Instant::now();
    let response = match llm::chat(
        &profile,
        system_prompt,
        &[ChatMessage {
            role: "user".to_string(),
            content: user_prompt,
        }],
    )
    .await
    {
        Ok(response) => response,
        Err(_error) => {
            let fallback_reason = FALLBACK_MODEL_REQUEST_FAILED;
            let empty_usage = crate::domain::models::TokenUsageSummary::default();
            if let Err(record_error) = crate::infrastructure::db::record_llm_call(
                pool,
                crate::infrastructure::db::LlmCallInput {
                    company_id,
                    employee_id: &administrator.id,
                    job_id,
                    model_profile_id: profile_id,
                    model: &profile.model,
                    status: "FAILED",
                    usage: &empty_usage,
                    latency_ms: i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX),
                    error: Some(fallback_reason),
                },
            )
            .await
            {
                tracing::warn!(
                    request_id,
                    ?record_error,
                    "unable to persist failed hiring personalization usage"
                );
            }
            tracing::warn!(
                request_id,
                fallback_reason,
                "administrator model candidate personalization fell back to deterministic candidates"
            );
            return Ok(HiringPersonalizationResult::fallback(
                request,
                fallback_reason,
            ));
        }
    };
    let latency_ms = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    if let Err(record_error) = crate::infrastructure::db::record_llm_call(
        pool,
        crate::infrastructure::db::LlmCallInput {
            company_id,
            employee_id: &administrator.id,
            job_id,
            model_profile_id: profile_id,
            model: &profile.model,
            status: "COMPLETED",
            usage: &response.usage,
            latency_ms,
            error: None,
        },
    )
    .await
    {
        tracing::warn!(
            request_id,
            ?record_error,
            fallback_reason = FALLBACK_USAGE_RECORD_FAILED,
            "administrator model candidate personalization fell back to deterministic candidates"
        );
        return Ok(HiringPersonalizationResult::fallback(
            request,
            FALLBACK_USAGE_RECORD_FAILED,
        ));
    }
    match apply_ai_candidate_profiles(pool, company_id, request_id, &response.content).await {
        Ok(request) => Ok(HiringPersonalizationResult {
            request,
            generation_mode: "ai".to_string(),
            fallback_reason: None,
        }),
        Err(_error) => {
            tracing::warn!(
                request_id,
                fallback_reason = FALLBACK_MODEL_OUTPUT_INVALID,
                "administrator model candidate personalization fell back to deterministic candidates"
            );
            Ok(HiringPersonalizationResult::fallback(
                request,
                FALLBACK_MODEL_OUTPUT_INVALID,
            ))
        }
    }
}

pub fn hiring_personalization_system_prompt() -> &'static str {
    "你是虚拟公司的楼层管理员，负责招聘。请根据招聘需求独立设计候选人的姓名、年龄、性别、职位、性格、技能、工作方式和外观。必须只返回 JSON 对象 {\"candidates\":[...]}，保留每个候选人的 id；字段必须完整并沿用输入 JSON 的 camelCase 结构。每个 personality 必须包含 openness、conscientiousness、extraversion、agreeableness、emotionalStability（均为 0..100）、traits（至少一项）、communicationStyle、decisionStyle、speechHabits（至少一项）和 behaviorHabits（至少一项）；workStyle、strengths、weaknesses、expertise 也必须提供。gender 只能是 male、female 或 non_binary，age 必须是 18..75 的整数，skills.level 和 matchScore 范围为 0..100，appearance 必须保留完整字段。招聘评分只能依据岗位、技能、经验、沟通表现、性格和工作方式，严禁依据年龄、性别或任何 appearance 字段评分。"
}

async fn active_company_administrator(
    pool: &sqlx::SqlitePool,
    company_id: &str,
    preferred_id: Option<&str>,
) -> Result<Employee> {
    let employees = crate::infrastructure::db::employees(pool, company_id).await?;
    if let Some(preferred_id) = preferred_id {
        if let Some(employee) = employees.iter().find(|employee| {
            employee.id == preferred_id
                && employee.is_floor_administrator
                && employee.employment_status == "ACTIVE"
        }) {
            return Ok(employee.clone());
        }
    }
    employees
        .into_iter()
        .find(|employee| employee.is_floor_administrator && employee.employment_status == "ACTIVE")
        .ok_or_else(|| anyhow!("company has no active administrator"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiCandidateSet {
    candidates: Vec<AiCandidateProfile>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiCandidateProfile {
    id: String,
    name: String,
    age: u8,
    gender: String,
    title: String,
    personality: PersonalityProfile,
    skills: Vec<SkillProfile>,
    work_style: String,
    strengths: Vec<String>,
    weaknesses: Vec<String>,
    expertise: Vec<String>,
    appearance: crate::domain::models::AppearanceProfile,
    match_score: i64,
    interview_evaluation: String,
}

pub async fn apply_ai_candidate_profiles(
    pool: &SqlitePool,
    company_id: &str,
    request_id: &str,
    model_output: &str,
) -> Result<HiringRequestSummary> {
    let json = extract_json_object(model_output)?;
    let generated: AiCandidateSet = serde_json::from_str(json)?;
    let request = hiring_requests(pool, company_id)
        .await?
        .into_iter()
        .find(|request| request.id == request_id)
        .ok_or_else(|| anyhow!("hiring request was not found"))?;
    let available_ids = request
        .candidates
        .iter()
        .filter(|candidate| candidate.status == "AVAILABLE")
        .map(|candidate| candidate.id.as_str())
        .collect::<HashSet<_>>();
    if generated.candidates.len() != available_ids.len() {
        return Err(anyhow!(
            "model must return every available candidate exactly once"
        ));
    }
    let mut seen = HashSet::new();
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    for candidate in generated.candidates {
        if !available_ids.contains(candidate.id.as_str()) || !seen.insert(candidate.id.clone()) {
            return Err(anyhow!(
                "model returned an unknown or duplicate candidate id"
            ));
        }
        validate_ai_candidate_profile(&candidate)?;
        let seed = request
            .candidates
            .iter()
            .find(|existing| existing.id == candidate.id)
            .map(|existing| existing.appearance.seed)
            .ok_or_else(|| anyhow!("candidate seed disappeared during AI profile update"))?;
        // The model may describe an appearance, but identity is owned by the
        // server: keep the persisted seed and derive all visual details from
        // the validated age/gender pair. This prevents a later AI response
        // from changing a candidate's hair or body while preserving identity.
        let appearance = create_appearance(seed, &candidate.gender, candidate.age);
        let match_score = profile_match_score(
            &candidate.skills,
            &candidate.personality,
            &candidate.work_style,
            &candidate.expertise,
        );
        sqlx::query(
            "UPDATE candidates SET name = ?, age = ?, gender = ?, title = ?, personality = ?,
             skills = ?, work_style = ?, strengths = ?, weaknesses = ?, expertise = ?,
             appearance = ?, match_score = ?, interview_evaluation = ?, updated_at = ?
             WHERE company_id = ? AND hiring_request_id = ? AND id = ? AND status = 'AVAILABLE'",
        )
        .bind(candidate.name.trim())
        .bind(i64::from(candidate.age))
        .bind(candidate.gender.trim())
        .bind(candidate.title.trim())
        .bind(serde_json::to_string(&candidate.personality)?)
        .bind(serde_json::to_string(&candidate.skills)?)
        .bind(candidate.work_style.trim())
        .bind(serde_json::to_string(&candidate.strengths)?)
        .bind(serde_json::to_string(&candidate.weaknesses)?)
        .bind(serde_json::to_string(&candidate.expertise)?)
        .bind(serde_json::to_string(&appearance)?)
        .bind(match_score)
        .bind(candidate.interview_evaluation.trim())
        .bind(&now)
        .bind(company_id)
        .bind(request_id)
        .bind(&candidate.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    hiring_requests(pool, company_id)
        .await?
        .into_iter()
        .find(|request| request.id == request_id)
        .ok_or_else(|| anyhow!("hiring request disappeared after AI profile update"))
}

const MAX_PROFILE_TEXT_LENGTH: usize = 240;
const MAX_EVALUATION_LENGTH: usize = 1_000;
const MAX_PROFILE_ITEMS: usize = 12;
const MAX_PROFILE_TOTAL_LENGTH: usize = 2_000;

fn validate_ai_candidate_profile(candidate: &AiCandidateProfile) -> Result<()> {
    if !(18..=75).contains(&candidate.age)
        || candidate.skills.is_empty()
        || candidate.skills.len() > MAX_PROFILE_ITEMS
        || candidate.match_score.clamp(0, 100) != candidate.match_score
    {
        return Err(anyhow!("model returned invalid candidate fields"));
    }
    validate_text(&candidate.name, "candidate name", MAX_PROFILE_TEXT_LENGTH)?;
    validate_text(&candidate.title, "candidate title", MAX_PROFILE_TEXT_LENGTH)?;
    if !matches!(candidate.gender.as_str(), "male" | "female" | "non_binary") {
        return Err(anyhow!("model returned invalid candidate gender"));
    }
    validate_personality(&candidate.personality)?;
    validate_appearance(&candidate.appearance)?;
    validate_text(
        &candidate.work_style,
        "candidate work style",
        MAX_PROFILE_TEXT_LENGTH,
    )?;
    if candidate
        .skills
        .iter()
        .any(|skill| skill.level > 100 || validate_text(&skill.name, "skill name", 120).is_err())
        || candidate
            .skills
            .iter()
            .map(|skill| skill.name.trim().chars().count())
            .sum::<usize>()
            > MAX_PROFILE_TOTAL_LENGTH
    {
        return Err(anyhow!("model returned invalid candidate skills"));
    }
    validate_text_list(
        &candidate.strengths,
        "candidate strengths",
        1,
        MAX_PROFILE_ITEMS,
    )?;
    validate_text_list(
        &candidate.weaknesses,
        "candidate weaknesses",
        1,
        MAX_PROFILE_ITEMS,
    )?;
    validate_text_list(
        &candidate.expertise,
        "candidate expertise",
        1,
        MAX_PROFILE_ITEMS,
    )?;
    validate_text(
        &candidate.interview_evaluation,
        "interview evaluation",
        MAX_EVALUATION_LENGTH,
    )?;
    Ok(())
}

fn validate_text(value: &str, field: &str, max_length: usize) -> Result<()> {
    let length = value.trim().chars().count();
    if length == 0 || length > max_length {
        return Err(anyhow!(
            "model returned invalid {field}: expected 1..={max_length} characters"
        ));
    }
    Ok(())
}

fn validate_text_list(
    values: &[String],
    field: &str,
    min_items: usize,
    max_items: usize,
) -> Result<()> {
    if !(min_items..=max_items).contains(&values.len())
        || values
            .iter()
            .any(|value| validate_text(value, field, MAX_PROFILE_TEXT_LENGTH).is_err())
        || values
            .iter()
            .map(|value| value.trim().chars().count())
            .sum::<usize>()
            > MAX_PROFILE_TOTAL_LENGTH
    {
        return Err(anyhow!("model returned invalid {field}"));
    }
    Ok(())
}

fn validate_personality(personality: &PersonalityProfile) -> Result<()> {
    let scores = [
        personality.openness,
        personality.conscientiousness,
        personality.extraversion,
        personality.agreeableness,
        personality.emotional_stability,
    ];
    if scores.iter().any(|score| *score > 100)
        || personality.traits.is_empty()
        || personality.traits.len() > MAX_PROFILE_ITEMS
        || personality.speech_habits.is_empty()
        || personality.speech_habits.len() > MAX_PROFILE_ITEMS
        || personality.behavior_habits.is_empty()
        || personality.behavior_habits.len() > MAX_PROFILE_ITEMS
    {
        return Err(anyhow!("model returned invalid personality fields"));
    }
    validate_text_list(
        &personality.traits,
        "personality traits",
        1,
        MAX_PROFILE_ITEMS,
    )?;
    validate_text_list(
        &personality.speech_habits,
        "speech habits",
        1,
        MAX_PROFILE_ITEMS,
    )?;
    validate_text_list(
        &personality.behavior_habits,
        "behavior habits",
        1,
        MAX_PROFILE_ITEMS,
    )?;
    validate_text(
        &personality.communication_style,
        "communication style",
        MAX_PROFILE_TEXT_LENGTH,
    )?;
    validate_text(
        &personality.decision_style,
        "decision style",
        MAX_PROFILE_TEXT_LENGTH,
    )?;
    Ok(())
}

fn validate_appearance(appearance: &crate::domain::models::AppearanceProfile) -> Result<()> {
    if !appearance.height_scale.is_finite() || !(0.5..=1.5).contains(&appearance.height_scale) {
        return Err(anyhow!("model returned invalid appearance fields"));
    }
    for (field, value) in [
        ("appearance skin tone", appearance.skin_tone.as_str()),
        ("appearance hair style", appearance.hair_style.as_str()),
        ("appearance hair color", appearance.hair_color.as_str()),
        ("appearance top style", appearance.top_style.as_str()),
        ("appearance top color", appearance.top_color.as_str()),
        ("appearance bottom style", appearance.bottom_style.as_str()),
        ("appearance bottom color", appearance.bottom_color.as_str()),
        ("appearance shoe color", appearance.shoe_color.as_str()),
        ("appearance body type", appearance.body_type.as_str()),
        ("appearance accessory", appearance.accessory.as_str()),
    ] {
        validate_text(value, field, 64)?;
    }
    if !matches!(appearance.body_type.as_str(), "slim" | "regular" | "broad") {
        return Err(anyhow!("model returned invalid appearance body type"));
    }
    Ok(())
}

/// Derive the hiring score only from job-relevant profile data. In particular,
/// age, gender and appearance are deliberately absent from this function.
fn profile_match_score(
    skills: &[SkillProfile],
    personality: &PersonalityProfile,
    work_style: &str,
    expertise: &[String],
) -> i64 {
    let skill_average = if skills.is_empty() {
        0.0
    } else {
        skills
            .iter()
            .map(|skill| f32::from(skill.level))
            .sum::<f32>()
            / skills.len() as f32
    };
    let personality_fit = (f32::from(personality.conscientiousness)
        + f32::from(personality.emotional_stability))
        / 2.0;
    let profile_completeness = if work_style.trim().is_empty() || expertise.is_empty() {
        0.0
    } else {
        100.0
    };
    (skill_average * 0.7 + personality_fit * 0.2 + profile_completeness * 0.1)
        .round()
        .clamp(0.0, 100.0) as i64
}

fn extract_json_object(value: &str) -> Result<&str> {
    let start = value
        .find('{')
        .ok_or_else(|| anyhow!("model response did not contain a JSON object"))?;
    let end = value
        .rfind('}')
        .ok_or_else(|| anyhow!("model response did not contain a complete JSON object"))?;
    if end < start {
        return Err(anyhow!("model response contained malformed JSON"));
    }
    Ok(&value[start..=end])
}

pub async fn hire_candidate(
    pool: &SqlitePool,
    company_id: &str,
    request_id: &str,
    candidate_id: &str,
    requested_workstation_id: Option<&str>,
) -> Result<HireDraft> {
    let mut tx = pool.begin().await?;
    let candidate = sqlx::query(
        "SELECT c.id, c.name, c.age, c.gender, c.title, c.personality, c.skills,
                c.work_style, c.appearance, c.hiring_request_id, r.floor_id
         FROM candidates c
         JOIN hiring_requests r ON r.id = c.hiring_request_id AND r.company_id = ?
         WHERE c.id = ? AND c.hiring_request_id = ? AND c.status = 'AVAILABLE'",
    )
    .bind(company_id)
    .bind(candidate_id)
    .bind(request_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| anyhow!("candidate is unavailable or does not belong to this request"))?;
    let floor_id: String = candidate.try_get("floor_id")?;
    let workstation = if let Some(workstation_id) = requested_workstation_id {
        sqlx::query(
            "SELECT id, location_id FROM workstations
             WHERE company_id = ? AND floor_id = ? AND id = ? AND status = 'AVAILABLE'
               AND assigned_employee_id IS NULL",
        )
        .bind(company_id)
        .bind(&floor_id)
        .bind(workstation_id)
        .fetch_optional(&mut *tx)
        .await?
    } else {
        sqlx::query(
            "SELECT id, location_id FROM workstations
             WHERE company_id = ? AND floor_id = ? AND status = 'AVAILABLE'
               AND assigned_employee_id IS NULL ORDER BY created_at, id LIMIT 1",
        )
        .bind(company_id)
        .bind(&floor_id)
        .fetch_optional(&mut *tx)
        .await?
    }
    .ok_or_else(|| anyhow!("NO_AVAILABLE_WORKSTATION: floor has no available workstation"))?;
    let workstation_id: String = workstation.try_get("id")?;
    let location_id: String = workstation.try_get("location_id")?;
    let entrance = sqlx::query(
        "SELECT id FROM office_locations
         WHERE company_id = ? AND floor_id = ? AND type = 'ENTRANCE' LIMIT 1",
    )
    .bind(company_id)
    .bind(&floor_id)
    .fetch_one(&mut *tx)
    .await?;
    let entrance_id: String = entrance.try_get("id")?;
    let entrance_position =
        sqlx::query("SELECT x, y, room_id FROM office_locations WHERE company_id = ? AND id = ?")
            .bind(company_id)
            .bind(&entrance_id)
            .fetch_one(&mut *tx)
            .await?;
    let now = Utc::now().to_rfc3339();
    let employee_id = format!("employee-{}", Uuid::new_v4());
    let personality: String = candidate.try_get("personality")?;
    let skills: String = candidate.try_get("skills")?;
    let work_style: String = candidate.try_get("work_style")?;
    let appearance: String = candidate.try_get("appearance")?;
    let name: String = candidate.try_get("name")?;
    let age: i64 = candidate.try_get("age")?;
    let gender: String = candidate.try_get("gender")?;
    let title: String = candidate.try_get("title")?;
    let model_config = serde_json::json!({
        "provider": "mock",
        "model": "mock-office-v1",
        "mode": "mock"
    })
    .to_string();
    sqlx::query(
        "INSERT INTO employees
         (id, company_id, name, age, gender, role, department, title, status,
          personality, skills, work_style, appearance, model_config, model_profile_id, desk_id,
          floor_id, hired_at, current_state, activity, movement, location_id,
          position_x, position_y, room_id, device, posture, presence)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'online', ?, ?, ?, ?, ?, 'mock-profile-01', ?, ?,
                 ?, 'IDLE', 'IDLE', 'STATIONARY', ?, ?, ?, ?, 'NONE', 'STANDING', 'BUSY')",
    )
    .bind(&employee_id)
    .bind(company_id)
    .bind(&name)
    .bind(age)
    .bind(&gender)
    .bind(&title)
    .bind(&floor_id)
    .bind(&title)
    .bind(&personality)
    .bind(&skills)
    .bind(&work_style)
    .bind(&appearance)
    .bind(model_config)
    .bind(&workstation_id)
    .bind(&floor_id)
    .bind(&now)
    .bind(&entrance_id)
    .bind(entrance_position.try_get::<f64, _>("x")?)
    .bind(entrance_position.try_get::<f64, _>("y")?)
    .bind(entrance_position.try_get::<Option<String>, _>("room_id")?)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE workstations SET status = 'OCCUPIED', assigned_employee_id = ?, updated_at = ?
         WHERE company_id = ? AND id = ?",
    )
    .bind(&employee_id)
    .bind(&now)
    .bind(company_id)
    .bind(&workstation_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE candidates SET status = 'HIRED', hired_employee_id = ?, updated_at = ?
         WHERE company_id = ? AND id = ?",
    )
    .bind(&employee_id)
    .bind(&now)
    .bind(company_id)
    .bind(candidate_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(HireDraft {
        employee_id,
        floor_id,
        workstation_id,
        location_id,
    })
}

pub async fn employee_for_hire(
    pool: &SqlitePool,
    company_id: &str,
    employee_id: &str,
) -> Result<Employee> {
    crate::infrastructure::db::employees(pool, company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| anyhow!("hired employee was not found: {employee_id}"))
}

pub fn hiring_intent(content: &str) -> bool {
    let normalized = content
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    normalized.contains("招聘")
        || (normalized.contains('招')
            || normalized.contains('找')
            || normalized.contains("需要")
            || normalized.contains("要"))
            && (normalized.contains('人')
                || normalized.contains("岗位")
                || normalized.contains("员工")
                || normalized.contains("职位"))
}

pub fn candidate_refresh_intent(content: &str) -> bool {
    [
        "刷新候选",
        "换一批",
        "重新推荐",
        "重新找",
        "再找三",
        "再给三",
    ]
    .iter()
    .any(|marker| content.contains(marker))
}

pub async fn refresh_latest_candidates(
    pool: &SqlitePool,
    company_id: &str,
    requester_id: &str,
) -> Result<HiringCreation> {
    let row = sqlx::query(
        "SELECT id, floor_id FROM hiring_requests
         WHERE company_id = ? AND requester_id = ?
           AND status NOT IN ('CLOSED', 'CANCELLED')
         ORDER BY updated_at DESC, created_at DESC LIMIT 1",
    )
    .bind(company_id)
    .bind(requester_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| anyhow!("还没有可刷新的招聘需求，请先说明要招聘的岗位"))?;
    let request_id: String = row.try_get("id")?;
    let floor_id: String = row.try_get("floor_id")?;
    let request = refresh_candidates(pool, company_id, &request_id).await?;
    Ok(HiringCreation { floor_id, request })
}

async fn resolve_floor(pool: &SqlitePool, company_id: &str, content: &str) -> Result<FloorSummary> {
    let floors = floors(pool, company_id).await?;
    let normalized = content
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    if let Some(floor) = floors.iter().find(|floor| {
        normalized.contains(&format!("F{}", floor.floor_number))
            || normalized.contains(&floor.name.to_uppercase())
    }) {
        return Ok(floor.clone());
    }
    floors
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("company has no floor; create a floor before hiring"))
}

#[derive(Clone, Debug)]
struct PositionSpec {
    title: String,
    headcount: u8,
    requirements: Vec<String>,
    preferences: Vec<String>,
}

fn parse_position_specs(content: &str) -> Vec<PositionSpec> {
    let mut specs = Vec::new();
    for raw in content.split(|character| "，。；,;\n".contains(character)) {
        let segment = raw.trim();
        if segment.is_empty()
            || (!segment.contains('招')
                && !segment.contains("需要")
                && !segment.contains('找')
                && !segment.contains('要'))
        {
            continue;
        }
        let headcount = extract_count(segment);
        let title = derive_title(segment);
        if title.is_empty() {
            continue;
        }
        let terms = extract_terms(segment);
        let preferences = terms
            .iter()
            .filter(|term| {
                segment.contains(&format!("希望{term}"))
                    || segment.contains(&format!("比较{term}"))
                    || segment.contains(&format!("{term}一点"))
                    || ["严谨", "自然", "好奇", "沟通", "内向", "外向"]
                        .iter()
                        .any(|value| term.contains(value))
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut requirements = terms
            .into_iter()
            .filter(|term| !preferences.contains(term))
            .collect::<Vec<_>>();
        if requirements.is_empty() {
            requirements.push(title.clone());
        }
        specs.push(PositionSpec {
            title,
            headcount,
            requirements,
            preferences,
        });
    }
    if specs.is_empty() && hiring_intent(content) {
        specs.push(PositionSpec {
            title: "根据对话定义的岗位".to_string(),
            headcount: extract_count(content),
            requirements: extract_terms(content),
            preferences: Vec::new(),
        });
    }
    specs
}

fn derive_title(segment: &str) -> String {
    let mut title = segment.trim().to_string();
    for marker in ["再要", "需要", "招聘", "招", "找", "要"] {
        if let Some(index) = title.find(marker) {
            title = title[index + marker.len()..].to_string();
            break;
        }
    }
    for prefix in [
        "我想",
        "我需要",
        "我要",
        "帮我",
        "请帮我",
        "再",
        "另外",
        "以及",
        "还要",
    ] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.to_string();
        }
    }
    for prefix in ["招", "招聘", "找", "需要", "要"] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.to_string();
        }
    }
    let mut chars = title.chars().collect::<Vec<_>>();
    while chars.first().is_some_and(|character| {
        matches!(
            character,
            '一' | '两' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
        ) || character.is_ascii_digit()
            || matches!(character, '个' | '名' | '位' | '人')
    }) {
        chars.remove(0);
    }
    title = chars.into_iter().collect();
    for suffix in ["的人", "员工", "岗位"] {
        if title.ends_with(suffix) && title.chars().count() > suffix.chars().count() + 2 {
            title = title.trim_end_matches(suffix).to_string();
        }
    }
    if let Some(index) = title.rfind('的') {
        let after = title[index + '的'.len_utf8()..].trim();
        if after.chars().count() >= 2 {
            title = after.to_string();
        }
    }
    title
        .trim_matches(|character: char| " ：:()（）".contains(character))
        .to_string()
}

fn extract_count(value: &str) -> u8 {
    if value.contains("十") {
        return 10;
    }
    for (digit, word) in [
        (9, '九'),
        (8, '八'),
        (7, '七'),
        (6, '六'),
        (5, '五'),
        (4, '四'),
        (3, '三'),
        (2, '两'),
        (2, '二'),
        (1, '一'),
    ] {
        if value.contains(word) {
            return digit;
        }
    }
    let digits = value
        .chars()
        .skip_while(|character| !character.is_ascii_digit())
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse::<u8>().unwrap_or(1).clamp(1, 20)
}

fn extract_terms(value: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&character) {
            current.push(character);
        } else if current.chars().count() >= 2 {
            terms.push(current.clone());
            current.clear();
        } else {
            current.clear();
        }
    }
    if current.chars().count() >= 2 {
        terms.push(current);
    }
    let stop = [
        "我要",
        "我想",
        "帮我",
        "请帮我",
        "需要",
        "招聘",
        "招募",
        "找人",
        "一个",
        "两个",
        "三个",
        "人员",
        "比较",
        "希望",
        "负责",
        "最好",
        "再要",
        "另外",
        "一个做",
    ];
    let mut seen = HashSet::new();
    terms
        .into_iter()
        .map(|term| {
            stop.iter()
                .fold(term, |value, item| value.replace(item, ""))
        })
        .filter(|term| term.chars().count() >= 2 && !stop.contains(&term.as_str()))
        .filter(|term| seen.insert(term.clone()))
        .take(12)
        .collect()
}

async fn find_or_create_role(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    company_id: &str,
    floor_id: &str,
    spec: &PositionSpec,
    now: &str,
) -> Result<String> {
    if let Some(id) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM role_definitions WHERE company_id = ? AND floor_id = ? AND name = ? LIMIT 1",
    )
    .bind(company_id)
    .bind(floor_id)
    .bind(&spec.title)
    .fetch_optional(&mut **tx)
    .await?
    {
        return Ok(id);
    }
    let id = format!("role-{}", Uuid::new_v4());
    sqlx::query(
        "INSERT INTO role_definitions
         (id, company_id, floor_id, name, responsibilities, skills, preferences, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(company_id)
    .bind(floor_id)
    .bind(&spec.title)
    .bind(serde_json::to_string(&vec![spec.title.clone()])?)
    .bind(serde_json::to_string(&spec.requirements)?)
    .bind(serde_json::to_string(&spec.preferences)?)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

async fn insert_floor_administrator(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    company_id: &str,
    floor_id: &str,
    floor_number: i64,
    floor_name: &str,
    now: &str,
) -> Result<String> {
    let employee_id = format!("employee-{}", Uuid::new_v4());
    let mut workstation_id = None;
    let mut location_id = office::floor_location_id(floor_id, "hr_desk_01");
    let room_id = office::floor_room_id(floor_id, "hr-office");
    if let Some(existing) = sqlx::query(
        "SELECT id, assigned_employee_id FROM workstations
         WHERE company_id = ? AND floor_id = ? AND location_id = ?",
    )
    .bind(company_id)
    .bind(floor_id)
    .bind(&location_id)
    .fetch_optional(&mut **tx)
    .await?
    {
        let assigned_employee_id: Option<String> = existing.try_get("assigned_employee_id")?;
        let occupied_by_active_employee = if let Some(assigned_employee_id) = assigned_employee_id {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM employees
                 WHERE company_id = ? AND id = ? AND employment_status = 'ACTIVE'",
            )
            .bind(company_id)
            .bind(assigned_employee_id)
            .fetch_one(&mut **tx)
            .await?
                > 0
        } else {
            false
        };
        if occupied_by_active_employee {
            // A malformed legacy database can have a regular active employee
            // at the old administrator location. Keep that assignment and
            // provision a distinct administrator location instead.
            location_id = format!("{location_id}__administrator");
        } else {
            workstation_id = Some(existing.try_get("id")?);
        }
    }
    sqlx::query(
        "INSERT OR IGNORE INTO office_locations
         (id, company_id, name, type, room_id, x, y, floor_id, created_at)
         VALUES (?, ?, '管理员工位', 'WORKSTATION', ?, 150, 318, ?, ?)",
    )
    .bind(&location_id)
    .bind(company_id)
    .bind(&room_id)
    .bind(floor_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    let location = sqlx::query(
        "SELECT x, y, room_id FROM office_locations
         WHERE company_id = ? AND floor_id = ? AND id = ?",
    )
    .bind(company_id)
    .bind(floor_id)
    .bind(&location_id)
    .fetch_one(&mut **tx)
    .await?;
    let x: f64 = location.try_get("x")?;
    let y: f64 = location.try_get("y")?;
    let room_id: Option<String> = location.try_get("room_id")?;
    let seed = Uuid::new_v4().as_u128() as u64;
    let names = [
        "安澜", "程知", "江宁", "陆遥", "许衡", "叶清", "周行", "顾安",
    ];
    let name = names[(seed as usize) % names.len()];
    let gender = if seed.is_multiple_of(2) {
        "female"
    } else {
        "male"
    };
    let age = 30 + (seed % 9) as u8;
    let personality = PersonalityProfile {
        openness: 72,
        conscientiousness: 92,
        extraversion: 66,
        agreeableness: 76,
        emotional_stability: 88,
        traits: vec!["有条理".into(), "善于协调".into(), "主动巡查".into()],
        communication_style: "先确认人员与预算边界，再给出候选和执行建议".into(),
        decision_style: "依据任务状态、证据和Token预算进行干预".into(),
        speech_habits: vec!["先复述关键需求".into(), "结尾给出明确的人员动作".into()],
        behavior_habits: vec![
            "定期巡查人员与任务状态".into(),
            "先确认约束再安排面试".into(),
        ],
    };
    let skills = vec![
        SkillProfile {
            name: "招聘与解雇".into(),
            level: 90,
        },
        SkillProfile {
            name: "楼层管理".into(),
            level: 92,
        },
        SkillProfile {
            name: "任务巡查".into(),
            level: 91,
        },
        SkillProfile {
            name: "Token预算".into(),
            level: 86,
        },
    ];
    let appearance = create_appearance(seed, gender, age);
    let workstation_id = if let Some(workstation_id) = workstation_id {
        sqlx::query(
            "UPDATE workstations
             SET room_id = ?, position_x = 70, position_y = 210, orientation = 'DOWN',
                 status = 'AVAILABLE', assigned_employee_id = NULL, updated_at = ?
             WHERE company_id = ? AND floor_id = ? AND id = ?",
        )
        .bind(&room_id)
        .bind(now)
        .bind(company_id)
        .bind(floor_id)
        .bind(&workstation_id)
        .execute(&mut **tx)
        .await?;
        workstation_id
    } else {
        let workstation_id = format!("workstation-{}", Uuid::new_v4());
        sqlx::query(
            "INSERT INTO workstations
             (id, company_id, floor_id, location_id, room_id, position_x, position_y,
              orientation, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 70, 210, 'DOWN', 'AVAILABLE', ?, ?)",
        )
        .bind(&workstation_id)
        .bind(company_id)
        .bind(floor_id)
        .bind(&location_id)
        .bind(&room_id)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
        workstation_id
    };
    sqlx::query(
        "INSERT INTO employees
         (id, company_id, name, age, gender, role, department, title, status,
          personality, skills, work_style, appearance, model_config, model_profile_id, desk_id,
          floor_id, hired_at, current_state, activity, movement, location_id,
          position_x, position_y, room_id, device, posture, presence, is_floor_administrator)
         VALUES (?, ?, ?, ?, ?, '楼层管理员', ?, '管理员', 'online', ?, ?,
                 '先核对人员与预算约束，再安排招聘、巡查和后续动作', ?,
                 '{\"provider\":\"mock\",\"model\":\"mock-office-v1\",\"mode\":\"mock\"}',
                 'mock-profile-01', ?, ?, ?, 'IDLE', 'IDLE', 'STATIONARY', ?, ?, ?, ?,
                 'COMPUTER', 'SITTING', 'AVAILABLE', 1)",
    )
    .bind(&employee_id)
    .bind(company_id)
    .bind(name)
    .bind(i64::from(age))
    .bind(gender)
    .bind(format!("管理员办公室 · F{floor_number} {floor_name}"))
    .bind(serde_json::to_string(&personality)?)
    .bind(serde_json::to_string(&skills)?)
    .bind(serde_json::to_string(&appearance)?)
    .bind(&workstation_id)
    .bind(floor_id)
    .bind(now)
    .bind(&location_id)
    .bind(x)
    .bind(y)
    .bind(&room_id)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE workstations SET status = 'OCCUPIED', assigned_employee_id = ?, updated_at = ?
         WHERE id = ? AND company_id = ?",
    )
    .bind(&employee_id)
    .bind(now)
    .bind(&workstation_id)
    .bind(company_id)
    .execute(&mut **tx)
    .await?;
    Ok(employee_id)
}

async fn insert_candidate_set(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    company_id: &str,
    request_id: &str,
    position_id: &str,
    spec: &PositionSpec,
    position_index: usize,
    now: &str,
) -> Result<()> {
    for candidate_index in 0..3 {
        let candidate_id = format!("candidate-{}", Uuid::new_v4());
        let seed = Uuid::new_v4().as_u128() as u64;
        let gender = if seed.is_multiple_of(2) {
            "female"
        } else {
            "male"
        };
        let age = 25 + (seed % 16) as u8;
        let name = candidate_name(seed, position_index, candidate_index);
        let personality = candidate_personality(seed, &spec.preferences);
        let skills = candidate_skills(&spec.requirements, seed);
        let appearance = create_appearance(seed, gender, age);
        let strengths = if spec.requirements.is_empty() {
            vec!["能够快速理解岗位目标".to_string()]
        } else {
            spec.requirements.iter().take(3).cloned().collect()
        };
        let weaknesses = vec!["需要通过实际任务继续验证长期稳定性".to_string()];
        let expertise = spec
            .requirements
            .iter()
            .chain(spec.preferences.iter())
            .take(6)
            .cloned()
            .collect::<Vec<_>>();
        let work_style = candidate_work_style(seed, &spec.title);
        let match_score = profile_match_score(&skills, &personality, &work_style, &expertise);
        sqlx::query(
            "INSERT INTO candidates
             (id, company_id, hiring_request_id, position_id, name, age, gender, title,
              personality, skills, work_style, strengths, weaknesses, expertise,
              expected_model, agent_runtime, appearance, match_score,
              interview_evaluation, status, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '由录用时模型配置决定',
                     'Company Agent Runtime', ?, ?, ?, 'AVAILABLE', ?, ?)",
        )
        .bind(&candidate_id)
        .bind(company_id)
        .bind(request_id)
        .bind(position_id)
        .bind(&name)
        .bind(i64::from(age))
        .bind(gender)
        .bind(&spec.title)
        .bind(serde_json::to_string(&personality)?)
        .bind(serde_json::to_string(&skills)?)
        .bind(work_style)
        .bind(serde_json::to_string(&strengths)?)
        .bind(serde_json::to_string(&weaknesses)?)
        .bind(serde_json::to_string(&expertise)?)
        .bind(serde_json::to_string(&appearance)?)
        .bind(match_score)
        .bind(format!(
            "管理员初筛匹配 {match_score} 分；年龄、性别和外观未参与评分。"
        ))
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn candidate_name(seed: u64, position_index: usize, candidate_index: usize) -> String {
    let surnames = ["林", "周", "沈", "许", "顾", "唐", "叶", "苏"];
    let given = ["远", "知行", "言川", "清禾", "予安", "宁", "闻", "嘉"];
    format!(
        "{}{}",
        surnames[(seed as usize + position_index * 3 + candidate_index) % surnames.len()],
        given[((seed >> 8) as usize + position_index + candidate_index * 2) % given.len()]
    )
}

fn candidate_personality(seed: u64, preferences: &[String]) -> PersonalityProfile {
    let communication_styles = [
        "先确认目标和边界，再给出可执行方案",
        "用事实和例子说明判断，最后明确下一步",
        "先列出关键取舍，再说明我建议的路径",
        "表达简洁，遇到风险会先说影响范围",
    ];
    let decision_styles = [
        "结合事实、经验与结果反馈推进",
        "优先验证最关键的假设，再逐步扩大范围",
        "在交付速度与长期维护之间保持平衡",
        "先按风险排序，再选择可恢复的方案",
    ];
    let speech_habit_sets = [
        vec!["表达保持简洁".to_string(), "结尾明确下一步".to_string()],
        vec!["常用具体例子".to_string(), "会标注当前假设".to_string()],
        vec!["先复述问题".to_string(), "习惯列出取舍".to_string()],
        vec![
            "少寒暄，直接进入重点".to_string(),
            "会说明验证方式".to_string(),
        ],
    ];
    let behavior_habit_sets = [
        vec![
            "先做小范围验证再扩展".to_string(),
            "会保留可回滚路径".to_string(),
        ],
        vec![
            "习惯把任务拆成可交付步骤".to_string(),
            "定期同步风险".to_string(),
        ],
        vec![
            "专注时减少无关打断".to_string(),
            "完成后主动整理记录".to_string(),
        ],
        vec![
            "遇到阻塞会尽早求助".to_string(),
            "会复盘并更新工作清单".to_string(),
        ],
    ];
    let style_index = (seed as usize) % communication_styles.len();
    PersonalityProfile {
        openness: 60 + (seed % 30) as u8,
        conscientiousness: 68 + (seed % 25) as u8,
        extraversion: 45 + (seed % 40) as u8,
        agreeableness: 58 + (seed % 30) as u8,
        emotional_stability: 65 + (seed % 30) as u8,
        traits: {
            let mut traits = if preferences.is_empty() {
                vec!["目标感清晰".to_string(), "愿意协作".to_string()]
            } else {
                preferences.iter().take(2).cloned().collect()
            };
            let trait_pool = ["重视验证", "善于拆解", "保持专注", "主动同步"];
            traits.push(trait_pool[(seed as usize / 7) % trait_pool.len()].to_string());
            traits
        },
        communication_style: communication_styles[style_index].to_string(),
        decision_style: decision_styles[(seed as usize / 3) % decision_styles.len()].to_string(),
        speech_habits: speech_habit_sets[style_index].clone(),
        behavior_habits: behavior_habit_sets[(seed as usize / 5) % behavior_habit_sets.len()]
            .clone(),
    }
}

fn candidate_work_style(seed: u64, title: &str) -> String {
    let styles = [
        "先拆成可验证的小步，完成一项就同步结果",
        "偏好异步记录，集中时间处理高复杂度问题",
        "先和协作者对齐边界，再快速交付可运行版本",
        "先做风险排序，保留回滚方案后再扩大范围",
    ];
    format!(
        "管理员针对“{title}”拟定：{}。",
        styles[(seed as usize / 11) % styles.len()]
    )
}

fn candidate_skills(requirements: &[String], seed: u64) -> Vec<SkillProfile> {
    let mut values = requirements
        .iter()
        .take(5)
        .enumerate()
        .map(|(index, name)| SkillProfile {
            name: name.clone(),
            level: 72 + ((seed as usize + index * 9) % 24) as u8,
        })
        .collect::<Vec<_>>();
    if values.is_empty() {
        values.push(SkillProfile {
            name: "岗位适应".to_string(),
            level: 78,
        });
    }
    values
}

fn clean_required(value: &str, field: &str) -> Result<String> {
    let cleaned = value.trim();
    if cleaned.is_empty() {
        return Err(anyhow!("{field} cannot be empty"));
    }
    if cleaned.chars().count() > 80 {
        return Err(anyhow!("{field} is too long"));
    }
    Ok(cleaned.to_string())
}

fn floor_from_row(
    row: sqlx::sqlite::SqliteRow,
    roles: Vec<FloorRoleSummary>,
) -> Result<FloorSummary, sqlx::Error> {
    Ok(FloorSummary {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        floor_number: row.try_get("floor_number")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        mission: row.try_get("mission")?,
        responsibilities: json_vec(row.try_get("responsibilities")?),
        roles,
        status: row.try_get("status")?,
        employee_count: row.try_get("employee_count")?,
        workstation_count: row.try_get("workstation_count")?,
        available_workstations: row.try_get("available_workstations")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn workstation_from_row(row: sqlx::sqlite::SqliteRow) -> Result<WorkstationSummary, sqlx::Error> {
    Ok(WorkstationSummary {
        id: row.try_get("id")?,
        company_id: row.try_get("company_id")?,
        floor_id: row.try_get("floor_id")?,
        location_id: row.try_get("location_id")?,
        room_id: row.try_get("room_id")?,
        x: row.try_get::<f64, _>("position_x")? as f32,
        y: row.try_get::<f64, _>("position_y")? as f32,
        orientation: row.try_get("orientation")?,
        status: row.try_get("status")?,
        assigned_employee_id: row.try_get("assigned_employee_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn position_from_row(row: sqlx::sqlite::SqliteRow) -> Result<HiringPositionSummary, sqlx::Error> {
    Ok(HiringPositionSummary {
        id: row.try_get("id")?,
        role_definition_id: row.try_get("role_definition_id")?,
        title: row.try_get("title")?,
        headcount: row.try_get("headcount")?,
        requirements: json_vec(row.try_get("requirements")?),
        preferences: json_vec(row.try_get("preferences")?),
        status: row.try_get("status")?,
    })
}

fn candidate_from_row(row: sqlx::sqlite::SqliteRow) -> Result<CandidateSummary, sqlx::Error> {
    Ok(CandidateSummary {
        id: row.try_get("id")?,
        hiring_request_id: row.try_get("hiring_request_id")?,
        position_id: row.try_get("position_id")?,
        name: row.try_get("name")?,
        age: row.try_get::<i64, _>("age")?.clamp(0, 255) as u8,
        gender: row.try_get("gender")?,
        title: row.try_get("title")?,
        personality: serde_json::from_str(&row.try_get::<String, _>("personality")?)
            .unwrap_or_default(),
        skills: serde_json::from_str(&row.try_get::<String, _>("skills")?).unwrap_or_default(),
        work_style: row.try_get("work_style")?,
        strengths: json_vec(row.try_get("strengths")?),
        weaknesses: json_vec(row.try_get("weaknesses")?),
        expertise: json_vec(row.try_get("expertise")?),
        expected_model: row.try_get("expected_model")?,
        agent_runtime: row.try_get("agent_runtime")?,
        appearance: serde_json::from_str(&row.try_get::<String, _>("appearance")?)
            .unwrap_or_default(),
        match_score: row.try_get("match_score")?,
        interview_evaluation: row.try_get("interview_evaluation")?,
        status: row.try_get("status")?,
        hired_employee_id: row.try_get("hired_employee_id")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn json_vec(value: String) -> Vec<String> {
    serde_json::from_str(&value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        administrator_intervention_intent, administrator_patrol_intent, candidate_personality,
        candidate_refresh_intent, candidate_work_style, derive_title, extract_count, extract_terms,
        hiring_intent, parse_position_specs, validate_ai_candidate_profile, AiCandidateProfile,
        HiringPersonalizationResult, HiringRequestSummary,
    };

    #[test]
    fn parses_dynamic_positions_without_role_enum() {
        let specs = parse_position_specs("我要两个懂 AI 和科技的文章作者，再要一个做文章审核的人");
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].headcount, 2);
        assert!(specs[0].title.contains("文章作者"));
        assert_eq!(specs[1].headcount, 1);
    }

    #[test]
    fn detects_hiring_intent_and_counts_chinese_numbers() {
        assert!(hiring_intent("帮我找三个懂研究的人"));
        assert!(hiring_intent("需要两个懂研究的人"));
        assert!(hiring_intent("招聘一个UI/UX设计"));
        assert_eq!(derive_title("招聘一个UI/UX设计"), "UI/UX设计");
        assert_eq!(extract_count("两个岗位"), 2);
        assert_eq!(derive_title("再要一个做文章审核的人"), "做文章审核");
        assert!(!extract_terms("AI研究").is_empty());
    }

    #[test]
    fn detects_administrator_refresh_and_patrol_commands() {
        assert!(candidate_refresh_intent("这批不合适，换一批候选人"));
        assert!(administrator_patrol_intent("巡查一下谁在消耗 token"));
        assert!(administrator_intervention_intent(
            "解除死循环并停止卡住任务"
        ));
        assert!(!administrator_intervention_intent("只查看，不要停止任务"));
    }

    #[test]
    fn mock_candidate_personality_is_seed_stable_and_varied() {
        let first = candidate_personality(42, &[]);
        let second = candidate_personality(42, &[]);
        let different = candidate_personality(47, &[]);
        assert_eq!(first.communication_style, second.communication_style);
        assert_eq!(first.speech_habits, second.speech_habits);
        assert_eq!(first.behavior_habits, second.behavior_habits);
        assert_ne!(first.communication_style, different.communication_style);
        assert_ne!(first.behavior_habits, different.behavior_habits);
        assert_ne!(
            candidate_work_style(42, "工程师"),
            candidate_work_style(99, "工程师")
        );
        assert!(!first.speech_habits.is_empty());
        assert!(!first.behavior_habits.is_empty());
    }

    #[test]
    fn hiring_fallback_is_explicit_and_uses_safe_reason() {
        let request: HiringRequestSummary = serde_json::from_value(json!({
            "id": "request-1",
            "companyId": "company-01",
            "floorId": "floor-01",
            "requesterId": "hr-01",
            "summary": "招聘工程师",
            "status": "CANDIDATES_READY",
            "sourceMessageId": null,
            "positions": [],
            "candidates": [],
            "createdAt": "",
            "updatedAt": ""
        }))
        .expect("test request should deserialize");
        let fallback =
            HiringPersonalizationResult::fallback(request, "administrator_model_output_invalid");
        assert_eq!(fallback.generation_mode, "mock_fallback");
        assert_eq!(
            fallback.fallback_reason.as_deref(),
            Some("administrator_model_output_invalid")
        );
    }

    #[test]
    fn rejects_malformed_ai_candidate_profile_before_persistence() {
        let malformed: AiCandidateProfile = serde_json::from_value(json!({
            "id": "candidate-1",
            "name": "候选人",
            "age": 30,
            "gender": "female",
            "title": "工程师",
            "personality": {
                "openness": 50,
                "conscientiousness": 50,
                "extraversion": 50,
                "agreeableness": 50,
                "emotionalStability": 50,
                "traits": ["可靠"],
                "communicationStyle": "先确认目标",
                "decisionStyle": "依据证据",
                "speechHabits": ["先复述"],
                "behaviorHabits": ["先验证"]
            },
            "skills": [{"name": "Rust", "level": 80}],
            "workStyle": "先拆解再交付",
            "strengths": [],
            "weaknesses": ["需要更多领域经验"],
            "expertise": ["Rust"],
            "appearance": {
                "seed": 1,
                "skinTone": "#F3C7A8",
                "hairStyle": "short",
                "hairColor": "#241A17",
                "topStyle": "shirt",
                "topColor": "#3E5C76",
                "bottomStyle": "trousers",
                "bottomColor": "#26364A",
                "shoeColor": "#1D2026",
                "bodyType": "regular",
                "heightScale": 1.0,
                "glasses": false,
                "accessory": "none"
            },
            "matchScore": 80,
            "interviewEvaluation": "回答具体"
        }))
        .expect("test profile should deserialize");
        assert!(validate_ai_candidate_profile(&malformed).is_err());
    }
}
