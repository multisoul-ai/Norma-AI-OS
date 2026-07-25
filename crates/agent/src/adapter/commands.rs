use super::{AdapterCommand, AdapterError};
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
struct CreateAgentArgs {
    profile: String,
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
                "cursor" => AgentProfile::Cursor,
                "grok" => AgentProfile::Grok,
                other => {
                    return Err(AdapterError::InvalidArguments(format!(
                        "unsupported Agent profile: {other}"
                    )));
                }
            };
            Ok(AdapterCommand::CreateAgent { profile })
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
