use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type NodeId = u64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WireRequest {
    pub request_id: u64,
    pub command: CardCommand,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WireResponse {
    pub request_id: u64,
    pub result: Result<CardCommandResult, CardCommandError>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CardCommand {
    ListAgents,
    CreateAgent {
        name: Option<String>,
        profile: AgentProfile,
        activate: bool,
    },
    ActivateAgent {
        name: String,
    },
    DispatchAgentTasks {
        assignments: Vec<AgentTaskAssignment>,
        force_untracked: bool,
    },
    Create {
        kind: CardKind,
        placement: Placement,
    },
    Delete {
        node_id: NodeId,
    },
    SendText {
        node_id: NodeId,
        text: String,
        submit: bool,
    },
    Focus {
        node_id: NodeId,
    },
    List,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTaskAssignment {
    pub name: String,
    pub task: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CardKind {
    Terminal {
        shell: Option<String>,
    },
    Agent {
        profile: AgentProfile,
    },
    Preview {
        path: PathBuf,
        explicit_type: Option<PreviewType>,
    },
    Browser {
        url: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AgentProfile {
    Claude,
    Codex,
    Cursor,
    OpenCode,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PreviewType {
    Markdown,
    Text,
    Image,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum Placement {
    VisibleArea,
    Bounds(CardBounds),
}

impl Eq for Placement {}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct CardBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Eq for CardBounds {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CardCommandResult {
    Created {
        node_id: NodeId,
    },
    Deleted {
        node_id: NodeId,
    },
    Sent {
        node_id: NodeId,
    },
    Focused {
        node_id: NodeId,
    },
    Listed {
        cards: Vec<CardSummary>,
    },
    AgentsListed {
        agents: Vec<AgentSummary>,
    },
    AgentCreated {
        definition_id: i64,
        name: String,
        node_id: Option<NodeId>,
    },
    AgentActivated {
        definition_id: i64,
        name: String,
        node_id: NodeId,
        reused: bool,
    },
    AgentTasksDispatched {
        receipts: Vec<AgentTaskReceipt>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentSummary {
    pub definition_id: i64,
    pub name: String,
    pub speech_alias: Option<String>,
    pub profile: AgentProfile,
    pub enabled: bool,
    pub active_node_id: Option<NodeId>,
    pub busy: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTaskReceipt {
    pub name: String,
    pub node_id: NodeId,
    pub task: String,
    pub status: AgentTaskReceiptStatus,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AgentTaskReceiptStatus {
    Submitted,
    Queued,
    Untracked,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CardSummary {
    pub node_id: NodeId,
    pub title: String,
    #[serde(default)]
    pub alias: Option<String>,
    pub kind: CardSummaryKind,
    pub selected: bool,
    pub focused: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CardSummaryKind {
    Rect,
    Terminal,
    Agent { profile: AgentProfile },
    Preview,
    Browser,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CardCommandError {
    AppNotReady,
    NotFound { node_id: NodeId },
    Unsupported { message: String },
    Runtime { message: String },
}

impl std::fmt::Display for CardCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AppNotReady => write!(f, "app is not ready for card commands"),
            Self::NotFound { node_id } => write!(f, "card {node_id} was not found"),
            Self::Unsupported { message } => write!(f, "{message}"),
            Self::Runtime { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for CardCommandError {}
