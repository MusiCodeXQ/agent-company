use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::process::Command;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

use crate::models::{Employee, OpenClawStatus};

const DEFAULT_GATEWAY_URL: &str = "ws://127.0.0.1:18789";
const DEFAULT_PROTOCOL_VERSION: i64 = 4;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RUN_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Clone, Debug)]
pub struct OpenClawConfig {
    pub enabled: bool,
    pub gateway_url: String,
    pub gateway_token: Option<String>,
    pub protocol_version: i64,
    pub workspace_root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct OpenClawRunResult {
    pub content: String,
    pub run_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProvisionResult {
    pub runtime_agent_id: String,
    pub workspace_path: String,
    pub agent_dir: Option<String>,
}

#[derive(Clone, Debug)]
struct GatewayHello {
    protocol: i64,
    version: Option<String>,
}

pub fn config() -> OpenClawConfig {
    let enabled = env_flag("OPENCLAW_ENABLED");
    let gateway_url = env::var("OPENCLAW_GATEWAY_URL")
        .unwrap_or_else(|_| DEFAULT_GATEWAY_URL.to_string())
        .trim_end_matches('/')
        .to_string();
    let gateway_token = env::var("OPENCLAW_GATEWAY_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let protocol_version = env::var("OPENCLAW_PROTOCOL_VERSION")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(DEFAULT_PROTOCOL_VERSION);
    let workspace_root = env::var("OPENCLAW_WORKSPACE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("data/openclaw/workspaces"));
    OpenClawConfig {
        enabled,
        gateway_url,
        gateway_token,
        protocol_version,
        workspace_root,
    }
}

pub fn config_status() -> OpenClawStatus {
    let config = config();
    OpenClawStatus {
        enabled: config.enabled,
        configured: config.enabled && config.gateway_token.is_some(),
        gateway_url: config.gateway_url,
        reachable: false,
        protocol: Some(config.protocol_version),
        version: None,
        last_error: if !config.enabled {
            None
        } else if config.gateway_token.is_none() {
            Some(
                "缺少 OPENCLAW_GATEWAY_TOKEN；请从 OpenClaw Gateway 配置中提供共享 Token。"
                    .to_string(),
            )
        } else {
            None
        },
        checked_at: None,
    }
}

pub async fn health() -> OpenClawStatus {
    let config = config();
    let checked_at = Some(Utc::now().to_rfc3339());
    let mut status = config_status();
    status.checked_at = checked_at;
    if !status.configured {
        return status;
    }
    match connect(&config).await {
        Ok((mut socket, hello)) => {
            let _ = socket.close(None).await;
            status.reachable = true;
            status.protocol = Some(hello.protocol);
            status.version = hello.version;
            status.last_error = None;
        }
        Err(error) => status.last_error = Some(public_error(&error)),
    }
    status
}

pub async fn run_turn(
    runtime_agent_id: &str,
    session_key: &str,
    message: &str,
) -> Result<OpenClawRunResult> {
    let config = config();
    if !config.enabled {
        return Err(anyhow!("OpenClaw integration is disabled"));
    }
    if config.gateway_token.is_none() {
        return Err(anyhow!(
            "OPENCLAW_GATEWAY_TOKEN is required when OpenClaw is enabled"
        ));
    }
    if runtime_agent_id.trim().is_empty() || session_key.trim().is_empty() {
        return Err(anyhow!(
            "OpenClaw runtime agent and session key are required"
        ));
    }

    let (mut socket, _) = connect(&config).await?;
    let request_id = format!("ai-company-{}", Uuid::new_v4());
    let idempotency_key = format!("ai-company-run-{}", Uuid::new_v4());
    socket
        .send(text_message(json!({
            "type": "req",
            "id": request_id,
            "method": "sessions.send",
            "params": {
                "key": session_key,
                "agentId": runtime_agent_id,
                "message": message,
                "idempotencyKey": idempotency_key,
            }
        }))?)
        .await
        .context("send OpenClaw sessions.send request")?;

    let deadline = tokio::time::Instant::now() + RUN_TIMEOUT;
    let mut run_id = None;
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow!("timed out waiting for OpenClaw run"));
        }
        let frame = tokio::time::timeout(remaining, socket.next())
            .await
            .context("waiting for OpenClaw response")?
            .ok_or_else(|| anyhow!("OpenClaw Gateway closed the connection"))??;
        let Some(value) = frame_value(frame) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("res")
            && value.get("id").and_then(Value::as_str) == Some(request_id.as_str())
        {
            if value.get("ok").and_then(Value::as_bool) != Some(true) {
                return Err(gateway_error(&value));
            }
            let payload = value.get("payload").cloned().unwrap_or(Value::Null);
            run_id = payload
                .get("runId")
                .or_else(|| payload.get("runID"))
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(content) = assistant_text(&payload) {
                return Ok(OpenClawRunResult { content, run_id });
            }
            continue;
        }
        if value.get("type").and_then(Value::as_str) != Some("event") {
            continue;
        }
        let event_name = value
            .get("event")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !matches!(event_name, "chat" | "session.message" | "session.operation") {
            continue;
        }
        let payload = value.get("payload").cloned().unwrap_or(Value::Null);
        if let Some(candidate_run_id) = payload
            .get("runId")
            .or_else(|| payload.get("runID"))
            .and_then(Value::as_str)
        {
            run_id = Some(candidate_run_id.to_string());
        }
        let state = payload.get("state").and_then(Value::as_str);
        let is_terminal = matches!(
            state,
            Some("final" | "done" | "completed" | "error" | "aborted")
        );
        if let Some(content) = assistant_text(&payload) {
            if is_terminal || event_name == "session.message" {
                return Ok(OpenClawRunResult { content, run_id });
            }
        }
        if matches!(state, Some("error" | "aborted")) {
            return Err(gateway_error(&payload));
        }
    }
}

pub async fn archive_agent(runtime_agent_id: &str, session_key: &str) -> Result<()> {
    let config = config();
    if !config.enabled || config.gateway_token.is_none() {
        return Ok(());
    }
    let (mut socket, _) = connect(&config).await?;
    let abort_params = json!({
        "key": session_key,
        "agentId": runtime_agent_id,
        "clearQueued": true,
    });
    let _ = rpc(&mut socket, "sessions.abort", abort_params).await;
    rpc(
        &mut socket,
        "sessions.patch",
        json!({
            "key": session_key,
            "agentId": runtime_agent_id,
            "archived": true,
        }),
    )
    .await
    .map(|_| ())
}

pub async fn provision_employee(employee: &Employee) -> Result<ProvisionResult> {
    let config = config();
    if !config.enabled {
        return Err(anyhow!("OpenClaw integration is disabled"));
    }
    let runtime_agent_id = format!("company_{}", safe_id(&employee.id));
    let workspace = config.workspace_root.join(&runtime_agent_id);
    write_workspace_files(&workspace, employee).await?;

    let add = Command::new("openclaw")
        .args([
            "agents",
            "add",
            runtime_agent_id.as_str(),
            "--workspace",
            workspace.to_string_lossy().as_ref(),
            "--non-interactive",
            "--json",
        ])
        .output()
        .await
        .context("run openclaw agents add; install OpenClaw before provisioning")?;
    if !add.status.success() {
        let listed = Command::new("openclaw")
            .args(["agents", "list", "--json"])
            .output()
            .await
            .context("check existing OpenClaw agents")?;
        if !listed.status.success() || !stdout_has_agent(&listed.stdout, &runtime_agent_id) {
            return Err(command_error("openclaw agents add", &add.stderr));
        }
    }

    let identity = Command::new("openclaw")
        .args([
            "agents",
            "set-identity",
            "--agent",
            runtime_agent_id.as_str(),
            "--name",
            employee.name.as_str(),
            "--json",
        ])
        .output()
        .await
        .context("set OpenClaw agent identity")?;
    if !identity.status.success() {
        return Err(command_error(
            "openclaw agents set-identity",
            &identity.stderr,
        ));
    }

    Ok(ProvisionResult {
        runtime_agent_id,
        workspace_path: workspace.to_string_lossy().to_string(),
        agent_dir: None,
    })
}

async fn connect(
    config: &OpenClawConfig,
) -> Result<(
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    GatewayHello,
)> {
    let (mut socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(&config.gateway_url))
        .await
        .context("OpenClaw Gateway connection timed out")??;
    let _challenge = match tokio::time::timeout(CONNECT_TIMEOUT, socket.next()).await {
        Ok(Some(Ok(frame))) => frame_value(frame).filter(|value| {
            value.get("event").and_then(Value::as_str) == Some("connect.challenge")
        }),
        Ok(Some(Err(error))) => return Err(error.into()),
        Ok(None) => return Err(anyhow!("OpenClaw Gateway closed before handshake")),
        Err(_) => None,
    };
    let connect_id = format!("connect-{}", Uuid::new_v4());
    let params = json!({
        "minProtocol": config.protocol_version,
        "maxProtocol": config.protocol_version,
        "client": {
            "id": "gateway-client",
            "version": env!("CARGO_PKG_VERSION"),
            "platform": "linux",
            "mode": "backend"
        },
        "role": "operator",
        "scopes": ["operator.read", "operator.write"],
        "caps": ["tool-events"],
        "commands": [],
        "permissions": {},
        "auth": {"token": config.gateway_token.clone().unwrap_or_default()},
        "locale": "zh-CN",
        "userAgent": "ai-company-server"
    });
    socket
        .send(text_message(json!({
            "type": "req",
            "id": connect_id,
            "method": "connect",
            "params": params,
        }))?)
        .await
        .context("send OpenClaw connect handshake")?;

    loop {
        let frame = tokio::time::timeout(CONNECT_TIMEOUT, socket.next())
            .await
            .context("OpenClaw handshake timed out")?
            .ok_or_else(|| anyhow!("OpenClaw Gateway closed during handshake"))??;
        let Some(value) = frame_value(frame) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("res")
            || value.get("id").and_then(Value::as_str) != Some(connect_id.as_str())
        {
            continue;
        }
        if value.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(gateway_error(&value));
        }
        let payload = value.get("payload").cloned().unwrap_or(Value::Null);
        let protocol = payload
            .get("protocol")
            .and_then(Value::as_i64)
            .unwrap_or(config.protocol_version);
        let version = payload
            .get("server")
            .and_then(|server| server.get("version"))
            .and_then(Value::as_str)
            .map(str::to_string);
        return Ok((socket, GatewayHello { protocol, version }));
    }
}

