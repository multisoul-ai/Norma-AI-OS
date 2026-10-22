//! 工作空间想法：纯类型与状态流转，零 GPUI、零存储。

/// 想法正文上限（字符数，不是字节数）。
pub const MAX_IDEA_CHARS: usize = 500;

/// 想法的来源：对话文字、语音、手动添加。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeaSource {
    Chat,
    Voice,
    Manual,
}

impl IdeaSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::Voice => "voice",
            Self::Manual => "manual",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "chat" => Self::Chat,
            "voice" => Self::Voice,
            "manual" => Self::Manual,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeaError {
    Empty,
    TooLong,
    InvalidTransition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeaState {
    Todo,
    Dispatching,
    Assigned,
    Done,
    Dismissed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeaEvent {
    BeginDispatch,
    DispatchSucceeded,
    DispatchFailed,
    Complete,
    Retract,
    Dismiss,
    Restore,
}

impl IdeaState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Dispatching => "dispatching",
            Self::Assigned => "assigned",
            Self::Done => "done",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "todo" => Self::Todo,
            "dispatching" => Self::Dispatching,
            "assigned" => Self::Assigned,
            "done" => Self::Done,
            "dismissed" => Self::Dismissed,
            _ => return None,
        })
    }
}

/// 去首尾空白；空白或超长直接拒绝，不静默截断用户的想法。
pub fn normalize_body(raw: &str) -> Result<String, IdeaError> {
    let body = raw.trim();
    if body.is_empty() {
        return Err(IdeaError::Empty);
    }
    if body.chars().count() > MAX_IDEA_CHARS {
        return Err(IdeaError::TooLong);
    }
    Ok(body.to_owned())
}

pub fn transition(state: IdeaState, event: IdeaEvent) -> Result<IdeaState, IdeaError> {
    use IdeaEvent as E;
    use IdeaState as S;
    match (state, event) {
        (S::Todo, E::BeginDispatch) => Ok(S::Dispatching),
        (S::Dispatching, E::DispatchSucceeded) => Ok(S::Assigned),
        (S::Dispatching, E::DispatchFailed) => Ok(S::Todo),
        (S::Assigned, E::Complete) => Ok(S::Done),
        (S::Assigned, E::Retract) => Ok(S::Todo),
        (S::Todo, E::Dismiss) => Ok(S::Dismissed),
        (S::Dismissed, E::Restore) => Ok(S::Todo),
        _ => Err(IdeaError::InvalidTransition),
    }
}

/// 任务阶段：协调器与派发库的状态文本归并后，想法只关心这几类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskPhase {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    /// 无法追踪完成（没有任务回执可读）。
    Untracked,
}

/// 任务阶段对想法存储状态的联动。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IdeaEffect {
    Keep,
    Complete,
    Retract,
}

/// 未知状态保守地当作运行中，绝不当作成功或取消。
pub fn classify_task_state(state: &str) -> TaskPhase {
    match state {
        "queued" => TaskPhase::Queued,
        "succeeded" | "done" => TaskPhase::Succeeded,
        "failed" | "interrupted" => TaskPhase::Failed,
        "cancelled" => TaskPhase::Cancelled,
        "untracked" => TaskPhase::Untracked,
        _ => TaskPhase::Running,
    }
}

/// 只有成功会让想法完成，只有取消会让它回到待办；失败与无法追踪保持已指派，交给界面提示。
pub fn idea_effect(phase: TaskPhase) -> IdeaEffect {
    match phase {
        TaskPhase::Succeeded => IdeaEffect::Complete,
        TaskPhase::Cancelled => IdeaEffect::Retract,
        TaskPhase::Queued | TaskPhase::Running | TaskPhase::Failed | TaskPhase::Untracked => {
            IdeaEffect::Keep
        }
    }
}
