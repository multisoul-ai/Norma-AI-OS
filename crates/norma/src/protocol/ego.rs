use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum EgoCommand {
    Status,
    Running,
    Handoff { message: String },
    Complete,
    Fail { message: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EgoPhase {
    Creating,
    ReadyForUser,
    PreparingAgent,
    AgentWorking,
    AwaitingUser,
    UserControlling,
    ResumeRequested,
    Completed,
    Failed,
    Disconnected,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EgoOwnership {
    Agent { node_id: super::NodeId },
    AgentDelegatedToUser { node_id: super::NodeId },
    User,
    Inactive,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EgoCaptureMode {
    LiveMirror,
    ExternalWindow,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EgoCardStatus {
    pub node_id: super::NodeId,
    pub task_space_id: Option<String>,
    pub task_space_name: Option<String>,
    pub phase: EgoPhase,
    pub ownership: EgoOwnership,
    pub bound_agent_node_id: Option<super::NodeId>,
    pub capture_mode: EgoCaptureMode,
    pub message: Option<String>,
    pub master: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EgoPresentationDisposition {
    Created,
    Reused,
    Replaced,
    Focused,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EgoErrorCode {
    Busy,
    Unauthorized,
    InvalidState,
    TaskSpaceMissing,
    IntegrationUnavailable,
    InternalError,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EgoCommandFailure {
    pub code: EgoErrorCode,
    pub message: String,
    pub retryable: bool,
}
