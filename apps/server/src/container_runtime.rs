use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

use crate::{
    app_state::AppState,
    container_engine::safe_resource_name,
    db,
    models::RuntimeWorkspaceSummary,
    runtime_store::{self, RuntimeWorkspaceUpsert},
};

const EMPLOYEE_RUNTIME_POLICY: &str = r#"# 员工个人运行环境规则

你的临时容器只提供基础文件、网络和诊断命令，不预装 Python、Node.js、Rust、JDK、Gradle 或 Maven。

- `$HOME` 是 `/home/employee`，它绑定到宿主机上你的专属员工目录，不属于临时容器 writable layer。
- 个人可执行文件统一放到 `$HOME/.local/bin`，下载或解压的工具链放到 `$HOME/.local/opt` 或 `$HOME/.local/share`。
- Rust 使用 `$HOME/.cargo` 和 `$HOME/.rustup`；npm 全局 prefix 是 `$HOME/.local`。
- Gradle 缓存使用 `$HOME/.gradle`，Maven 本地仓库使用 `$HOME/.m2`。
- Python、Node.js、Rust、JDK、Gradle、Maven 及未来工具链都由你在个人目录安装、升级和清理，不得写入容器根文件系统。
- 安装前核对来源和校验和；不要使用 sudo、apt 或其他 root 安装方式。缺少系统库时提交基础镜像软件建议。
- 安装或升级完成后调用 `company_record_personal_toolchain`，登记名称、版本和个人目录内的安装路径。
- 已登记工具见 `$HOME/.ai-company/toolchains.json`。每次工作前优先复用已有版本，避免重复下载。
"#;

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
        .join(employee_directory_name(employee_id)?);
    let workspace_path = employee_root.join("workspace");
    let state_path = employee_root.join("state");
    let artifacts_path = employee_root.join("artifacts");
    let environment_path = employee_root.join("environment");
    for path in [
        &workspace_path,
        &state_path,
        &artifacts_path,
        &environment_path,
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
    prepare_employee_home(&environment_path).await?;

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
            prepare_employee_home(Path::new(&workspace.environment_path)).await?;
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

fn employee_directory_name(employee_id: &str) -> Result<&str> {
    if employee_id.is_empty()
        || !employee_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(anyhow!(
            "employee id is not safe for an exact directory name: {employee_id}"
        ));
    }
    Ok(employee_id)
}

async fn prepare_employee_home(home: &Path) -> Result<()> {
    for relative in [
        ".local/bin",
        ".local/opt",
        ".local/share",
        ".cache",
        ".config",
        ".cargo",
        ".rustup",
        ".gradle",
        ".m2",
        ".ai-company",
    ] {
        tokio::fs::create_dir_all(home.join(relative))
            .await
            .with_context(|| format!("create employee home directory {relative}"))?;
    }
    tokio::fs::write(
        home.join(".ai-company/RUNTIME_POLICY.md"),
        EMPLOYEE_RUNTIME_POLICY,
    )
    .await
    .context("write employee runtime policy")?;
    let manifest = home.join(".ai-company/toolchains.json");
    if !tokio::fs::try_exists(&manifest).await? {
        tokio::fs::write(manifest, b"[]\n")
            .await
            .context("initialize employee toolchain manifest")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{employee_directory_name, prepare_employee_home};

    #[test]
    fn employee_directory_uses_the_exact_valid_employee_id() {
        assert_eq!(
            employee_directory_name("employee-123_A").expect("employee id should be valid"),
            "employee-123_A"
        );
        assert!(employee_directory_name("../employee").is_err());
        assert!(employee_directory_name("employee/child").is_err());
        assert!(employee_directory_name("").is_err());
    }

    #[tokio::test]
    async fn initializes_employee_home_policy_without_replacing_manifest() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let home = std::env::temp_dir().join(format!(
            "ai-company-home-test-{}-{nonce}",
            std::process::id()
        ));
        tokio::fs::create_dir_all(&home)
            .await
            .expect("employee home should be created");
        prepare_employee_home(&home)
            .await
            .expect("employee home should be initialized");
        let policy = tokio::fs::read_to_string(home.join(".ai-company/RUNTIME_POLICY.md"))
            .await
            .expect("runtime policy should exist");
        assert!(policy.contains("company_record_personal_toolchain"));
        let manifest = home.join(".ai-company/toolchains.json");
        tokio::fs::write(&manifest, b"[{\"name\":\"custom\"}]\n")
            .await
            .expect("custom manifest should be written");
        prepare_employee_home(&home)
            .await
            .expect("existing employee home should be refreshed");
        assert_eq!(
            tokio::fs::read_to_string(&manifest)
                .await
                .expect("manifest should remain readable"),
            "[{\"name\":\"custom\"}]\n"
        );
        tokio::fs::remove_dir_all(home)
            .await
            .expect("test directory should be removable");
    }
}
