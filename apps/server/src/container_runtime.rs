use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::{
    app_state::AppState,
    container_engine::safe_resource_name,
    db,
    models::RuntimeWorkspaceSummary,
    runtime_store::{self, RuntimeWorkspaceUpsert},
};

pub async fn provision_employee(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
) -> Result<RuntimeWorkspaceSummary> {
    if !state.container_engine.config().enabled {
        return Err(anyhow!(
            "Docker employee runtime is disabled; set AI_COMPANY_DOCKER_ENABLED=true"
        ));
    }
    let _guard = state.container_lock.lock().await;
    let employee = db::employees(&state.pool, company_id)
        .await?
        .into_iter()
        .find(|employee| employee.id == employee_id)
        .ok_or_else(|| anyhow!("employee {employee_id} was not found"))?;
    if employee.employment_status != "ACTIVE" {
        return Err(anyhow!("only active employees can own a runtime workspace"));
    }

    let configured_root = &state.container_engine.config().employee_root;
    let configured_root = if configured_root.is_absolute() {
        configured_root.clone()
    } else {
        std::env::current_dir()?.join(configured_root)
    };
    let employee_root = configured_root
        .join(safe_resource_name(company_id))
        .join(safe_resource_name(employee_id));
    let workspace_path = employee_root.join("workspace");
    let state_path = employee_root.join("state");
    let artifacts_path = employee_root.join("artifacts");
    let environment_path = employee_root.join("environment");
    for path in [
        &workspace_path,
        &state_path,
        &artifacts_path,
        &environment_path,
        &state_path.join("home"),
    ] {
        tokio::fs::create_dir_all(path)
            .await
            .with_context(|| format!("create employee runtime directory {}", path.display()))?;
    }
    let employee_root = canonical(&employee_root)?;
    let workspace_path = canonical(&workspace_path)?;
    let state_path = canonical(&state_path)?;
    let artifacts_path = canonical(&artifacts_path)?;
    let environment_path = canonical(&environment_path)?;
    ensure_under(&employee_root, &workspace_path)?;
    ensure_under(&employee_root, &state_path)?;
    ensure_under(&employee_root, &artifacts_path)?;
    ensure_under(&employee_root, &environment_path)?;

    let record = runtime_store::upsert_workspace(
        &state.pool,
        RuntimeWorkspaceUpsert {
            company_id,
            employee_id,
            host_root_path: path_text(&employee_root)?,
            workspace_path: path_text(&workspace_path)?,
            state_path: path_text(&state_path)?,
            artifacts_path: path_text(&artifacts_path)?,
            environment_path: path_text(&environment_path)?,
        },
    )
    .await?;
    db::set_adk_runtime_binding(
        &state.pool,
        db::AdkRuntimeBindingUpdate {
            company_id,
            employee_id,
            runtime_agent_id: &format!("adk_{}", safe_resource_name(employee_id)),
            workspace_path: &record.workspace_path,
            state_path: &record.state_path,
            environment_path: &record.environment_path,
            artifacts_path: &record.artifacts_path,
        },
    )
    .await?;
    Ok(record)
}

pub async fn ensure_workspace(
    state: &AppState,
    company_id: &str,
    employee_id: &str,
) -> Result<RuntimeWorkspaceSummary> {
    if let Some(workspace) =
        runtime_store::workspace_for_employee(&state.pool, company_id, employee_id).await?
    {
        if workspace.status == "READY" && workspace_paths_exist(&workspace) {
            return Ok(workspace);
        }
    }
    provision_employee(state, company_id, employee_id).await
}

pub async fn archive_employee(state: &AppState, company_id: &str, employee_id: &str) -> Result<()> {
    runtime_store::archive_workspace(&state.pool, company_id, employee_id).await?;
    db::update_runtime_binding_status(
        &state.pool,
        company_id,
        employee_id,
        Some("DISABLED"),
        Some("ARCHIVED"),
        None,
    )
    .await?;
    Ok(())
}

fn workspace_paths_exist(workspace: &RuntimeWorkspaceSummary) -> bool {
    [
        &workspace.host_root_path,
        &workspace.workspace_path,
        &workspace.state_path,
        &workspace.artifacts_path,
        &workspace.environment_path,
    ]
    .iter()
    .all(|path| Path::new(path).is_dir())
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("canonicalize employee runtime path {}", path.display()))
}

fn ensure_under(root: &Path, child: &Path) -> Result<()> {
    if child.starts_with(root) {
        Ok(())
    } else {
        Err(anyhow!(
            "employee runtime directory escaped configured root"
        ))
    }
}

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| anyhow!("employee runtime path is not valid UTF-8"))
}
