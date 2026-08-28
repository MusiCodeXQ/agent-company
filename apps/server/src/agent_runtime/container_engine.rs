use std::{
    env,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::Command,
    sync::mpsc,
};

use crate::domain::models::ContainerRuntimeStatus;

const DOCKER_CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
const DOCKER_STOP_TIMEOUT_SECONDS: &str = "5";

#[derive(Clone, Debug)]
pub struct ContainerEngineConfig {
    pub enabled: bool,
    pub keep_containers: bool,
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
            keep_containers: env_flag("AI_COMPANY_DOCKER_KEEP_CONTAINERS"),
            binary: env::var("AI_COMPANY_DOCKER_BIN").unwrap_or_else(|_| "docker".to_string()),
            default_image: env::var("AI_COMPANY_EMPLOYEE_IMAGE")
                .unwrap_or_else(|_| "ai-company/employee-base:debian-stable".to_string()),
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
        output_tx: mpsc::Sender<Vec<u8>>,
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
        let mut command = Command::new(&self.config.binary);
        command.kill_on_drop(true);
        tokio::time::timeout(DOCKER_CONTROL_TIMEOUT, command.args(args).output())
            .await
            .map_err(|_| {
                anyhow!(
                    "Docker CLI command timed out after {} seconds",
                    DOCKER_CONTROL_TIMEOUT.as_secs()
                )
            })?
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

    async fn force_remove(&self, container_name: &str) -> Result<()> {
        let args = vec![
            "rm".to_string(),
            "--force".to_string(),
            container_name.to_string(),
        ];
        let output = self.output(&args).await?;
        if !output.status.success() {
            return Err(anyhow!(
                "docker rm --force failed for employee container (exit code {:?})",
                output.status.code()
            ));
        }
        Ok(())
    }

    async fn stop_and_retain(&self, container_name: &str) -> Result<()> {
        let args = vec![
            "stop".to_string(),
            "--time".to_string(),
            DOCKER_STOP_TIMEOUT_SECONDS.to_string(),
            container_name.to_string(),
        ];
        match self.output(&args).await {
            Ok(output) if output.status.success() => {
                tracing::info!(
                    container_name,
                    stop_timeout_seconds = DOCKER_STOP_TIMEOUT_SECONDS,
                    "stopped and retained Docker employee container"
                );
                Ok(())
            }
            Ok(output) => {
                tracing::warn!(
                    container_name,
                    exit_code = ?output.status.code(),
                    "docker stop failed; trying docker kill while retaining the container"
                );
                self.kill_and_retain(container_name).await.map_err(|error| {
                    anyhow!("docker stop and docker kill failed for employee container: {error}")
                })
            }
            Err(error) => {
                tracing::warn!(
                    container_name,
                    %error,
                    "could not run docker stop; trying docker kill while retaining the container"
                );
                self.kill_and_retain(container_name).await.map_err(|error| {
                    anyhow!("docker stop and docker kill failed for employee container: {error}")
                })
            }
        }
    }

    async fn kill_and_retain(&self, container_name: &str) -> Result<()> {
        let args = vec!["kill".to_string(), container_name.to_string()];
        match self.output(&args).await {
            Ok(output) if output.status.success() => {
                tracing::info!(
                    container_name,
                    "killed and retained Docker employee container"
                );
                Ok(())
            }
            Ok(output) => {
                tracing::warn!(
                    container_name,
                    exit_code = ?output.status.code(),
                    "docker kill failed; Docker employee container may still be running"
                );
                Err(anyhow!(
                    "docker kill failed for employee container (exit code {:?})",
                    output.status.code()
                ))
            }
            Err(error) => {
                tracing::warn!(
                    container_name,
                    %error,
                    "could not run docker kill; Docker employee container may still be running"
                );
                Err(error).context("docker kill failed for employee container")
            }
        }
    }

