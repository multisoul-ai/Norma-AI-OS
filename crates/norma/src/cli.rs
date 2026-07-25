use crate::protocol::{
    AgentErrorCode, AgentProfile, CardCommand, CardCommandError, CardCommandResult, CardKind,
    Placement, PreviewType,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use serde_json::json;
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Json,
    Text,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliInvocation {
    pub command: CardCommand,
    pub format: OutputFormat,
    pub wait_timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Parser)]
#[command(name = "norma")]
#[command(about = "Control cards in a running Norma OS app")]
struct NormaArgs {
    #[arg(long, value_enum, global = true)]
    format: Option<OutputFormat>,
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Debug, Subcommand)]
enum TopCommand {
    Card(CardArgs),
    Agent(AgentArgs),
}

#[derive(Debug, Args)]
struct CardArgs {
    #[command(subcommand)]
    command: CardCliCommand,
}

#[derive(Debug, Subcommand)]
enum CardCliCommand {
    Create(CreateArgs),
    Send(SendArgs),
    Delete { node_id: u64 },
    Focus { node_id: u64 },
    List,
}

#[derive(Debug, Args)]
struct AgentArgs {
    #[command(subcommand)]
    command: AgentCliCommand,
}

#[derive(Debug, Subcommand)]
enum AgentCliCommand {
    List,
    Task(AgentTaskArgs),
}

#[derive(Debug, Args)]
struct AgentTaskArgs {
    #[command(subcommand)]
    command: AgentTaskCommand,
}

#[derive(Debug, Subcommand)]
enum AgentTaskCommand {
    Assign {
        agent: String,
        #[arg(long, value_parser = non_blank_task)]
        task: String,
        #[arg(long)]
        force_untracked: bool,
    },
    Status {
        task_id: String,
    },
    Wait {
        task_id: String,
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    Cancel {
        task_id: String,
    },
}

#[derive(Debug, Args)]
struct SendArgs {
    node_id: u64,
    text: String,
    #[arg(long)]
    submit: bool,
}

#[derive(Debug, Subcommand)]
enum CreateKind {
    Terminal {
        #[arg(long)]
        shell: Option<String>,
    },
    Agent {
        #[arg(long, value_enum)]
        profile: AgentProfileValue,
    },
    Preview {
        path: PathBuf,
        #[arg(long = "type", value_enum)]
        explicit_type: Option<PreviewTypeValue>,
    },
}

#[derive(Debug, Args)]
struct CreateArgs {
    #[command(subcommand)]
    kind: CreateKind,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AgentProfileValue {
    Claude,
    Codex,
    Cursor,
    Grok,
    #[value(name = "opencode")]
    OpenCode,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PreviewTypeValue {
    Markdown,
    Text,
    Image,
}

pub fn parse_command_from<I, T>(args: I) -> Result<CardCommand, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    Ok(parse_invocation_from(args)?.command)
}

pub fn parse_invocation_from<I, T>(args: I) -> Result<CliInvocation, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    NormaArgs::try_parse_from(args).map(NormaArgs::into_invocation)
}

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
                    "{}\t{:?}\t{}\t{}",
                    agent.definition_id,
                    agent.profile,
                    agent.name,
                    if agent.enabled { "enabled" } else { "disabled" }
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
                    "{}\t{}\t{:?}",
                    receipt.name, receipt.node_id, receipt.status
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
    }
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

impl NormaArgs {
    fn into_invocation(self) -> CliInvocation {
        let explicit_format = self.format;
        match self.command {
            TopCommand::Card(card) => CliInvocation {
                command: card.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Text),
                wait_timeout_seconds: None,
            },
            TopCommand::Agent(agent) => {
                let (command, wait_timeout_seconds) = agent.into_command();
                CliInvocation {
                    command,
                    format: explicit_format.unwrap_or(OutputFormat::Json),
                    wait_timeout_seconds,
                }
            }
        }
    }
}

