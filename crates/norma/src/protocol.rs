use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod ego;

pub use ego::{
    EgoCaptureMode, EgoCardStatus, EgoCommand, EgoCommandFailure, EgoErrorCode, EgoOwnership,
    EgoPhase, EgoPresentationDisposition,
};

pub type NodeId = u64;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WireRequest {
    pub request_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization: Option<ProviderAuthorization>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<NodeId>,
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
    AgentTaskStatus {
        task_ref: String,
    },
    AgentTaskCancel {
        task_ref: String,
    },
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
    Present {
        target: PresentationTarget,
        create_new: bool,
    },
    Ego {
        node_id: NodeId,
        command: EgoCommand,
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
pub enum PresentationTarget {
    Ego,
    Browser {
        url: String,
    },
    Preview {
        path: PathBuf,
        explicit_type: Option<PreviewType>,
    },
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
    Grok,
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
    AgentTaskStatus {
        task: AgentTaskStatus,
    },
    AgentTaskCancelled {
        task_id: String,
        status: AgentTaskReceiptStatus,
    },
    Presented {
        node_id: NodeId,
        kind: PresentationKind,
        disposition: PresentationDisposition,
        layout: PresentationLayout,
    },
    EgoPresented {
        status: EgoCardStatus,
        disposition: EgoPresentationDisposition,
    },
    EgoStatus {
        status: EgoCardStatus,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationKind {
    Browser,
    Preview,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationDisposition {
    Created,
    Reused,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationLayout {
    Master,
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
    pub tracking: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTaskReceipt {
    pub task_id: String,
    pub target_definition_id: i64,
    #[serde(rename = "target_name")]
    pub name: String,
    #[serde(rename = "target_node_id")]
    pub node_id: NodeId,
    #[serde(skip)]
    pub task: String,
    pub status: AgentTaskReceiptStatus,
    pub tracking: String,
    pub parent_task_id: Option<String>,
    pub root_task_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskReceiptStatus {
    Accepted,
    Submitted,
    Queued,
    Untracked,
    CancelRequested,
    Done,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTaskStatus {
    pub task_id: String,
    pub target_definition_id: i64,
    pub target_name: String,
    pub target_node_id: NodeId,
    pub status: AgentTaskReceiptStatus,
    pub tracking: String,
    pub parent_task_id: Option<String>,
    pub root_task_id: String,
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
    Agent(AgentCommandFailure),
    Presentation(PresentationCommandFailure),
    Ego(EgoCommandFailure),
}

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderAuthorization {
    pub provider: AgentProfile,
    pub token: String,
}

impl std::fmt::Debug for ProviderAuthorization {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderAuthorization")
            .field("provider", &self.provider)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorCode {
    AppUnavailable,
    ProjectUnavailable,
    Unauthorized,
    ProviderNotInstalled,
    SourceAgentUnavailable,
    UnknownAgent,
    AgentDisabled,
    EmptyTask,
    SelfDispatchForbidden,
    DelegationCycle,
    DelegationBudgetExceeded,
    UntrackedRequiresForce,
    TaskNotFound,
    RuntimeExpired,
    TaskUntracked,
    WaitTimeout,
    InstallConflict,
    ComponentInstallFailed,
    UnsupportedProvider,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentCommandFailure {
    pub code: AgentErrorCode,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_status: Option<AgentTaskReceiptStatus>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PresentationErrorCode {
    InvalidUrl,
    ForbiddenUrlScheme,
    PreviewNotFound,
    PreviewNotFile,
    PreviewUnreadable,
    UnsupportedPreviewType,
    PresentationFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PresentationCommandFailure {
    pub code: PresentationErrorCode,
    pub message: String,
    pub retryable: bool,
}

impl CardCommandError {
    pub fn agent(code: AgentErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self::Agent(AgentCommandFailure {
            code,
            message: message.into(),
            retryable,
            current_status: None,
        })
    }

    pub fn agent_with_status(
        code: AgentErrorCode,
        message: impl Into<String>,
        retryable: bool,
        current_status: AgentTaskReceiptStatus,
    ) -> Self {
        Self::Agent(AgentCommandFailure {
            code,
            message: message.into(),
            retryable,
            current_status: Some(current_status),
        })
    }

    pub fn presentation(
        code: PresentationErrorCode,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self::Presentation(PresentationCommandFailure {
            code,
            message: message.into(),
            retryable,
        })
    }

    pub fn ego(code: EgoErrorCode, message: impl Into<String>, retryable: bool) -> Self {
        Self::Ego(EgoCommandFailure {
            code,
            message: message.into(),
            retryable,
        })
    }
}

impl std::fmt::Display for CardCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AppNotReady => write!(f, "app is not ready for card commands"),
            Self::NotFound { node_id } => write!(f, "card {node_id} was not found"),
            Self::Unsupported { message } => write!(f, "{message}"),
            Self::Runtime { message } => write!(f, "{message}"),
            Self::Agent(error) => write!(f, "{}", error.message),
            Self::Presentation(error) => write!(f, "{}", error.message),
            Self::Ego(error) => write!(f, "{}", error.message),
        }
    }
}

impl std::error::Error for CardCommandError {}