async fn rpc(
    socket: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    method: &str,
    params: Value,
) -> Result<Value> {
    let request_id = format!("rpc-{}", Uuid::new_v4());
    socket
        .send(text_message(json!({
            "type": "req",
            "id": request_id,
            "method": method,
            "params": params,
        }))?)
        .await?;
    loop {
        let frame = tokio::time::timeout(CONNECT_TIMEOUT, socket.next())
            .await
            .context("OpenClaw RPC timed out")?
            .ok_or_else(|| anyhow!("OpenClaw Gateway closed during RPC"))??;
        let Some(value) = frame_value(frame) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) == Some("res")
            && value.get("id").and_then(Value::as_str) == Some(request_id.as_str())
        {
            if value.get("ok").and_then(Value::as_bool) != Some(true) {
                return Err(gateway_error(&value));
            }
            return Ok(value.get("payload").cloned().unwrap_or(Value::Null));
        }
    }
}

async fn write_workspace_files(workspace: &Path, employee: &Employee) -> Result<()> {
    tokio::fs::create_dir_all(workspace.join("memory")).await?;
    tokio::fs::create_dir_all(workspace.join("artifacts")).await?;
    tokio::fs::create_dir_all(workspace.join("notes")).await?;
    tokio::fs::write(
        workspace.join("IDENTITY.md"),
        format!(
            "# 员工身份\n\n姓名：{}\n职位：{}\n部门：{}\n员工 ID：{}\n",
            employee.name, employee.title, employee.department, employee.id
        ),
    )
    .await?;
    tokio::fs::write(
        workspace.join("SOUL.md"),
        format!(
            "# 工作风格\n\n性格：{}\n沟通方式：{}\n决策方式：{}\n说话习惯：{}\n行为习惯：{}\n工作方式：{}\n\n这些特征影响措辞和关注点，但不能降低事实准确性或偏离岗位职责，也不能替代系统的移动与环境规则。\n",
            employee.personality.traits.join("、"),
            employee.personality.communication_style,
            employee.personality.decision_style,
            employee.personality.speech_habits.join("；"),
            employee.personality.behavior_habits.join("；"),
            employee.work_style
        ),
    )
    .await?;
    tokio::fs::write(
        workspace.join("USER.md"),
        "# 公司用户\n\n只记录与公司工作相关的稳定信息。\n",
    )
    .await?;
    tokio::fs::write(
        workspace.join("MEMORY.md"),
        "# 长期记忆\n\n由 Runtime 在公司权限范围内维护。\n",
    )
    .await?;
    tokio::fs::write(workspace.join("AGENTS.md"), format!(
        "# AI Company Runtime Rules\n\n- 你是公司员工 {}。\n- 公司消息必须通过 AI Company Company Message Bus。\n- 不得伪造其他员工身份。\n- 不得访问其他员工 Workspace。\n- 不展示隐藏推理，只输出可交付的工作摘要。\n- 收到离职系统事件后，不启动新任务，整理交接内容。\n",
        employee.name
    )).await?;
    Ok(())
}

