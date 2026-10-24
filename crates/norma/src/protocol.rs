use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod brain;
mod ego;
mod errors;
mod workspace;

pub use brain::{BrainDecision, BrainOp, ReportPriority};
pub use ego::{
    EgoCaptureMode, EgoCardStatus, EgoCommand, EgoCommandFailure, EgoErrorCode, EgoOwnership,
    EgoPhase, EgoPresentationDisposition,
};
pub use errors::{
    AgentCommandFailure, AgentErrorCode, CardCommandError, PresentationCommandFailure,
    PresentationErrorCode,
};
pub use workspace::{OrchestrationDoctorReport, WorkspaceSummary};

pub type NodeId = u64;
pub const CURRENT_PROTOCOL_VERSION: u32 = 3;

fn legacy_protocol_version() -> u32 {
    1
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WireRequest {
    #[serde(default = "legacy_protocol_version")]
    pub protocol_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_version: Option<String>,
    pub request_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorization: Option<ProviderAuthorization>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_workspace_route_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_node_id: Option<NodeId>,
    pub command: CardCommand,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WireResponse {
    #[serde(default)]
    pub server_version: String,
    #[serde(default = "legacy_protocol_version")]
    pub protocol_version: u32,
    pub request_id: u64,
    pub result: Result<CardCommandResult, CardCommandError>,
}

impl WireResponse {
    pub fn new(request_id: u64, result: Result<CardCommandResult, CardCommandError>) -> Self {
        Self {
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: CURRENT_PROTOCOL_VERSION,
            request_id,
            result,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum CardCommand {
    Doctor,
    ListWorkspaces,
    ListAgents,
    AgentTaskStatus {
        task_ref: String,
    },
    AgentTaskResult {
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
    /// 后台大脑的命令，用 App 下发的 token 鉴权。
    Brain {
        token: String,
        op: BrainOp,
    },
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
    InfCode,
    Kimi,
    ZCode,
    Pi,
    Hermes,
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
    /// 大脑命令的结果，是一段 JSON 文本。
    Brain {
        output: String,
    },
    Doctor {
        report: OrchestrationDoctorReport,
    },
    WorkspacesListed {
        workspaces: Vec<WorkspaceSummary>,
    },
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
    AgentTaskResult {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_workspace_id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<AgentTaskResult>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskResultAvailability {
    Available,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentTaskResult {
    pub availability: AgentTaskResultAvailability,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub truncated: bool,
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
