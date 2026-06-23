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
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum AgentProfile {
    Claude,
    Codex,
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
    Created { node_id: NodeId },
    Deleted { node_id: NodeId },
    Sent { node_id: NodeId },
    Focused { node_id: NodeId },
    Listed { cards: Vec<CardSummary> },
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
