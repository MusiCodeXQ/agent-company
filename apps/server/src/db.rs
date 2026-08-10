use std::str::FromStr;

use chrono::Utc;
use sqlx::sqlite::{SqliteJournalMode, SqlitePoolOptions};
use sqlx::{sqlite::SqliteConnectOptions, FromRow, Row, SqlitePool};

use crate::appearance::create_appearance;
use crate::models::{
    AgentModelConfig, Company, Employee, MeetingSummary, PersonalityProfile, SkillProfile,
    TaskSummary,
};

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
    desk_id: Option<String>,
    hired_at: String,
    current_state: String,
}

#[derive(Debug, FromRow)]
struct CompanyRow {
    id: String,
    name: String,
    status: String,
    created_at: String,
}

pub async fn initialize(
    database_url: &str,
) -> Result<SqlitePool, Box<dyn std::error::Error + Send + Sync>> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
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
    sqlx::query(
        "INSERT OR IGNORE INTO companies (id, name, status, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind("company-01")
    .bind("星河软件公司 · Demo Office")
    .bind("active")
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
              personality, skills, appearance, model_config, desk_id, hired_at, current_state)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
        .bind(&employee.desk_id)
        .bind(&employee.hired_at)
        .bind(&employee.current_state)
        .execute(pool)
        .await?;
    }

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

pub async fn employees(pool: &SqlitePool, company_id: &str) -> Result<Vec<Employee>, sqlx::Error> {
    let rows = sqlx::query_as::<_, EmployeeRow>(
        "SELECT id, company_id, name, age, gender, role, department, title, status,
                personality, skills, appearance, model_config, desk_id, hired_at, current_state
         FROM employees WHERE company_id = ? ORDER BY hired_at, id",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(employee_from_row).collect())
}

pub async fn tasks(pool: &SqlitePool, company_id: &str) -> Result<Vec<TaskSummary>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id, title, status, assignee_id FROM tasks WHERE company_id = ? ORDER BY created_at",
    )
    .bind(company_id)
    .fetch_all(pool)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(TaskSummary {
                id: row.try_get("id")?,
                title: row.try_get("title")?,
                status: row.try_get("status")?,
                assignee_id: row.try_get("assignee_id")?,
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
    rows.into_iter()
        .map(|row| {
            let participant_ids =
                serde_json::from_str::<Vec<String>>(&row.try_get::<String, _>("participant_ids")?)
                    .unwrap_or_default();
            Ok(MeetingSummary {
                id: row.try_get("id")?,
                title: row.try_get("title")?,
                meeting_type: row.try_get("type")?,
                room_id: row.try_get("room_id")?,
                participant_ids,
                status: row.try_get("status")?,
            })
        })
        .collect()
}

pub async fn insert_demo_meeting(
    pool: &SqlitePool,
    company_id: &str,
    meeting_id: &str,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR REPLACE INTO meetings
         (id, company_id, title, type, room_id, participant_ids, status, agenda,
          transcript, decisions, action_items, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(meeting_id)
    .bind(company_id)
    .bind("实时通信边界同步")
    .bind("technical")
    .bind("meeting-a")
    .bind(r#"["backend-01","supervisor-01","product-01"]"#)
    .bind("scheduled")
    .bind(r#"["确认 Rust/WebRTC 服务边界","记录下一步任务"]"#)
    .bind("[]")
    .bind("[]")
    .bind("[]")
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(())
}

fn employee_from_row(row: EmployeeRow) -> Employee {
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
        desk_id: row.desk_id,
        hired_at: row.hired_at,
        current_state: row.current_state,
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
        current_state: "idle".to_string(),
    }
}