    async fn kill_and_reap_cli(&self, child: &mut tokio::process::Child, container_name: &str) {
        if let Err(error) = child.start_kill() {
            tracing::debug!(container_name, %error, "Docker CLI was already stopped or could not be killed");
        }
        match tokio::time::timeout(DOCKER_CONTROL_TIMEOUT, child.wait()).await {
            Ok(Ok(status)) => {
                tracing::debug!(
                    container_name,
                    exit_code = ?status.code(),
                    "reaped Docker CLI process"
                );
            }
            Ok(Err(error)) => {
                tracing::warn!(container_name, %error, "could not reap Docker CLI process");
            }
            Err(_) => {
                tracing::warn!(container_name, "timed out while reaping Docker CLI process");
            }
        }
    }

    async fn cleanup_execution_container(&self, container_name: &str) -> Result<bool> {
        if should_force_remove_container(self.config.keep_containers) {
            self.force_remove(container_name).await?;
            Ok(false)
        } else {
            self.stop_and_retain(container_name).await?;
            Ok(true)
        }
    }

    async fn cleanup_after_exec_error(
        &self,
        child: &mut tokio::process::Child,
        container_name: &str,
        error: anyhow::Error,
        reason: &'static str,
    ) -> anyhow::Error {
        let cleanup_result = self
            .cleanup_execution_container(container_name)
            .await
            .map(|_| ());
        if let Err(cleanup_error) = &cleanup_result {
            tracing::warn!(
                container_name,
                %cleanup_error,
                reason,
                "could not clean employee Docker container after runtime error"
            );
        }
        self.kill_and_reap_cli(child, container_name).await;
        match cleanup_result {
            Ok(()) => error,
            Err(cleanup_error) => error.context(format!(
                "container cleanup failed after {reason}: {cleanup_error}"
            )),
        }
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
            .output(&stale_container_list_args(self.config.keep_containers))
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
        if self.config.keep_containers {
            let mut stopped = 0;
            for id in &ids {
                if let Err(error) = self.stop_and_retain(id).await {
                    return Err(anyhow!(
                        "stop running AI Company execution container: {error}"
                    ));
                }
                stopped += 1;
            }
            tracing::info!(
                found = ids.len(),
                stopped_and_retained = stopped,
                "completed startup handling for running AI Company execution containers"
            );
            return Ok(stopped);
        }
        let mut removal_failed = false;
        for id in &ids {
            if let Err(error) = self.force_remove(id).await {
                removal_failed = true;
                tracing::warn!(
                    container_name = %id,
                    %error,
                    "could not remove stale AI Company execution container"
                );
            }
        }
        if removal_failed {
            return Err(anyhow!(
                "one or more stale AI Company execution containers could not be removed"
            ));
        }
        Ok(ids.len() as u64)
    }

