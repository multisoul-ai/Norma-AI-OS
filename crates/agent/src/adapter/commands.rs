use super::{AdapterCommand, AdapterError, WorkspaceSnapshotScope};
use norma::protocol::{AgentProfile, PreviewType};
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FindAgentTasksArgs {
    task_id: Option<u64>,
    agent: Option<String>,
    #[serde(default)]
    states: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceSnapshotArgs {
    scope: WorkspaceSnapshotScope,
    area_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignWorkspaceTaskArgs {
    workspace_id: String,
    area_id: String,
    node_id: u64,
    task: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenWorkspaceArtifactArgs {
    workspace_id: String,
    area_id: String,
    node_id: u64,
    turn_id: String,
    source_path: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocateWorkspaceAgentArgs {
    workspace_id: String,
    area_id: String,
    node_id: u64,
    approval_run_id: Option<String>,
    approval_turn_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateAgentArgs {
    profile: String,
    task: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AssignAgentTaskArgs {
    agent: String,
    task: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyAgentArgs {
    agent: String,
    message: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CancelAgentReplyArgs {
    reply_id: Option<u64>,
    agent: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TaskIdArgs {
    task_id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBrowserArgs {
    url: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreatePreviewArgs {
    path: String,
    preview_type: Option<PreviewTypeArg>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PreviewTypeArg {
    Markdown,
    Text,
    Image,
}

impl From<PreviewTypeArg> for PreviewType {
    fn from(value: PreviewTypeArg) -> Self {
        match value {
            PreviewTypeArg::Markdown => Self::Markdown,
            PreviewTypeArg::Text => Self::Text,
            PreviewTypeArg::Image => Self::Image,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeIdArgs {
    node_id: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteCanvasCardArgs {
    node_id: Option<u64>,
    position: Option<u64>,
    agent: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalInputArgs {
    node_id: u64,
    text: String,
    submit: bool,
}

pub(super) fn parse_adapter_command(
    name: &str,
    arguments: Value,
) -> Result<AdapterCommand, AdapterError> {
    match name {
        "locate_workspace_agent" => {
            let args: LocateWorkspaceAgentArgs = parse(arguments)?;
            require_positive("node_id", args.node_id)?;
            let approval_run_id = clean_optional(args.approval_run_id);
            let approval_turn_id = clean_optional(args.approval_turn_id);
            if approval_run_id.is_some() != approval_turn_id.is_some() {
                return Err(AdapterError::InvalidArguments(
                    "审批定位必须同时提供 run_id 和 turn_id".into(),
                ));
            }
            Ok(AdapterCommand::LocateWorkspaceAgent {
                workspace_id: require_text("workspace_id", args.workspace_id)?,
                area_id: require_text("area_id", args.area_id)?,
                node_id: args.node_id,
                approval_run_id,
                approval_turn_id,
            })
        }
        "open_workspace_artifact" => {
            let args: OpenWorkspaceArtifactArgs = parse(arguments)?;
            require_positive("node_id", args.node_id)?;
            Ok(AdapterCommand::OpenWorkspaceArtifact {
                workspace_id: require_text("workspace_id", args.workspace_id)?,
                area_id: require_text("area_id", args.area_id)?,
                node_id: args.node_id,
                turn_id: require_text("turn_id", args.turn_id)?,
                source_path: require_text("source_path", args.source_path)?,
                version: require_text("version", args.version)?,
            })
        }
        "assign_workspace_task" => {
            let args: AssignWorkspaceTaskArgs = parse(arguments)?;
            require_positive("node_id", args.node_id)?;
            Ok(AdapterCommand::AssignWorkspaceTask {
                workspace_id: require_text("workspace_id", args.workspace_id)?,
                area_id: require_text("area_id", args.area_id)?,
                node_id: args.node_id,
                task: require_text("task", args.task)?,
            })
        }
        "workspace_snapshot" => {
            let args: WorkspaceSnapshotArgs = parse(arguments)?;
            let area_id = clean_optional(args.area_id);
            if (args.scope == WorkspaceSnapshotScope::Area) != area_id.is_some() {
                return Err(AdapterError::InvalidArguments(
                    "area_id 只在 scope=area 时必填".into(),
                ));
            }
            Ok(AdapterCommand::WorkspaceSnapshot {
                scope: args.scope,
                area_id,
            })
        }
        "workspace_memory" => {
            let mut fields = arguments
                .as_object()
                .cloned()
                .ok_or_else(|| AdapterError::InvalidArguments("invalid workspace memory".into()))?;
            let area_id = fields
                .remove("area_id")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(|| AdapterError::InvalidArguments("area_id required".into()))?;
            if area_id.is_empty()
                || area_id.len() > 128
                || !area_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            {
                return Err(AdapterError::InvalidArguments("invalid area_id".into()));
            }
            let action = fields
                .remove("action")
                .and_then(|v| v.as_str().map(str::to_owned))
                .ok_or_else(|| AdapterError::InvalidArguments("action required".into()))?;
            if !matches!(action.as_str(), "list" | "search" | "read")
                || !matches!(
                    fields.get("scope").and_then(Value::as_str),
                    Some("project" | "worktree")
                )
            {
                return Err(AdapterError::InvalidArguments(
                    "read-only project/worktree scope required".into(),
                ));
            }
            let command = crate::memory_access::parse_memory_command(
                &format!("memory_{action}"),
                Value::Object(fields),
            )?;
            Ok(AdapterCommand::WorkspaceMemory { area_id, command })
        }
        "memory_list" | "memory_search" | "memory_read" | "memory_write" => {
            crate::memory_access::parse_memory_command(name, arguments).map(AdapterCommand::Memory)
        }
        "find_agent_tasks" => {
            let args: FindAgentTasksArgs = parse(arguments)?;
            Ok(AdapterCommand::FindAgentTasks {
                task_id: args.task_id,
                agent: clean_optional(args.agent),
                states: args.states,
            })
        }
        "create_agent" => {
            let args: CreateAgentArgs = parse(arguments)?;
            let profile = match args.profile.as_str() {
                "codex" => AgentProfile::Codex,
                "claude" => AgentProfile::Claude,
                "opencode" => AgentProfile::OpenCode,
                "infcode" => AgentProfile::InfCode,
                "cursor" => AgentProfile::Cursor,
                "grok" => AgentProfile::Grok,
                "kimi" => AgentProfile::Kimi,
                "zcode" => AgentProfile::ZCode,
                "pi" => AgentProfile::Pi,
                "hermes" => AgentProfile::Hermes,
                other => {
                    return Err(AdapterError::InvalidArguments(format!(
                        "unsupported Agent profile: {other}"
                    )));
                }
            };
            Ok(AdapterCommand::CreateAgent {
                profile,
                task: clean_optional(args.task),
            })
        }
        "assign_agent_task" => {
            let args: AssignAgentTaskArgs = parse(arguments)?;
            let agent = require_text("agent", args.agent)?;
            let task = require_text("task", args.task)?;
            Ok(AdapterCommand::AssignAgentTask { agent, task })
        }
        "cancel_agent_task" => {
            let args: TaskIdArgs = parse(arguments)?;
            require_positive("task_id", args.task_id)?;
            Ok(AdapterCommand::CancelAgentTask {
                task_id: args.task_id,
            })
        }
        "reply_agent" => {
            let args: ReplyAgentArgs = parse(arguments)?;
            let agent = require_text("agent", args.agent)?;
            let message = require_text("message", args.message)?;
            Ok(AdapterCommand::ReplyAgent { agent, message })
        }
        "cancel_agent_reply" => {
            let args: CancelAgentReplyArgs = parse(arguments)?;
            let reply_id = match args.reply_id {
                Some(value) => {
                    require_positive("reply_id", value)?;
                    Some(value)
                }
                None => None,
            };
            let agent = clean_optional(args.agent);
            if reply_id.is_none() && agent.is_none() {
                return Err(AdapterError::InvalidArguments(
                    "cancel_agent_reply requires reply_id or agent".to_string(),
                ));
            }
            Ok(AdapterCommand::CancelAgentReply { reply_id, agent })
        }
        "create_terminal" => {
            let _: EmptyArgs = parse(arguments)?;
            Ok(AdapterCommand::CreateTerminal)
        }
        "create_browser" => {
            let args: CreateBrowserArgs = parse(arguments)?;
            Ok(AdapterCommand::CreateBrowser {
                url: clean_optional(args.url),
            })
        }
        "create_preview" => {
            let args: CreatePreviewArgs = parse(arguments)?;
            let path = require_text("path", args.path)?;
            Ok(AdapterCommand::CreatePreview {
                path: PathBuf::from(path),
                explicit_type: args.preview_type.map(Into::into),
            })
        }
        "focus_card" => {
            let args: NodeIdArgs = parse(arguments)?;
            require_positive("node_id", args.node_id)?;
            Ok(AdapterCommand::FocusCard {
                node_id: args.node_id,
            })
        }
        "send_terminal_input" => {
            let args: TerminalInputArgs = parse(arguments)?;
            require_positive("node_id", args.node_id)?;
            Ok(AdapterCommand::SendTerminalInput {
                node_id: args.node_id,
                text: args.text,
                submit: args.submit,
            })
        }
        "delete_canvas_card" => {
            let args: DeleteCanvasCardArgs = parse(arguments)?;
            match (args.node_id, args.position, args.agent) {
                (Some(node_id), None, None) => {
                    require_positive("node_id", node_id)?;
                    Ok(AdapterCommand::DeleteCanvasCard { node_id })
                }
                (None, Some(position), None) => {
                    require_positive("position", position)?;
                    Ok(AdapterCommand::DeleteCanvasCardAtPosition { position })
                }
                (None, None, Some(agent)) => Ok(AdapterCommand::DeleteAgentCard {
                    agent: require_text("agent", agent)?,
                }),
                _ => Err(AdapterError::InvalidArguments(
                    "delete_canvas_card requires exactly one of node_id, position, or agent"
                        .to_string(),
                )),
            }
        }
        other => Err(AdapterError::InvalidArguments(format!(
            "unknown Norma tool: {other}"
        ))),
    }
}

fn parse<T: for<'de> Deserialize<'de>>(value: Value) -> Result<T, AdapterError> {
    serde_json::from_value(value).map_err(|error| AdapterError::InvalidArguments(error.to_string()))
}

fn clean_optional(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn require_text(field: &str, value: String) -> Result<String, AdapterError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AdapterError::InvalidArguments(format!(
            "{field} must not be empty"
        )));
    }
    Ok(trimmed.to_string())
}

fn require_positive(field: &str, value: u64) -> Result<(), AdapterError> {
    if value == 0 {
        return Err(AdapterError::InvalidArguments(format!(
            "{field} must be greater than zero"
        )));
    }
    Ok(())
}

impl AdapterCommand {
    pub fn is_read_only(&self) -> bool {
        match self {
            Self::FindAgentTasks { .. } => true,
            Self::Memory(command) | Self::WorkspaceMemory { command, .. } => {
                !matches!(command, crate::MemoryCommand::Write { .. })
            }
            _ => false,
        }
    }
}
