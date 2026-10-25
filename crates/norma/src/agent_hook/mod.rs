pub mod cli;

mod approval;
mod approval_forward;
mod atomic_file;
mod conversation_observe;
mod cursor;
mod event;
mod forward;
mod hermes;
mod install;
mod json_config;
mod kimi;
mod opencode;
mod path_convention;
mod pi;
mod session_claim;
pub mod session_title;
mod zcode;

use crate::protocol::{AgentProfile, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::path::{Path, PathBuf};

pub use approval::{
    claude_permission_output, fresh_control_id, ApprovalDecision, ApprovalStatus, HookApproval,
    HookApprovalRequest, APPROVAL_TIMEOUT, ENV_CONTROL_RUN_ID,
};
pub use approval_forward::forward_permission;
pub use conversation_observe::{
    observe_conversation, ConversationObservation, ConversationObservationKind,
    ConversationObservationState, MAX_CONVERSATION_MESSAGE_CHARS,
};
pub use event::{
    extract_prompt_text, hook_events_for_source, parse_hook_event, task_state_for_provider_event,
};
pub use forward::{forward_event, forward_hook};
pub use hermes::{disable_hermes_plugin_blocking, enable_hermes_plugin_blocking};
pub use install::{
    agent_hooks_installed, agent_hooks_installed_at, agent_hooks_installed_for_source,
    install_agent_hooks, install_hook_config, uninstall_agent_hooks, uninstall_hook_config,
    uninstall_hook_config_except_hermes, unmanaged_hook_path,
};
pub use kimi::kimi_config_path_for;
pub use path_convention::{
    path_convention_enabled_from_env, session_context_output, ENV_PATH_CONVENTION,
    PATH_CONVENTION_PROMPT,
};

pub const ENV_NODE_ID: &str = "MULTISOUL_NODE_ID";
pub const ENV_AGENT_PROFILE: &str = "MULTISOUL_AGENT_PROFILE";
pub const ENV_HOOK_ENDPOINT: &str = "MULTISOUL_HOOK_ENDPOINT";
pub const ENV_HOOK_TOKEN: &str = "MULTISOUL_HOOK_TOKEN";
pub const AGENT_HOOK_BINARY_NAME: &str = "multisoul-agent-hook";
pub const OPENCODE_PLUGIN_FILE_NAME: &str = "multisoul-agent-hook.js";
pub const OPENCODE_PLUGIN_MARKER: &str = "multisoul-opencode-plugin-marker";
pub const PI_EXTENSION_FILE_NAME: &str = "norma-agent-hook.ts";
pub const PI_EXTENSION_MARKER: &str = "multisoul-pi-extension-marker";
pub const HERMES_PLUGIN_NAME: &str = "norma-agent-hook";
pub const HERMES_PLUGIN_MARKER: &str = "multisoul-hermes-plugin-marker";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookSource {
    Claude,
    Codex,
    Cursor,
    Grok,
    OpenCode,
    InfCode,
    Pi,
    Kimi,
    Hermes,
    ZCode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HookTarget {
    Claude,
    Codex,
    Cursor,
    Grok,
    OpenCode,
    InfCode,
    Pi,
    Kimi,
    Hermes,
    ZCode,
    All,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AgentHookEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_run_id: Option<String>,
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
    pub cursor_hooks: PathBuf,
    pub grok_hooks: PathBuf,
    pub opencode_plugin: PathBuf,
    pub infcode_settings: PathBuf,
    pub pi_extension: PathBuf,
    pub kimi_config: PathBuf,
    pub hermes_plugin_dir: PathBuf,
    pub zcode_config: PathBuf,
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
        "cursor" | "Cursor" => Some(HookSource::Cursor),
        "grok" | "Grok" => Some(HookSource::Grok),
        "opencode" | "OpenCode" => Some(HookSource::OpenCode),
        "infcode" | "InfCode" => Some(HookSource::InfCode),
        "pi" | "Pi" => Some(HookSource::Pi),
        "kimi" | "Kimi" => Some(HookSource::Kimi),
        "hermes" | "Hermes" => Some(HookSource::Hermes),
        "zcode" | "ZCode" => Some(HookSource::ZCode),
        _ => None,
    }
}

pub fn parse_hook_target(value: &str) -> Option<HookTarget> {
    match value {
        "claude" | "Claude" => Some(HookTarget::Claude),
        "codex" | "Codex" => Some(HookTarget::Codex),
        "cursor" | "Cursor" => Some(HookTarget::Cursor),
        "grok" | "Grok" => Some(HookTarget::Grok),
        "opencode" | "OpenCode" => Some(HookTarget::OpenCode),
        "infcode" | "InfCode" => Some(HookTarget::InfCode),
        "pi" | "Pi" => Some(HookTarget::Pi),
        "kimi" | "Kimi" => Some(HookTarget::Kimi),
        "hermes" | "Hermes" => Some(HookTarget::Hermes),
        "zcode" | "ZCode" => Some(HookTarget::ZCode),
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
        HookSource::Cursor => "cursor",
        HookSource::Grok => "grok",
        HookSource::OpenCode => "opencode",
        HookSource::InfCode => "infcode",
        HookSource::Pi => "pi",
        HookSource::Kimi => "kimi",
        HookSource::Hermes => "hermes",
        HookSource::ZCode => "zcode",
    }
}

pub fn profile_for_source(source: HookSource) -> AgentProfile {
    match source {
        HookSource::Claude => AgentProfile::Claude,
        HookSource::Codex => AgentProfile::Codex,
        HookSource::Cursor => AgentProfile::Cursor,
        HookSource::Grok => AgentProfile::Grok,
        HookSource::OpenCode => AgentProfile::OpenCode,
        HookSource::InfCode => AgentProfile::InfCode,
        HookSource::Pi => AgentProfile::Pi,
        HookSource::Kimi => AgentProfile::Kimi,
        HookSource::Hermes => AgentProfile::Hermes,
        HookSource::ZCode => AgentProfile::ZCode,
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
        cursor_hooks: home.join(".cursor").join("hooks.json"),
        grok_hooks: home
            .join(".grok")
            .join("hooks")
            .join("multisoul-agent-hook.json"),
        opencode_plugin: home
            .join(".config")
            .join("opencode")
            .join("plugins")
            .join(OPENCODE_PLUGIN_FILE_NAME),
        // InfCode 使用 Claude 式 command hooks，配置在 ~/.infcode/settings.json（已验证）。
        infcode_settings: home.join(".infcode").join("settings.json"),
        pi_extension: pi::pi_extension_path(
            &home,
            std::env::var("PI_CODING_AGENT_DIR").ok().as_deref(),
        ),
        kimi_config: kimi::kimi_config_path_for(&home, std::env::var_os("KIMI_CODE_HOME")),
        hermes_plugin_dir: std::env::var_os("HERMES_HOME")
            .map(|hermes_home| {
                PathBuf::from(hermes_home)
                    .join("plugins")
                    .join(HERMES_PLUGIN_NAME)
            })
            .unwrap_or_else(|| hermes_plugin_dir_for_home(&home)),
        zcode_config: home.join(".zcode").join("cli").join("config.json"),
    }
}

/// `<hermes_home>/plugins/norma-agent-hook`，`hermes_home` 默认 `<home>/.hermes`。
pub fn hermes_plugin_dir_for_home(home: &Path) -> PathBuf {
    home.join(".hermes")
        .join("plugins")
        .join(HERMES_PLUGIN_NAME)
}

pub fn resolve_agent_hook_command_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| resolve_agent_hook_command_path_from(path.as_path()))
}

pub fn resolve_agent_hook_command_path_from(app_executable: &Path) -> Option<PathBuf> {
    if let Some(dir) = app_executable.parent() {
        let sibling = dir.join(AGENT_HOOK_BINARY_NAME);
        if sibling.is_file() {
            return Some(sibling);
        }
    }
    resolve_from_path_env(AGENT_HOOK_BINARY_NAME)
}

pub(crate) fn shell_quote(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn resolve_from_path_env(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var).find_map(|dir| {
        let candidate = dir.join(name);
        candidate.is_file().then_some(candidate)
    })
}
