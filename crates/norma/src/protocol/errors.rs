use super::{AgentTaskReceiptStatus, EgoCommandFailure, EgoErrorCode, NodeId};
use serde::{Deserialize, Serialize};

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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentErrorCode {
    AppUnavailable,
    OrchestrationChecking,
    ProtocolVersionMismatch,
    WorkspaceRequired,
    WorkspaceNotFound,
    WorkspaceNotOpen,
    WorkspaceRouteExpired,
    AmbiguousWorkspace,
    WorkspaceIdentityConflict,
    CrossWorkspaceForbidden,
    ProjectUnavailable,
    Unauthorized,
    ProviderNotInstalled,
    ClientUpgradeRequired,
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
    TaskNotCancellable,
    WaitTimeout,
    IdempotencyConflict,
    OutcomeUnknown,
    ServerOverloaded,
    AppShuttingDown,
    LegacyTaskRouteUnavailable,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub outcome_unknown: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
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
            operation_id: None,
            outcome_unknown: false,
        })
    }

    pub fn outcome_unknown(operation_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Agent(AgentCommandFailure {
            code: AgentErrorCode::OutcomeUnknown,
            message: message.into(),
            retryable: true,
            current_status: None,
            operation_id: Some(operation_id.into()),
            outcome_unknown: true,
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
            operation_id: None,
            outcome_unknown: false,
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