    async fn exec_employee_runtime(
        &self,
        request: &RunContainerRequest,
        input: &[u8],
        output_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<Vec<u8>> {
        self.require_enabled()?;
        validate_image_ref(&self.config.default_image)?;
        let user = host_user(&request.workspace_path)?;
        let container_name = format!(
            "ai-company-run-{}-{}",
            safe_resource_name(&request.employee_id),
            safe_resource_name(&request.run_id)
        );
        tracing::info!(
            employee_id = %request.employee_id,
            run_id = %request.run_id,
            container_name = %container_name,
            image = %self.config.default_image,
            keep_containers = self.config.keep_containers,
            "starting employee Docker runtime container"
        );
        let employee_home = "/home/employee";
        let path = "/home/employee/.local/bin:/home/employee/bin:/home/employee/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
        let mut command = Command::new(&self.config.binary);
        command.kill_on_drop(true);
        let mut child = command
            .args(docker_run_lifecycle_args(self.config.keep_containers))
            .args([
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
                &bind_mount(&request.environment_path, employee_home)?,
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
                &format!("HOME={employee_home}"),
                "--env",
                &format!("XDG_CONFIG_HOME={employee_home}/.config"),
                "--env",
                &format!("XDG_CACHE_HOME={employee_home}/.cache"),
                "--env",
                &format!("XDG_DATA_HOME={employee_home}/.local/share"),
                "--env",
                &format!("NPM_CONFIG_PREFIX={employee_home}/.local"),
                "--env",
                &format!("CARGO_HOME={employee_home}/.cargo"),
                "--env",
                &format!("RUSTUP_HOME={employee_home}/.rustup"),
                "--env",
                &format!("GRADLE_USER_HOME={employee_home}/.gradle"),
                "--env",
                &format!("PATH={path}"),
                &self.config.default_image,
                "/bin/bash",
                "-c",
                "mkdir -p /agent-state /home/employee/.local/bin /home/employee/.local/opt /home/employee/.local/share /home/employee/.cache /home/employee/.config /home/employee/.cargo /home/employee/.rustup /home/employee/.gradle /home/employee/.m2 /home/employee/.ai-company; exec /usr/local/bin/employee-runtime run",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("start employee Docker container")?;
        let mut stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                return Err(self
                    .cleanup_after_exec_error(
                        &mut child,
                        &container_name,
                        anyhow!("employee runtime stdin was not available"),
                        "employee runtime stdin was not available",
                    )
                    .await);
            }
        };
        if let Err(error) = stdin.write_all(input).await {
            drop(stdin);
            return Err(self
                .cleanup_after_exec_error(
                    &mut child,
                    &container_name,
                    anyhow!("send run request to employee runtime: {error}"),
                    "send run request to employee runtime",
                )
                .await);
        }
        drop(stdin);
        let mut stdout = match child.stdout.take() {
            Some(stdout) => BufReader::new(stdout),
            None => {
                return Err(self
                    .cleanup_after_exec_error(
                        &mut child,
                        &container_name,
                        anyhow!("employee runtime stdout was not available"),
                        "employee runtime stdout was not available",
                    )
                    .await);
            }
        };
        let mut stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                return Err(self
                    .cleanup_after_exec_error(
                        &mut child,
                        &container_name,
                        anyhow!("employee runtime stderr was not available"),
                        "employee runtime stderr was not available",
                    )
                    .await);
            }
        };
        let stderr_task = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).await.map(|_| bytes)
        });
        let mut collected_stdout = Vec::new();
        let mut line = String::new();
        let result = tokio::time::timeout(Duration::from_secs(900), async {
            loop {
                line.clear();
                let read = stdout.read_line(&mut line).await?;
                if read == 0 {
                    break;
                }
                collected_stdout.extend_from_slice(line.as_bytes());
                output_tx
                    .send(line.as_bytes().to_vec())
                    .await
                    .map_err(|_| anyhow!("employee runtime event stream receiver closed"))?;
            }
            let status = child.wait().await?;
            let stderr = stderr_task
                .await
                .map_err(|error| anyhow!("join employee runtime stderr reader: {error}"))?
                .map_err(|error| anyhow!("read employee runtime stderr: {error}"))?;
            Ok::<_, anyhow::Error>((status, stderr))
        })
        .await;
        let (status, stderr) = match result {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => {
                return Err(self
                    .cleanup_after_exec_error(
                        &mut child,
                        &container_name,
                        error,
                        "employee runtime stream processing failed",
                    )
                    .await);
            }
            Err(_) => {
                let (container_retained, cleanup_error) =
                    match self.cleanup_execution_container(&container_name).await {
                        Ok(retained) => (retained, None),
                        Err(error) => {
                            tracing::warn!(
                                employee_id = %request.employee_id,
                                run_id = %request.run_id,
                                container_name = %container_name,
                                %error,
                                "could not clean employee Docker container after hard timeout"
                            );
                            (false, Some(error))
                        }
                    };
                self.kill_and_reap_cli(&mut child, &container_name).await;
                tracing::warn!(
                    employee_id = %request.employee_id,
                    run_id = %request.run_id,
                    container_name = %container_name,
                    container_retained,
                    "employee runtime exceeded 15 minute hard timeout"
                );
                let timeout_error = anyhow!("employee runtime exceeded 15 minute hard timeout");
                return Err(match cleanup_error {
                    Some(error) => {
                        timeout_error.context(format!("container cleanup failed: {error}"))
                    }
                    None => timeout_error,
                });
            }
        };
        if !status.success() {
            tracing::warn!(
                employee_id = %request.employee_id,
                run_id = %request.run_id,
                container_name = %container_name,
                exit_code = ?status.code(),
                container_retained = self.config.keep_containers,
                "employee Docker runtime container exited with failure"
            );
            return Err(anyhow!(
                "employee runtime failed: {}",
                compact_stderr(&stderr)
            ));
        }
        tracing::info!(
            employee_id = %request.employee_id,
            run_id = %request.run_id,
            container_name = %container_name,
            container_retained = self.config.keep_containers,
            "employee Docker runtime container exited successfully"
        );
        Ok(collected_stdout)
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
        .is_some_and(|value| env_flag_value(&value))
}

