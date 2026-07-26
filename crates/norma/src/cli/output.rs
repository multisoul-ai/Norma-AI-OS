use super::{CliOutput, OutputFormat};
use crate::protocol::{
    AgentErrorCode, AgentTaskReceiptStatus, CardCommandError, CardCommandResult,
};
use serde::Serialize;
use serde_json::json;
use std::ffi::OsString;

pub fn format_result(result: &CardCommandResult) -> String {
    match result {
        CardCommandResult::Created { node_id } => format!("created card {node_id}"),
        CardCommandResult::Deleted { node_id } => format!("deleted card {node_id}"),
        CardCommandResult::Sent { node_id } => format!("sent text to card {node_id}"),
        CardCommandResult::Focused { node_id } => format!("focused card {node_id}"),
        CardCommandResult::Listed { cards } => cards
            .iter()
            .map(|card| format!("{}\t{:?}\t{}", card.node_id, card.kind, card.title))
            .collect::<Vec<_>>()
            .join("\n"),
        CardCommandResult::AgentsListed { agents } => agents
            .iter()
            .map(|agent| {
                format!(
                    "{}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}",
                    agent.definition_id,
                    agent.profile,
                    agent.name,
                    agent.speech_alias.as_deref().unwrap_or("-"),
                    agent
                        .active_node_id
                        .map(|node_id| node_id.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    if agent.enabled { "enabled" } else { "disabled" },
                    if agent.busy { "busy" } else { "idle" },
                    agent.tracking,
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        CardCommandResult::AgentCreated {
            definition_id,
            name,
            node_id,
        } => format!(
            "created agent {name} ({definition_id}){}",
            node_id
                .map(|node_id| format!(" on card {node_id}"))
                .unwrap_or_default()
        ),
        CardCommandResult::AgentActivated {
            definition_id,
            name,
            node_id,
            reused,
        } => format!(
            "{} agent {name} ({definition_id}) on card {node_id}",
            if *reused { "reused" } else { "activated" }
        ),
        CardCommandResult::AgentTasksDispatched { receipts } => receipts
            .iter()
            .map(|receipt| {
                format!(
                    "{}\t{}\t{}\t{}\t{:?}\t{}\t{}\t{}",
                    receipt.task_id,
                    receipt.target_definition_id,
                    receipt.name,
                    receipt.node_id,
                    receipt.status,
                    receipt.tracking,
                    receipt.parent_task_id.as_deref().unwrap_or("-"),
                    receipt.root_task_id,
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        CardCommandResult::AgentTaskStatus { task } => {
            format!("{}\t{:?}\t{}", task.task_id, task.status, task.target_name)
        }
        CardCommandResult::AgentTaskCancelled { task_id, status } => {
            format!("{task_id}\t{status:?}")
        }
        CardCommandResult::Presented {
            node_id,
            kind,
            disposition,
            layout,
        } => format!("{node_id}\t{kind:?}\t{disposition:?}\t{layout:?}"),
    }
}

pub(super) fn agent_error_output_format(args: &[OsString]) -> OutputFormat {
    for (index, argument) in args.iter().enumerate() {
        let argument = argument.to_string_lossy();
        if argument == "--format=text" {
            return OutputFormat::Text;
        }
        if argument == "--format"
            && args
                .get(index + 1)
                .is_some_and(|value| value.to_string_lossy() == "text")
        {
            return OutputFormat::Text;
        }
    }
    OutputFormat::Json
}

pub fn render_protocol_success(
    format: OutputFormat,
    request_id: String,
    result: &CardCommandResult,
) -> Result<CliOutput, serde_json::Error> {
    match format {
        OutputFormat::Json => {
            let envelope = ProtocolSuccess {
                schema_version: 1,
                ok: true,
                request_id,
                result: result_json(result)?,
            };
            Ok(CliOutput {
                stdout: serde_json::to_string(&envelope)?,
                stderr: String::new(),
                exit_code: 0,
            })
        }
        OutputFormat::Text => Ok(CliOutput {
            stdout: format_result(result),
            stderr: String::new(),
            exit_code: 0,
        }),
    }
}

pub fn render_protocol_error(
    format: OutputFormat,
    request_id: String,
    error: &CardCommandError,
) -> Result<CliOutput, serde_json::Error> {
    let (failure, exit_code) = protocol_failure(error);
    match format {
        OutputFormat::Json => {
            let envelope = ProtocolError {
                schema_version: 1,
                ok: false,
                request_id,
                error: failure,
            };
            Ok(CliOutput {
                stdout: serde_json::to_string(&envelope)?,
                stderr: String::new(),
                exit_code,
            })
        }
        OutputFormat::Text => Ok(CliOutput {
            stdout: String::new(),
            stderr: error.to_string(),
            exit_code,
        }),
    }
}

#[derive(Serialize)]
struct ProtocolSuccess {
    schema_version: u8,
    ok: bool,
    request_id: String,
    result: serde_json::Value,
}

#[derive(Serialize)]
struct ProtocolError {
    schema_version: u8,
    ok: bool,
    request_id: String,
    error: ProtocolFailure,
}

#[derive(Serialize)]
struct ProtocolFailure {
    code: String,
    message: String,
    retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_status: Option<AgentTaskReceiptStatus>,
}

fn result_json(result: &CardCommandResult) -> Result<serde_json::Value, serde_json::Error> {
    Ok(match result {
        CardCommandResult::AgentsListed { agents } => json!({ "agents": agents }),
        CardCommandResult::AgentTasksDispatched { receipts } => json!({ "receipts": receipts }),
        CardCommandResult::AgentTaskStatus { task } => json!({ "task": task }),
        CardCommandResult::AgentTaskCancelled { task_id, status } => {
            json!({ "task_id": task_id, "status": status })
        }
        CardCommandResult::Presented {
            node_id,
            kind,
            disposition,
            layout,
        } => json!({
            "node_id": node_id,
            "kind": kind,
            "disposition": disposition,
            "layout": layout,
        }),
        other => serde_json::to_value(other)?,
    })
}

fn protocol_failure(error: &CardCommandError) -> (ProtocolFailure, i32) {
    match error {
        CardCommandError::Agent(error) => {
            let exit_code = match error.code {
                AgentErrorCode::AppUnavailable | AgentErrorCode::ProjectUnavailable => 3,
                AgentErrorCode::Unauthorized | AgentErrorCode::ProviderNotInstalled => 4,
                AgentErrorCode::WaitTimeout => 6,
                AgentErrorCode::InternalError => 1,
                _ => 5,
            };
            (
                ProtocolFailure {
                    code: serialized_code(error.code),
                    message: error.message.clone(),
                    retryable: error.retryable,
                    current_status: error.current_status,
                },
                exit_code,
            )
        }
        CardCommandError::Presentation(error) => (
            ProtocolFailure {
                code: serialized_code(error.code),
                message: error.message.clone(),
                retryable: error.retryable,
                current_status: None,
            },
            if error.code == crate::protocol::PresentationErrorCode::PresentationFailed {
                1
            } else {
                5
            },
        ),
        other => (
            ProtocolFailure {
                code: "internal_error".to_string(),
                message: other.to_string(),
                retryable: false,
                current_status: None,
            },
            1,
        ),
    }
}

fn serialized_code(code: impl Serialize) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_string))
        .unwrap_or_else(|| "internal_error".to_string())
}
