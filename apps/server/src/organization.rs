use std::collections::HashSet;

use anyhow::{anyhow, Result};
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::{
    appearance::create_appearance,
    models::{
        CandidateSummary, Employee, FloorRoleSummary, FloorSummary, HiringPositionSummary,
        HiringRequestSummary, PersonalityProfile, SkillProfile, WorkstationSummary,
    },
    office,
};

#[derive(Clone, Debug)]
pub struct HiringCreation {
    pub request: HiringRequestSummary,
    pub floor_id: String,
}

#[derive(Clone, Debug)]
pub struct HireDraft {
    pub employee_id: String,
    pub floor_id: String,
    pub workstation_id: String,
    pub location_id: String,
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
    insert_default_floor_roles(&mut tx, company_id, &floor_id, &now).await?;
    tx.commit().await?;
    floor(pool, company_id, &floor_id).await
}

pub async fn delete_floor(pool: &SqlitePool, company_id: &str, floor_id: &str) -> Result<()> {
    let current = floor(pool, company_id, floor_id).await?;
    if floor_id == "floor-01" {
        return Err(anyhow!("基础办公层不能删除"));
    }
    if current.employee_count > 0 {
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

        for candidate_index in 0..3 {
            let candidate_id = format!("candidate-{}", Uuid::new_v4());
            let seed = 20_000_u64 + (position_index as u64) * 100 + candidate_index as u64;
            let gender = if (seed + candidate_index as u64).is_multiple_of(2) {
                "female"
            } else {
                "male"
            };
            let age = 25 + ((seed + candidate_index as u64 * 3) % 12) as u8;
            let name = candidate_name(position_index, candidate_index);
            let personality = candidate_personality(seed, &spec.preferences);
            let skills = candidate_skills(&spec.requirements, seed);
            let appearance = create_appearance(seed, gender, age);
            let strengths = if spec.requirements.is_empty() {
                vec!["能够快速理解岗位目标".to_string()]
            } else {
                spec.requirements.iter().take(3).cloned().collect()
            };
            let weaknesses = vec!["需要在入职后通过实际任务继续校准".to_string()];
            let expertise = spec
                .requirements
                .iter()
                .chain(spec.preferences.iter())
                .take(6)
                .cloned()
                .collect::<Vec<_>>();
            let evaluation = format!(
                "根据岗位“{}”的要求生成的初筛候选人；匹配 {} 分，建议用户结合实际对话和面试继续判断。",
                spec.title,
                78 + (candidate_index as i64 * 5)
            );
            sqlx::query(
                "INSERT INTO candidates
                 (id, company_id, hiring_request_id, position_id, name, age, gender, title,
                  personality, skills, work_style, strengths, weaknesses, expertise,
                  expected_model, agent_runtime, appearance, match_score,
                  interview_evaluation, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'mock-office-v1',
                         'Native Runtime', ?, ?, ?, 'AVAILABLE', ?, ?)",
            )
            .bind(&candidate_id)
            .bind(company_id)
            .bind(&request_id)
            .bind(&position_id)
            .bind(&name)
            .bind(i64::from(age))
            .bind(gender)
            .bind(&spec.title)
            .bind(serde_json::to_string(&personality)?)
            .bind(serde_json::to_string(&skills)?)
            .bind(format!(
                "先理解岗位目标，再按自己的节奏推进{}。",
                spec.title
            ))
            .bind(serde_json::to_string(&strengths)?)
            .bind(serde_json::to_string(&weaknesses)?)
            .bind(serde_json::to_string(&expertise)?)
            .bind(serde_json::to_string(&appearance)?)
            .bind(78 + candidate_index as i64 * 5)
            .bind(evaluation)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
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
                c.appearance, c.hiring_request_id, r.floor_id
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
          personality, skills, appearance, model_config, model_profile_id, desk_id,
          floor_id, hired_at, current_state, activity, movement, location_id,
          position_x, position_y, room_id, device, posture, presence)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'online', ?, ?, ?, ?, 'mock-profile-01', ?, ?,
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
    crate::db::employees(pool, company_id)
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
    (normalized.contains('招')
        || normalized.contains("招聘")
        || normalized.contains('找')
        || normalized.contains("需要")
        || normalized.contains("要"))
        && (normalized.contains('人')
            || normalized.contains("岗位")
            || normalized.contains("员工")
            || normalized.contains("职位"))
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

async fn insert_default_floor_roles(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    company_id: &str,
    floor_id: &str,
    now: &str,
) -> Result<()> {
    let defaults = [
        (
            "HR",
            vec!["理解招聘需求", "维护候选人流程", "协助员工入职"],
            vec!["沟通", "人才评估", "组织协调"],
        ),
        (
            "管理员",
            vec!["维护楼层配置", "管理工位与空间资源", "协助公司运行"],
            vec!["组织管理", "资源协调", "流程执行"],
        ),
    ];
    for (name, responsibilities, skills) in defaults {
        sqlx::query(
            "INSERT INTO role_definitions
             (id, company_id, floor_id, name, responsibilities, skills, preferences,
              created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, '[]', ?, ?)",
        )
        .bind(format!("role-{}", Uuid::new_v4()))
        .bind(company_id)
        .bind(floor_id)
        .bind(name)
        .bind(serde_json::to_string(&responsibilities)?)
        .bind(serde_json::to_string(&skills)?)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

fn candidate_name(position_index: usize, candidate_index: usize) -> String {
    let surnames = ["林", "周", "沈", "许", "顾", "唐", "叶", "苏"];
    let given = ["远", "知行", "言川", "清禾", "予安", "宁", "闻", "嘉"];
    format!(
        "{}{}",
        surnames[(position_index * 3 + candidate_index) % surnames.len()],
        given[(position_index + candidate_index * 2) % given.len()]
    )
}

fn candidate_personality(seed: u64, preferences: &[String]) -> PersonalityProfile {
    PersonalityProfile {
        openness: 60 + (seed % 30) as u8,
        conscientiousness: 68 + (seed % 25) as u8,
        extraversion: 45 + (seed % 40) as u8,
        agreeableness: 58 + (seed % 30) as u8,
        emotional_stability: 65 + (seed % 30) as u8,
        traits: if preferences.is_empty() {
            vec!["目标感清晰".to_string(), "愿意协作".to_string()]
        } else {
            preferences.iter().take(3).cloned().collect()
        },
        communication_style: "先确认目标和边界，再给出可执行方案".to_string(),
        decision_style: "结合事实、经验与结果反馈推进".to_string(),
    }
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
    use super::{derive_title, extract_count, extract_terms, hiring_intent, parse_position_specs};

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
        assert_eq!(extract_count("两个岗位"), 2);
        assert_eq!(derive_title("再要一个做文章审核的人"), "做文章审核");
        assert!(!extract_terms("AI研究").is_empty());
    }
}
