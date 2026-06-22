use crate::protocol::{
    AgentProfile, CardCommand, CardCommandResult, CardKind, Placement, PreviewType,
};
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::ffi::OsString;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "norma")]
#[command(about = "Control cards in a running Multisoul Desktop app")]
struct NormaArgs {
    #[command(subcommand)]
    command: TopCommand,
}

#[derive(Debug, Subcommand)]
enum TopCommand {
    Card(CardArgs),
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
    let args = NormaArgs::try_parse_from(args)?;
    Ok(args.into_command())
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
    }
}

impl NormaArgs {
    fn into_command(self) -> CardCommand {
        match self.command {
            TopCommand::Card(card) => card.into_command(),
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

impl CreateArgs {
    fn into_command(self) -> CardCommand {
        let kind = match self.kind {
            CreateKind::Terminal { shell } => CardKind::Terminal { shell },
            CreateKind::Agent { profile } => CardKind::Agent {
                profile: profile.into(),
            },
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
