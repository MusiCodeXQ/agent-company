use std::{
    env,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use tokio::{io::AsyncWriteExt, process::Command};

use crate::models::ContainerRuntimeStatus;

#[derive(Clone, Debug)]
pub struct ContainerEngineConfig {
    pub enabled: bool,
    pub binary: String,
    pub default_image: String,
    pub employee_root: PathBuf,
    pub cpu_limit: String,
    pub memory_limit: String,
    pub pids_limit: String,
}

impl ContainerEngineConfig {
    pub fn from_env() -> Self {
        Self {
            enabled: env_flag("AI_COMPANY_DOCKER_ENABLED"),
            binary: env::var("AI_COMPANY_DOCKER_BIN").unwrap_or_else(|_| "docker".to_string()),
            default_image: env::var("AI_COMPANY_EMPLOYEE_IMAGE")
                .unwrap_or_else(|_| "ai-company/employee-base:ubuntu24".to_string()),
            employee_root: env::var("AI_COMPANY_EMPLOYEE_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("data/employee-runtimes")),
            cpu_limit: env::var("AI_COMPANY_EMPLOYEE_CPUS").unwrap_or_else(|_| "2".to_string()),
            memory_limit: env::var("AI_COMPANY_EMPLOYEE_MEMORY")
                .unwrap_or_else(|_| "2g".to_string()),
            pids_limit: env::var("AI_COMPANY_EMPLOYEE_PIDS").unwrap_or_else(|_| "512".to_string()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RunContainerRequest {
    pub company_id: String,
    pub employee_id: String,
    pub run_id: String,
    pub workspace_path: PathBuf,
    pub state_path: PathBuf,
    pub artifacts_path: PathBuf,
    pub environment_path: PathBuf,
}

#[async_trait]
pub trait ContainerEngine: Send + Sync {
    fn config(&self) -> &ContainerEngineConfig;
    async fn status(&self) -> ContainerRuntimeStatus;
    async fn cleanup_stale_containers(&self) -> Result<u64>;
    async fn exec_employee_runtime(
        &self,
        request: &RunContainerRequest,
        input: &[u8],
    ) -> Result<Vec<u8>>;
}

#[derive(Clone, Debug)]
pub struct DockerCliEngine {
    config: ContainerEngineConfig,
}

impl DockerCliEngine {
    pub fn from_env() -> Self {
        Self {
            config: ContainerEngineConfig::from_env(),
        }
    }

    async fn output(&self, args: &[String]) -> Result<std::process::Output> {
        Command::new(&self.config.binary)
            .args(args)
            .output()
            .await
            .with_context(|| format!("run {} CLI", self.config.binary))
    }

    fn require_enabled(&self) -> Result<()> {
        if self.config.enabled {
            Ok(())
        } else {
            Err(anyhow!(
                "Docker employee runtime is disabled; set AI_COMPANY_DOCKER_ENABLED=true"
            ))
        }
    }

    async fn force_remove(&self, container_name: &str) {
        let _ = Command::new(&self.config.binary)
            .args(["rm", "--force", container_name])
            .output()
            .await;
    }
}

#[async_trait]
impl ContainerEngine for DockerCliEngine {
    fn config(&self) -> &ContainerEngineConfig {
        &self.config
    }

    async fn status(&self) -> ContainerRuntimeStatus {
        if !self.config.enabled {
            return ContainerRuntimeStatus {
                enabled: false,
                available: false,
                engine_version: None,
                default_image: self.config.default_image.clone(),
                last_error: None,
            };
        }
        let args = vec![
            "version".to_string(),
            "--format".to_string(),
            "{{.Server.Version}}".to_string(),
        ];
        match tokio::time::timeout(Duration::from_secs(5), self.output(&args)).await {
            Ok(Ok(output)) if output.status.success() => ContainerRuntimeStatus {
                enabled: true,
                available: true,
                engine_version: Some(String::from_utf8_lossy(&output.stdout).trim().to_string()),
                default_image: self.config.default_image.clone(),
                last_error: None,
            },
            Ok(Ok(output)) => unavailable_status(&self.config, compact_stderr(&output.stderr)),
            Ok(Err(error)) => unavailable_status(&self.config, error.to_string()),
            Err(_) => unavailable_status(&self.config, "Docker health check timed out".to_string()),
        }
    }

    async fn cleanup_stale_containers(&self) -> Result<u64> {
        self.require_enabled()?;
        let output = self
            .output(&[
                "ps".to_string(),
                "--all".to_string(),
                "--quiet".to_string(),
                "--filter".to_string(),
                "label=ai-company.managed=true".to_string(),
            ])
            .await?;
        if !output.status.success() {
            return Err(anyhow!(
                "list stale AI Company containers: {}",
                compact_stderr(&output.stderr)
            ));
        }
        let ids = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        for id in &ids {
            self.force_remove(id).await;
        }
        Ok(ids.len() as u64)
    }

    async fn exec_employee_runtime(
        &self,
        request: &RunContainerRequest,
        input: &[u8],
    ) -> Result<Vec<u8>> {
        self.require_enabled()?;
        validate_image_ref(&self.config.default_image)?;
        let user = host_user(&request.workspace_path)?;
        let container_name = format!(
            "ai-company-run-{}-{}",
            safe_resource_name(&request.employee_id),
            safe_resource_name(&request.run_id)
        );
        let path = "/employee-env/python-venv/bin:/employee-env/npm/bin:/employee-env/cargo/bin:/opt/python-venv/bin:/opt/cargo/bin:/opt/java/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
        let mut command = Command::new(&self.config.binary);
        command.kill_on_drop(true);
        let mut child = command
            .args([
                "run",
                "--rm",
                "--interactive",
                "--name",
                &container_name,
                "--label",
                "ai-company.managed=true",
                "--label",
                &format!("ai-company.company-id={}", request.company_id),
                "--label",
                &format!("ai-company.employee-id={}", request.employee_id),
                "--security-opt",
                "no-new-privileges",
                "--cap-drop",
                "ALL",
                "--read-only",
                "--user",
                &user,
                "--workdir",
                "/workspace",
                "--volume",
                &bind_mount(&request.workspace_path, "/workspace")?,
                "--volume",
                &bind_mount(&request.state_path, "/agent-state")?,
                "--volume",
                &bind_mount(&request.artifacts_path, "/artifacts")?,
                "--volume",
                &bind_mount(&request.environment_path, "/employee-env")?,
                "--tmpfs",
                "/tmp:rw,nosuid,nodev,size=268435456",
                "--cpus",
                &self.config.cpu_limit,
                "--memory",
                &self.config.memory_limit,
                "--pids-limit",
                &self.config.pids_limit,
                "--add-host",
                "host.docker.internal:host-gateway",
                "--env",
                "HOME=/agent-state/home",
                "--env",
                "VIRTUAL_ENV=/employee-env/python-venv",
                "--env",
                "NPM_CONFIG_PREFIX=/employee-env/npm",
                "--env",
                "CARGO_INSTALL_ROOT=/employee-env/cargo",
                "--env",
                &format!("PATH={path}"),
                &self.config.default_image,
                "/bin/bash",
                "-c",
                "mkdir -p /agent-state/home /employee-env/npm /employee-env/cargo; if [ ! -x /employee-env/python-venv/bin/python ]; then python3 -m venv /employee-env/python-venv; fi; exec /usr/local/bin/employee-runtime run",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("start temporary employee Docker container")?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("employee runtime stdin was not available"))?;
        stdin
            .write_all(input)
            .await
            .context("send run request to employee runtime")?;
        drop(stdin);
        let output =
            match tokio::time::timeout(Duration::from_secs(900), child.wait_with_output()).await {
                Ok(output) => output?,
                Err(_) => {
                    self.force_remove(&container_name).await;
                    return Err(anyhow!("employee runtime exceeded 15 minute hard timeout"));
                }
            };
        if !output.status.success() {
            return Err(anyhow!(
                "employee runtime failed: {}",
                compact_stderr(&output.stderr)
            ));
        }
        Ok(output.stdout)
    }
}

pub fn safe_resource_name(value: &str) -> String {
    let mut value = value.chars().fold(String::new(), |mut result, character| {
        if character.is_ascii_alphanumeric() {
            result.push(character.to_ascii_lowercase());
        } else if !result.ends_with('-') {
            result.push('-');
        }
        result
    });
    value.truncate(value.floor_char_boundary(48));
    let value = value.trim_matches('-');
    if value.is_empty() {
        "employee".to_string()
    } else {
        value.to_string()
    }
}

fn bind_mount(source: &Path, target: &str) -> Result<String> {
    if !source.is_absolute() || !source.exists() {
        return Err(anyhow!(
            "employee runtime path is not an existing absolute path"
        ));
    }
    let source = source
        .to_str()
        .ok_or_else(|| anyhow!("employee runtime path is not valid UTF-8"))?;
    if source.contains(':') || source.contains('\n') || source.contains('\r') {
        return Err(anyhow!("employee runtime path cannot be mounted safely"));
    }
    Ok(format!("{source}:{target}:rw"))
}

#[cfg(unix)]
fn host_user(path: &PathBuf) -> Result<String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path)?;
    Ok(format!("{}:{}", metadata.uid(), metadata.gid()))
}

#[cfg(not(unix))]
fn host_user(_path: &PathBuf) -> Result<String> {
    Ok("10001:10001".to_string())
}

fn validate_image_ref(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 255
        || value.chars().any(char::is_whitespace)
        || value.starts_with('-')
    {
        return Err(anyhow!("invalid Docker image reference"));
    }
    Ok(())
}

fn unavailable_status(config: &ContainerEngineConfig, error: String) -> ContainerRuntimeStatus {
    ContainerRuntimeStatus {
        enabled: true,
        available: false,
        engine_version: None,
        default_image: config.default_image.clone(),
        last_error: Some(error),
    }
}

fn compact_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let text = text.trim();
    if text.is_empty() {
        "unknown Docker error".to_string()
    } else {
        text.chars().take(800).collect()
    }
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::safe_resource_name;

    #[test]
    fn resource_names_are_stable_and_docker_safe() {
        assert_eq!(safe_resource_name("Backend_员工-01"), "backend-01");
        assert_eq!(safe_resource_name("---"), "employee");
    }
}
