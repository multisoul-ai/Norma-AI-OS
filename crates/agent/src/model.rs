use norma::protocol::{CardCommand, CardCommandResult};
use std::fmt;

pub type TaskId = u64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentMessage {
    pub role: MessageRole,
    pub content: String,
    pub tool_call_id: Option<String>,
    pub tool_calls: Vec<ProviderToolCall>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct ProviderToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderToolSpec {
    pub name: String,
    pub description: String,
    pub parameters_json_schema: serde_json::Value,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderRequest {
    pub messages: Vec<AgentMessage>,
    pub tools: Vec<ProviderToolSpec>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderResponse {
    pub text: String,
    pub tool_calls: Vec<ProviderToolCall>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentCommand {
    UserTurn(String),
    DirectCardTask(String),
    CancelLatest,
    CancelTask { task_id: TaskId },
    Status { task_id: Option<TaskId> },
    CardActivity { node_id: u64, summary: String },
    ProjectClosed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEvent {
    DialogueTextDelta {
        text: String,
    },
    /// Signals that all live deltas for the current ordinary dialogue reply were emitted.
    DialogueStreamFinished,
    TaskQueued {
        task_id: TaskId,
        summary: String,
    },
    TaskRunning {
        task_id: TaskId,
    },
    TaskStatus {
        task_id: TaskId,
        state: TaskState,
        completed_tool_calls: usize,
    },
    ToolStarted {
        task_id: TaskId,
        tool_call_id: String,
    },
    ToolFinished {
        task_id: TaskId,
        tool_call_id: String,
        result: Result<CardCommandResult, String>,
    },
    TaskCancelRequested {
        task_id: TaskId,
    },
    TaskSucceeded {
        task_id: TaskId,
        summary: String,
    },
    TaskFailed {
        task_id: TaskId,
        reason: String,
    },
    TaskCancelled {
        task_id: TaskId,
        partial_effects: bool,
        completed_tool_calls: usize,
    },
    TaskInterrupted {
        task_id: TaskId,
    },
    CardActivityNotification {
        node_id: u64,
        summary: String,
    },
    ProviderError {
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskState {
    Queued,
    Running,
    Succeeded,
    Failed { reason: String },
    CancelledWithPartialEffects,
    Cancelled,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardToolRequest {
    pub project_id: String,
    pub task_id: TaskId,
    pub tool_call_id: String,
    pub command: CardCommand,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardToolResult {
    pub result: CardCommandResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskRecord {
    pub project_id: String,
    pub task_id: TaskId,
    pub intent: String,
    pub state: TaskState,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub completed_tool_calls: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolCallRecord {
    pub project_id: String,
    pub task_id: TaskId,
    pub tool_call_id: String,
    pub command: CardCommand,
    pub result: Result<CardCommandResult, String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolErrorRecord {
    pub project_id: String,
    pub task_id: TaskId,
    pub tool_call_id: String,
    pub tool_name: String,
    pub arguments_json: String,
    pub error: String,
}

impl fmt::Display for TaskState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Queued => write!(f, "queued"),
            Self::Running => write!(f, "running"),
            Self::Succeeded => write!(f, "succeeded"),
            Self::Failed { reason } => write!(f, "failed: {reason}"),
            Self::CancelledWithPartialEffects => write!(f, "cancelled_with_partial_effects"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Interrupted => write!(f, "interrupted"),
        }
    }
}

impl TaskState {
    pub fn storage_name(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed { .. } => "failed",
            Self::CancelledWithPartialEffects => "cancelled_with_partial_effects",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}
