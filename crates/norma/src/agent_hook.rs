use crate::protocol::{AgentProfile, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const ENV_NODE_ID: &str = "MULTISOUL_NODE_ID";
pub const ENV_AGENT_PROFILE: &str = "MULTISOUL_AGENT_PROFILE";
pub const ENV_HOOK_ENDPOINT: &str = "MULTISOUL_HOOK_ENDPOINT";
pub const ENV_HOOK_TOKEN: &str = "MULTISOUL_HOOK_TOKEN";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookSource {
    Claude,
    Codex,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookTarget {
    Claude,
    Codex,
    All,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentHookEvent {
    pub node_id: NodeId,
    pub profile: AgentProfile,
    pub provider_event: String,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub tool_name: Option<String>,
    pub message: Option<String>,
    pub raw_json: Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentHookTaskState {
    Ready,
    Thinking,
    RunningTool { tool_name: Option<String> },
    WaitingApproval { tool_name: Option<String> },
    Done,
    Failed { message: String },
    Ended,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookInstallPaths {
    pub claude_settings: PathBuf,
    pub codex_hooks: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookForwardEnv {
    pub node_id: NodeId,
    pub endpoint: String,
    pub token: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookForwardOutcome {
    Noop,
    Posted,
    FailedButIgnored,
}

#[derive(Debug)]
pub enum AgentHookError {
    Io(std::io::Error),
    Json(serde_json::Error),
    MissingEventName,
    UnsupportedEndpoint(String),
}

impl fmt::Display for AgentHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::MissingEventName => write!(f, "hook JSON is missing hook_event_name"),
            Self::UnsupportedEndpoint(endpoint) => {
                write!(f, "unsupported hook endpoint {endpoint}")
            }
        }
    }
}

impl std::error::Error for AgentHookError {}

impl From<std::io::Error> for AgentHookError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for AgentHookError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub fn parse_hook_source(value: &str) -> Option<HookSource> {
    match value {
        "claude" | "Claude" => Some(HookSource::Claude),
        "codex" | "Codex" => Some(HookSource::Codex),
        _ => None,
    }
}

pub fn parse_hook_target(value: &str) -> Option<HookTarget> {
    match value {
        "claude" | "Claude" => Some(HookTarget::Claude),
        "codex" | "Codex" => Some(HookTarget::Codex),
        "all" | "All" => Some(HookTarget::All),
        _ => None,
    }
}

pub fn hook_forward_env_from_process() -> Option<HookForwardEnv> {
    let node_id = std::env::var(ENV_NODE_ID).ok()?.parse::<NodeId>().ok()?;
    let endpoint = std::env::var(ENV_HOOK_ENDPOINT).ok()?;
    let token = std::env::var(ENV_HOOK_TOKEN).ok()?;
    Some(HookForwardEnv {
        node_id,
        endpoint,
        token,
    })
}

pub fn hook_source_arg(source: HookSource) -> &'static str {
    match source {
        HookSource::Claude => "claude",
        HookSource::Codex => "codex",
    }
}

pub fn profile_for_source(source: HookSource) -> AgentProfile {
    match source {
        HookSource::Claude => AgentProfile::Claude,
        HookSource::Codex => AgentProfile::Codex,
    }
}

pub fn parse_hook_event(
    source: HookSource,
    node_id: NodeId,
    raw_json: Value,
) -> Result<AgentHookEvent, AgentHookError> {
    let provider_event = string_field(&raw_json, "hook_event_name")
        .or_else(|| string_field(&raw_json, "event_name"))
        .ok_or(AgentHookError::MissingEventName)?;
    Ok(AgentHookEvent {
        node_id,
        profile: profile_for_source(source),
        provider_event,
        session_id: string_field(&raw_json, "session_id"),
        turn_id: string_field(&raw_json, "turn_id"),
        tool_name: string_field(&raw_json, "tool_name"),
        message: string_field(&raw_json, "message")
            .or_else(|| string_field(&raw_json, "error"))
            .or_else(|| string_field(&raw_json, "reason")),
        raw_json,
    })
}

pub fn task_state_for_provider_event(event: &AgentHookEvent) -> Option<AgentHookTaskState> {
    match event.provider_event.as_str() {
        "SessionStart" => Some(AgentHookTaskState::Ready),
        "UserPromptSubmit" => Some(AgentHookTaskState::Thinking),
        "PreToolUse" => Some(AgentHookTaskState::RunningTool {
            tool_name: event.tool_name.clone(),
        }),
        "PostToolUse" | "SubagentStop" => Some(AgentHookTaskState::Thinking),
        "PermissionRequest" => Some(AgentHookTaskState::WaitingApproval {
            tool_name: event.tool_name.clone(),
        }),
        "PostToolUseFailure" | "StopFailure" => Some(AgentHookTaskState::Failed {
            message: event
                .message
                .clone()
                .unwrap_or_else(|| event.provider_event.clone()),
        }),
        "Stop" => Some(AgentHookTaskState::Done),
        "SessionEnd" => Some(AgentHookTaskState::Ended),
        "SubagentStart" => Some(AgentHookTaskState::RunningTool {
            tool_name: Some("subagent".to_string()),
        }),
        _ => None,
    }
}

pub fn hook_events_for_source(source: HookSource) -> &'static [&'static str] {
    match source {
        HookSource::Claude => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "PostToolUseFailure",
            "StopFailure",
            "Stop",
            "SessionEnd",
            "SubagentStart",
            "SubagentStop",
        ],
        HookSource::Codex => &[
            "SessionStart",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "Stop",
            "SubagentStart",
            "SubagentStop",
        ],
    }
}

pub fn build_hook_command(command_path: &Path, source: HookSource) -> String {
    format!(
        "{} forward --source {}",
        shell_quote(command_path),
        hook_source_arg(source)
    )
}

pub fn default_install_paths() -> HookInstallPaths {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    HookInstallPaths {
        claude_settings: home.join(".claude").join("settings.json"),
        codex_hooks: home.join(".codex").join("hooks.json"),
    }
}

pub fn install_hook_config(
    target: HookTarget,
    command_path: &Path,
    paths: &HookInstallPaths,
) -> Result<(), AgentHookError> {
    if matches!(target, HookTarget::Claude | HookTarget::All) {
        install_for_source(
            paths.claude_settings.as_path(),
            command_path,
            HookSource::Claude,
        )?;
    }
    if matches!(target, HookTarget::Codex | HookTarget::All) {
        install_for_source(paths.codex_hooks.as_path(), command_path, HookSource::Codex)?;
    }
    Ok(())
}

pub fn forward_hook(
    source: HookSource,
    raw_json: Value,
    env: Option<HookForwardEnv>,
) -> HookForwardOutcome {
    let Some(env) = env else {
        return HookForwardOutcome::Noop;
    };
    let Ok(event) = parse_hook_event(source, env.node_id, raw_json) else {
        return HookForwardOutcome::Noop;
    };
    match forward_event(&env.endpoint, &env.token, &event) {
        Ok(()) => HookForwardOutcome::Posted,
        Err(error) => {
            eprintln!("multisoul-agent-hook: ignored forward failure: {error}");
            HookForwardOutcome::FailedButIgnored
        }
    }
}

pub fn forward_event(
    endpoint: &str,
    token: &str,
    event: &AgentHookEvent,
) -> Result<(), AgentHookError> {
    let (host, port, path) = parse_http_endpoint(endpoint)?;
    let body = serde_json::to_vec(event)?;
    let mut stream = TcpStream::connect((host.as_str(), port))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    stream.flush()?;
    let mut response = [0u8; 32];
    let read = stream.read(&mut response)?;
    let status_line = std::str::from_utf8(&response[..read]).unwrap_or("");
    if status_line.starts_with("HTTP/1.1 2") || status_line.starts_with("HTTP/1.0 2") {
        Ok(())
    } else {
        Err(AgentHookError::UnsupportedEndpoint(status_line.to_string()))
    }
}

fn install_for_source(
    path: &Path,
    command_path: &Path,
    source: HookSource,
) -> Result<(), AgentHookError> {
    let mut root = read_json_or_empty(path)?;
    let command = build_hook_command(command_path, source);
    for event_name in hook_events_for_source(source) {
        add_command_hook(&mut root, event_name, &command);
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}

fn add_command_hook(root: &mut Value, event_name: &str, command: &str) {
    ensure_object(root);
    let hooks = root
        .as_object_mut()
        .expect("object ensured")
        .entry("hooks")
        .or_insert_with(|| json!({}));
    ensure_object(hooks);
    let event_hooks = hooks
        .as_object_mut()
        .expect("object ensured")
        .entry(event_name)
        .or_insert_with(|| json!([]));
    if command_exists(event_hooks, command) {
        return;
    }
    ensure_array(event_hooks);
    event_hooks
        .as_array_mut()
        .expect("array ensured")
        .push(json!({
            "matcher": "*",
            "hooks": [{
                "type": "command",
                "command": command,
                "timeout": 5
            }]
        }));
}

fn command_exists(event_hooks: &Value, command: &str) -> bool {
    event_hooks.as_array().is_some_and(|groups| {
        groups.iter().any(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|handlers| {
                    handlers.iter().any(|handler| {
                        handler.get("command").and_then(Value::as_str) == Some(command)
                    })
                })
        })
    })
}

fn read_json_or_empty(path: &Path) -> Result<Value, AgentHookError> {
    if !path.exists() {
        return Ok(json!({}));
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn ensure_object(value: &mut Value) {
    if !value.is_object() {
        *value = json!({});
    }
}

fn ensure_array(value: &mut Value) {
    if !value.is_array() {
        *value = json!([]);
    }
}

fn parse_http_endpoint(endpoint: &str) -> Result<(String, u16, String), AgentHookError> {
    let rest = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| AgentHookError::UnsupportedEndpoint(endpoint.to_string()))?;
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let (host, port) = authority
        .rsplit_once(':')
        .ok_or_else(|| AgentHookError::UnsupportedEndpoint(endpoint.to_string()))?;
    let port = port
        .parse::<u16>()
        .map_err(|_| AgentHookError::UnsupportedEndpoint(endpoint.to_string()))?;
    Ok((host.to_string(), port, format!("/{path}")))
}

fn string_field(raw: &Value, field: &str) -> Option<String> {
    raw.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}