fn env_flag(name: &str) -> bool {
    matches!(
        env::var(name).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

fn safe_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

fn stdout_has_agent(stdout: &[u8], agent_id: &str) -> bool {
    serde_json::from_slice::<Value>(stdout)
        .ok()
        .map(|value| value.to_string().contains(agent_id))
        .unwrap_or(false)
}

fn command_error(command: &str, stderr: &[u8]) -> anyhow::Error {
    let detail = String::from_utf8_lossy(stderr).trim().to_string();
    if detail.is_empty() {
        anyhow!("{command} failed")
    } else {
        anyhow!("{command} failed: {detail}")
    }
}

fn public_error(error: &anyhow::Error) -> String {
    let message = error.to_string();
    match config().gateway_token {
        Some(token) if !token.is_empty() => message.replace(&token, "[redacted]"),
        _ => message,
    }
}

fn text_message(value: Value) -> Result<Message> {
    Ok(Message::Text(serde_json::to_string(&value)?.into()))
}

fn frame_value(frame: Message) -> Option<Value> {
    match frame {
        Message::Text(text) => serde_json::from_str(text.as_ref()).ok(),
        Message::Binary(bytes) => serde_json::from_slice(&bytes).ok(),
        _ => None,
    }
}

fn gateway_error(value: &Value) -> anyhow::Error {
    let error = value.get("error").unwrap_or(value);
    let code = error
        .get("code")
        .and_then(Value::as_str)
        .unwrap_or("OPENCLAW_ERROR");
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .unwrap_or("OpenClaw Gateway request failed");
    anyhow!("{code}: {message}")
}

fn assistant_text(value: &Value) -> Option<String> {
    if let Some(role) = value.get("role").and_then(Value::as_str) {
        if role != "assistant" && role != "model" {
            return None;
        }
    }
    for key in ["reply", "text", "deltaText"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            if !text.trim().is_empty() {
                return Some(text.to_string());
            }
        }
    }
    if let Some(message) = value.get("message") {
        if let Some(text) = message.as_str() {
            if !text.trim().is_empty() {
                return Some(text.to_string());
            }
        }
        if let Some(text) = assistant_text(message) {
            return Some(text);
        }
    }
    if let Some(content) = value.get("content") {
        if let Some(text) = content.as_str() {
            if !text.trim().is_empty() {
                return Some(text.to_string());
            }
        }
        if let Some(items) = content.as_array() {
            for item in items {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    if !text.trim().is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
        }
    }
    None
}