fn env_flag_value(value: &str) -> bool {
    matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes")
}

fn docker_run_lifecycle_args(keep_containers: bool) -> Vec<&'static str> {
    let mut args = vec!["run"];
    if !keep_containers {
        args.push("--rm");
    }
    args
}

fn stale_container_list_args(keep_containers: bool) -> Vec<String> {
    let mut args = vec!["ps".to_string()];
    if !keep_containers {
        args.push("--all".to_string());
    }
    args.extend([
        "--quiet".to_string(),
        "--filter".to_string(),
        "label=ai-company.managed=true".to_string(),
    ]);
    args
}

pub(crate) fn should_cleanup_stale_containers(enabled: bool, keep_containers: bool) -> bool {
    enabled && !keep_containers
}

fn should_force_remove_container(keep_containers: bool) -> bool {
    !keep_containers
}

#[cfg(test)]
mod tests {
    use super::{
        docker_run_lifecycle_args, env_flag_value, safe_resource_name,
        should_cleanup_stale_containers, should_force_remove_container, stale_container_list_args,
    };

    #[test]
    fn resource_names_are_stable_and_docker_safe() {
        assert_eq!(safe_resource_name("Backend_员工-01"), "backend-01");
        assert_eq!(safe_resource_name("---"), "employee");
    }

    #[test]
    fn docker_run_uses_rm_by_default_and_omits_it_when_containers_are_kept() {
        assert_eq!(docker_run_lifecycle_args(false), vec!["run", "--rm"]);
        assert_eq!(docker_run_lifecycle_args(true), vec!["run"]);
    }

    #[test]
    fn keep_switch_preserves_or_removes_containers_on_cleanup() {
        assert!(should_force_remove_container(false));
        assert!(!should_force_remove_container(true));
    }

    #[test]
    fn destructive_stale_cleanup_runs_only_when_containers_are_not_kept() {
        assert!(should_cleanup_stale_containers(true, false));
        assert!(!should_cleanup_stale_containers(true, true));
        assert!(!should_cleanup_stale_containers(false, false));
    }

    #[test]
    fn keep_switch_lists_running_containers_only() {
        assert_eq!(
            stale_container_list_args(false),
            vec![
                "ps",
                "--all",
                "--quiet",
                "--filter",
                "label=ai-company.managed=true"
            ]
        );
        assert_eq!(
            stale_container_list_args(true),
            vec!["ps", "--quiet", "--filter", "label=ai-company.managed=true"]
        );
    }

    #[test]
    fn keep_switch_accepts_true_and_defaults_to_false_for_other_values() {
        assert!(env_flag_value("true"));
        assert!(env_flag_value("TRUE"));
        assert!(!env_flag_value("false"));
        assert!(!env_flag_value(""));
    }
}
