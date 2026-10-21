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
