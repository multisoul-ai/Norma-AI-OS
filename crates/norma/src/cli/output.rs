use super::{CliOutput, OutputFormat};
use crate::cli_runtime::AgentSelfInfo;
use crate::protocol::{
    AgentErrorCode, AgentProfile, AgentTaskReceiptStatus, AgentTaskStatus, CardCommandError,
    CardCommandResult,
};
use serde::Serialize;
use serde_json::json;
use std::ffi::OsString;

pub fn format_result(result: &CardCommandResult) -> String {
    match result {
        CardCommandResult::Brain { output } => output.clone(),
        CardCommandResult::Doctor { report } => format!(
            "App {}\nCLI {}\nProtocol {}\nSocket {}\nHook ingress {}\nProvider health {}\nSkill health {}\nOpen workspaces {}",
            report.app_version,
            report.cli_version,
            report.protocol_version,
            report.socket,
            report.hook_ingress,
            report.provider_health,
            report.skill_health,
            report.open_workspace_count
        ),
        CardCommandResult::WorkspacesListed { workspaces } => workspaces
            .iter()
            .map(|workspace| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\t{}",
                    workspace.workspace_id,
                    workspace.display_name,
                    workspace.canonical_root.display(),
                    if workspace.active { "active" } else { "inactive" },
                    workspace.route_state,
                    workspace.agent_count
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
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
        CardCommandResult::AgentTaskStatus { task }
        | CardCommandResult::AgentTaskResult { task } => format_task_status(task),
        CardCommandResult::AgentTaskCancelled { task_id, status } => {
            format!("{task_id}\t{status:?}")
        }
        CardCommandResult::Presented {
            node_id,
            kind,
            disposition,
            layout,
        } => format!("{node_id}\t{kind:?}\t{disposition:?}\t{layout:?}"),
        CardCommandResult::EgoPresented {
            status,
            disposition,
        } => format!(
            "{}\t{}\t{:?}\t{:?}",
            status.node_id,
            status.task_space_id.as_deref().unwrap_or("-"),
            status.phase,
            disposition
        ),
        CardCommandResult::EgoStatus { status } => format!(
            "{}\t{}\t{:?}\t{:?}",
            status.node_id,
            status.task_space_id.as_deref().unwrap_or("-"),
            status.phase,
            status.ownership
        ),
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

pub fn render_agent_self_success(
    format: OutputFormat,
    request_id: String,
    info: &AgentSelfInfo,
) -> Result<CliOutput, serde_json::Error> {
    match format {
        OutputFormat::Json => {
            let envelope = json!({
                "schema_version": 1,
                "ok": true,
                "request_id": request_id,
                "result": {
                    "self": {
                        "workspace_id": info.workspace_id,
                        "definition_id": info.definition_id,
                        "node_id": info.node_id,
                        "name": info.name,
                        "speech_alias": info.speech_alias,
                        "profile": profile_slug(info.profile),
                    }
                },
            });
            Ok(CliOutput {
                stdout: serde_json::to_string(&envelope)?,
                stderr: String::new(),
                exit_code: 0,
            })
        }
        OutputFormat::Text => Ok(CliOutput {
            stdout: format!(
                "{}\t{}\t{}\t{}\t{}",
                info.name,
                info.definition_id,
                info.node_id,
                info.speech_alias.as_deref().unwrap_or("-"),
                profile_slug(info.profile),
            ),
            stderr: String::new(),
            exit_code: 0,
        }),
    }
}

fn profile_slug(profile: AgentProfile) -> &'static str {
    match profile {
        AgentProfile::Claude => "claude",
        AgentProfile::Codex => "codex",
        AgentProfile::Cursor => "cursor",
        AgentProfile::Grok => "grok",
        AgentProfile::OpenCode => "opencode",
        AgentProfile::InfCode => "infcode",
        AgentProfile::Kimi => "kimi",
        AgentProfile::ZCode => "zcode",
        AgentProfile::Pi => "pi",
        AgentProfile::Hermes => "hermes",
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
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_id: Option<String>,
    #[serde(skip_serializing_if = "is_false")]
    outcome_unknown: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn result_json(result: &CardCommandResult) -> Result<serde_json::Value, serde_json::Error> {
    Ok(match result {
        CardCommandResult::Doctor { report } => json!({ "report": report }),
        CardCommandResult::WorkspacesListed { workspaces } => {
            json!({ "workspaces": workspaces })
        }
        CardCommandResult::AgentsListed { agents } => json!({ "agents": agents }),
        CardCommandResult::AgentTasksDispatched { receipts } => json!({ "receipts": receipts }),
        CardCommandResult::AgentTaskStatus { task }
        | CardCommandResult::AgentTaskResult { task } => json!({ "task": task }),
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
        CardCommandResult::EgoPresented {
            status,
            disposition,
        } => json!({ "status": status, "disposition": disposition }),
        CardCommandResult::EgoStatus { status } => json!({ "status": status }),
        other => serde_json::to_value(other)?,
    })
}

fn format_task_status(task: &AgentTaskStatus) -> String {
    let result = task
        .result
        .as_ref()
        .and_then(|result| result.text.as_deref())
        .unwrap_or("-");
    format!(
        "{}\t{:?}\t{}\t{}",
        task.task_id, task.status, task.target_name, result
    )
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
                    operation_id: error.operation_id.clone(),
                    outcome_unknown: error.outcome_unknown,
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
                operation_id: None,
                outcome_unknown: false,
            },
            if error.code == crate::protocol::PresentationErrorCode::PresentationFailed {
                1
            } else {
                5
            },
        ),
        CardCommandError::Ego(error) => (
            ProtocolFailure {
                code: serialized_code(error.code),
                message: error.message.clone(),
                retryable: error.retryable,
                current_status: None,
                operation_id: None,
                outcome_unknown: false,
            },
            if error.code == crate::protocol::EgoErrorCode::InternalError {
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
                operation_id: None,
                outcome_unknown: false,
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
