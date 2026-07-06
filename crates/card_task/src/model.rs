use norma::protocol::CardCommand;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CardTaskId(u64);

impl CardTaskId {
    pub fn new(raw: u64) -> Self {
        Self(raw)
    }
}

impl fmt::Display for CardTaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "task_{}", self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardTaskRequest {
    pub intent: String,
    pub tool_call_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CardTask {
    pub task_id: CardTaskId,
    pub request: CardTaskRequest,
    pub summary: String,
    pub steps: Vec<CardTaskStep>,
    pub state: CardTaskState,
    pub cancel_requested: bool,
}

impl CardTask {
    pub fn new(task_id: CardTaskId, request: CardTaskRequest) -> Self {
        let summary = request.intent.clone();
        Self {
            task_id,
            request,
            summary,
            steps: Vec::new(),
            state: CardTaskState::Queued,
            cancel_requested: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardTaskStep {
    Command(CardCommand),
    SendTextToPrevious {
        previous_step: usize,
        text: String,
        submit: bool,
    },
    FocusPrevious {
        previous_step: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardTaskState {
    Queued,
    Planning,
    Executing { step_index: usize },
    Succeeded,
    Failed { message: String },
    Cancelled { message: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CardTaskEvent {
    Queued {
        task_id: CardTaskId,
        summary: String,
    },
    Planning {
        task_id: CardTaskId,
    },
    Planned {
        task_id: CardTaskId,
        step_count: usize,
    },
    Executing {
        task_id: CardTaskId,
        step_index: usize,
    },
    StepSucceeded {
        task_id: CardTaskId,
        step_index: usize,
    },
    StepFailed {
        task_id: CardTaskId,
        step_index: usize,
        message: String,
    },
    Succeeded {
        task_id: CardTaskId,
        summary: String,
    },
    Failed {
        task_id: CardTaskId,
        message: String,
    },
    Cancelled {
        task_id: CardTaskId,
        message: String,
    },
}