impl CardArgs {
    fn into_command(self) -> CardCommand {
        match self.command {
            CardCliCommand::Create(create) => create.into_command(),
            CardCliCommand::Send(args) => CardCommand::SendText {
                node_id: args.node_id,
                text: args.text,
                submit: args.submit,
            },
            CardCliCommand::Delete { node_id } => CardCommand::Delete { node_id },
            CardCliCommand::Focus { node_id } => CardCommand::Focus { node_id },
            CardCliCommand::List => CardCommand::List,
        }
    }
}

impl AgentArgs {
    fn into_command(self) -> (CardCommand, Option<u64>) {
        match self.command {
            AgentCliCommand::List => (CardCommand::ListAgents, None),
            AgentCliCommand::Task(task) => task.into_command(),
        }
    }
}

impl AgentTaskArgs {
    fn into_command(self) -> (CardCommand, Option<u64>) {
        match self.command {
            AgentTaskCommand::Assign {
                agent,
                task,
                force_untracked,
            } => (
                CardCommand::DispatchAgentTasks {
                    assignments: vec![crate::protocol::AgentTaskAssignment { name: agent, task }],
                    force_untracked,
                },
                None,
            ),
            AgentTaskCommand::Status { task_id } => {
                (CardCommand::AgentTaskStatus { task_ref: task_id }, None)
            }
            AgentTaskCommand::Wait { task_id, timeout } => (
                CardCommand::AgentTaskStatus { task_ref: task_id },
                Some(timeout),
            ),
            AgentTaskCommand::Cancel { task_id } => {
                (CardCommand::AgentTaskCancel { task_ref: task_id }, None)
            }
        }
    }
}

impl CreateArgs {
    fn into_command(self) -> CardCommand {
        let kind = match self.kind {
            CreateKind::Terminal { shell } => CardKind::Terminal { shell },
            CreateKind::Agent { profile } => {
                return CardCommand::CreateAgent {
                    name: None,
                    profile: profile.into(),
                    activate: true,
                }
            }
            CreateKind::Preview {
                path,
                explicit_type,
            } => CardKind::Preview {
                path,
                explicit_type: explicit_type.map(PreviewType::from),
            },
        };
        CardCommand::Create {
            kind,
            placement: Placement::VisibleArea,
        }
    }
}

impl From<AgentProfileValue> for AgentProfile {
    fn from(value: AgentProfileValue) -> Self {
        match value {
            AgentProfileValue::Claude => AgentProfile::Claude,
            AgentProfileValue::Codex => AgentProfile::Codex,
            AgentProfileValue::Cursor => AgentProfile::Cursor,
            AgentProfileValue::Grok => AgentProfile::Grok,
            AgentProfileValue::OpenCode => AgentProfile::OpenCode,
        }
    }
}

impl From<PreviewTypeValue> for PreviewType {
    fn from(value: PreviewTypeValue) -> Self {
        match value {
            PreviewTypeValue::Markdown => PreviewType::Markdown,
            PreviewTypeValue::Text => PreviewType::Text,
            PreviewTypeValue::Image => PreviewType::Image,
        }
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
    code: AgentErrorCode,
    message: String,
    retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_status: Option<crate::protocol::AgentTaskReceiptStatus>,
}

fn result_json(result: &CardCommandResult) -> Result<serde_json::Value, serde_json::Error> {
    Ok(match result {
        CardCommandResult::AgentsListed { agents } => json!({ "agents": agents }),
        CardCommandResult::AgentTasksDispatched { receipts } => json!({ "receipts": receipts }),
        CardCommandResult::AgentTaskStatus { task } => json!({ "task": task }),
        CardCommandResult::AgentTaskCancelled { task_id, status } => {
            json!({ "task_id": task_id, "status": status })
        }
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
                    code: error.code,
                    message: error.message.clone(),
                    retryable: error.retryable,
                    current_status: error.current_status,
                },
                exit_code,
            )
        }
        other => (
            ProtocolFailure {
                code: AgentErrorCode::InternalError,
                message: other.to_string(),
                retryable: false,
                current_status: None,
            },
            1,
        ),
    }
}

fn non_blank_task(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("task must not be blank".to_string())
    } else {
        Ok(value.to_string())
    }
}
