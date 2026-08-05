use crate::protocol::{
    AgentErrorCode, CardCommand, CardCommandError, CardKind, EgoCommand, Placement, PreviewType,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;
mod operation;
mod output;
mod task_command;
mod value;

use operation::{non_blank_message, non_blank_operation_id, non_blank_task};
use output::agent_error_output_format;
pub use output::{
    format_result, render_agent_self_success, render_protocol_error, render_protocol_success,
};

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
    pub target_workspace_id: Option<String>,
    pub operation_id: Option<String>,
    /// 若为 true，响应按当前进程身份做 `agent self` 投影，而非直接渲染 Agent 列表。
    pub project_self: bool,
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
#[command(version)]
struct NormaArgs {
    #[arg(long, value_enum, global = true)]
    format: Option<OutputFormat>,
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Debug, Subcommand)]
enum TopCommand {
    Doctor,
    Workspace(WorkspaceArgs),
    Card(CardArgs),
    Agent(AgentArgs),
    Present(PresentArgs),
    Ego(EgoArgs),
}

#[derive(Debug, Args)]
struct WorkspaceArgs {
    #[command(subcommand)]
    command: WorkspaceCliCommand,
}

#[derive(Debug, Subcommand)]
enum WorkspaceCliCommand {
    List,
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
    List {
        #[arg(long)]
        workspace: Option<String>,
    },
    /// 打印当前 Agent Session 自身的稳定身份（复用 ListAgents，不做跨 Workspace 名称比较）。
    #[command(name = "self")]
    Myself,
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
        #[arg(long)]
        workspace: Option<String>,
        #[arg(long, value_parser = non_blank_task)]
        task: String,
        #[arg(long, value_parser = non_blank_operation_id)]
        operation_id: Option<String>,
        #[arg(long)]
        force_untracked: bool,
    },
    Status {
        task_id: String,
    },
    Result {
        task_id: String,
    },
    Wait {
        task_id: String,
        #[arg(long, default_value_t = 60)]
        timeout: u64,
    },
    Cancel {
        task_id: String,
        #[arg(long, value_parser = non_blank_operation_id)]
        operation_id: Option<String>,
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
    Ego {
        #[arg(long = "new")]
        create_new: bool,
    },
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

#[derive(Debug, Args)]
struct EgoArgs {
    #[command(subcommand)]
    command: EgoCliCommand,
}

#[derive(Debug, Subcommand)]
enum EgoCliCommand {
    Status {
        node_id: u64,
    },
    Running {
        node_id: u64,
    },
    Handoff {
        node_id: u64,
        #[arg(long, value_parser = non_blank_message)]
        message: String,
    },
    Complete {
        node_id: u64,
    },
    Fail {
        node_id: u64,
        #[arg(long, value_parser = non_blank_message)]
        message: String,
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
            if error.kind() == clap::error::ErrorKind::DisplayVersion {
                return Err(CliOutput {
                    stdout: error.to_string().trim().to_string(),
                    stderr: String::new(),
                    exit_code: 0,
                });
            }
            let is_managed_command = args.iter().any(|argument| {
                matches!(
                    argument.to_string_lossy().as_ref(),
                    "agent" | "workspace" | "present" | "ego"
                )
            });
            if !is_managed_command {
                return Err(CliOutput {
                    stdout: String::new(),
                    stderr: error.to_string(),
                    exit_code: error.exit_code(),
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
            TopCommand::Doctor => CliInvocation {
                command: CardCommand::Doctor,
                format: explicit_format.unwrap_or(OutputFormat::Text),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
            },
            TopCommand::Workspace(workspace) => CliInvocation {
                command: workspace.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Json),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
            },
            TopCommand::Card(card) => CliInvocation {
                command: card.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Text),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
            },
            TopCommand::Agent(agent) => {
                let project_self = matches!(agent.command, AgentCliCommand::Myself);
                let (command, wait_timeout_seconds, target_workspace_id, operation_id) =
                    agent.into_command();
                CliInvocation {
                    command,
                    format: explicit_format.unwrap_or(OutputFormat::Json),
                    wait_timeout_seconds,
                    target_workspace_id,
                    operation_id,
                    project_self,
                }
            }
            TopCommand::Present(present) => CliInvocation {
                command: present.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Json),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
            },
            TopCommand::Ego(ego) => CliInvocation {
                command: ego.into_command(),
                format: explicit_format.unwrap_or(OutputFormat::Json),
                wait_timeout_seconds: None,
                target_workspace_id: None,
                operation_id: None,
                project_self: false,
            },
        }
    }
}

impl WorkspaceArgs {
    fn into_command(self) -> CardCommand {
        match self.command {
            WorkspaceCliCommand::List => CardCommand::ListWorkspaces,
        }
    }
}

impl PresentArgs {
    fn into_command(self) -> CardCommand {
        let (target, create_new) = match self.kind {
            PresentKind::Ego { create_new } => {
                (crate::protocol::PresentationTarget::Ego, create_new)
            }
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

impl EgoArgs {
    fn into_command(self) -> CardCommand {
        let (node_id, command) = match self.command {
            EgoCliCommand::Status { node_id } => (node_id, EgoCommand::Status),
            EgoCliCommand::Running { node_id } => (node_id, EgoCommand::Running),
            EgoCliCommand::Handoff { node_id, message } => {
                (node_id, EgoCommand::Handoff { message })
            }
            EgoCliCommand::Complete { node_id } => (node_id, EgoCommand::Complete),
            EgoCliCommand::Fail { node_id, message } => (node_id, EgoCommand::Fail { message }),
        };
        CardCommand::Ego { node_id, command }
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
    fn into_command(self) -> (CardCommand, Option<u64>, Option<String>, Option<String>) {
        match self.command {
            AgentCliCommand::List { workspace } => (CardCommand::ListAgents, None, workspace, None),
            AgentCliCommand::Myself => (CardCommand::ListAgents, None, None, None),
            AgentCliCommand::Task(task) => task.into_command(),
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
