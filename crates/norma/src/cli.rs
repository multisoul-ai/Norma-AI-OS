use crate::protocol::{
    AgentErrorCode, AgentProfile, CardCommand, CardCommandError, CardKind, Placement, PreviewType,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

mod output;

use output::agent_error_output_format;
pub use output::{format_result, render_protocol_error, render_protocol_success};

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
    Present(PresentArgs),
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

#[derive(Debug, Args)]
struct PresentArgs {
    #[command(subcommand)]
    kind: PresentKind,
}

#[derive(Debug, Subcommand)]
enum PresentKind {
    Browser {
        url: Option<String>,
        #[arg(long = "new")]
        create_new: bool,
    },
    Preview {
        path: PathBuf,
        #[arg(long = "type", value_enum)]
        explicit_type: Option<PreviewTypeValue>,
        #[arg(long = "new")]
        create_new: bool,
    },
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

pub fn parse_process_invocation_from<I, T>(args: I) -> Result<CliInvocation, CliOutput>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    match parse_invocation_from(args.clone()) {
        Ok(invocation) => Ok(invocation),
        Err(error) => {
            let is_managed_command = args
                .iter()
                .any(|argument| matches!(argument.to_string_lossy().as_ref(), "agent" | "present"));
            if !is_managed_command {
                return Err(CliOutput {
                    stdout: String::new(),
                    stderr: error.to_string(),
                    exit_code: i32::from(error.exit_code()),
                });
            }
            let format = agent_error_output_format(&args);
            let message = error.to_string();
            let code = if message.contains("task must not be blank") {
                AgentErrorCode::EmptyTask
            } else {
                AgentErrorCode::InternalError
            };
            render_protocol_error(
                format,
                format!("local-parse-{}", std::process::id()),
                &CardCommandError::agent(code, message, false),
            )
            .map_err(|serialization_error| CliOutput {
                stdout: String::new(),
                stderr: serialization_error.to_string(),
                exit_code: 1,
            })
            .and_then(Err)
        }
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
            TopCommand::Present(present) => CliInvocation {
                command: present.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Json),
                wait_timeout_seconds: None,
            },
        }
    }
}

impl PresentArgs {
    fn into_command(self) -> CardCommand {
        let (target, create_new) = match self.kind {
            PresentKind::Browser { url, create_new } => (
                crate::protocol::PresentationTarget::Browser {
                    url: url.unwrap_or_default(),
                },
                create_new,
            ),
            PresentKind::Preview {
                path,
                explicit_type,
                create_new,
            } => (
                crate::protocol::PresentationTarget::Preview {
                    path,
                    explicit_type: explicit_type.map(PreviewType::from),
                },
                create_new,
            ),
        };
        CardCommand::Present { target, create_new }
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

fn non_blank_task(value: &str) -> Result<String, String> {
    if value.trim().is_empty() {
        Err("task must not be blank".to_string())
    } else {
        Ok(value.to_string())
    }
}
