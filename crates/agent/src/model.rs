pub type TaskId = u64;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AgentInputSource {
    Voice,
    /// Only the local desktop Doubao session may use this source.
    DoubaoVoice,
    /// GPT-Live delegation; authorization uses only the observed user transcript.
    GptLiveVoice,
    /// Workspace discussion cannot dispatch mutable desktop tools.
    MobileDiscussion,
    /// The phone has expressed an explicit execution intent.
    MobileExecution,
    #[default]
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentCommand {
    UpdateAgentRoster(Vec<crate::AgentRosterEntry>),
    UpdateCurrentCanvasContext(crate::CurrentCanvasContext),
    UpdateDesktopContext(Option<crate::DesktopContext>),
    SwitchModel(crate::ProviderConfig),
    UserTurn {
        input: String,
        input_source: AgentInputSource,
    },
    CancelLatest,
    CancelTask {
        task_id: TaskId,
    },
    Status {
        task_id: Option<TaskId>,
    },
    CardActivity {
        node_id: u64,
        summary: String,
    },
    ProjectClosed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgentEvent {
    DialogueTextDelta {
        text: String,
    },
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
        result: Result<norma::protocol::CardCommandResult, String>,
    },
    TaskCancelRequested {
        task_id: TaskId,
    },
    TaskSucceeded {
        task_id: TaskId,
        summary: String,
    },
    TaskPartiallyDispatched {
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
    CancelRequested,
    Succeeded,
    Failed { reason: String },
    Cancelled,
    Interrupted,
}

impl std::fmt::Display for TaskState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Queued => formatter.write_str("queued"),
            Self::Running => formatter.write_str("running"),
            Self::CancelRequested => formatter.write_str("cancel_requested"),
            Self::Succeeded => formatter.write_str("succeeded"),
            Self::Failed { .. } => formatter.write_str("failed"),
            Self::Cancelled => formatter.write_str("cancelled"),
            Self::Interrupted => formatter.write_str("interrupted"),
        }
    }
}
