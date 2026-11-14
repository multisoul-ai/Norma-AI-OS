//! 大脑的输入事件与输出状态。

use crate::acp::trace::AcpTrace;
use crate::risk::ApprovalSubject;
use norma::protocol::BrainScope;

/// Every service input carries the exact live workspace route.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrainEvent {
    pub scope: BrainScope,
    pub kind: BrainEventKind,
}

impl BrainEventKind {
    pub fn in_scope(self, scope: BrainScope) -> BrainEvent {
        BrainEvent { scope, kind: self }
    }
}

/// The scope survives worker queues, retries and all resulting status records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrainStatus {
    pub scope: BrainScope,
    pub kind: BrainStatusKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Succeeded,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalRequest {
    /// 审批的稳定标识，`norma brain decide` 用它定位。
    pub id: String,
    pub agent: String,
    pub subject: ApprovalSubject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryPhase {
    Accepted,
    NeedsAttention,
    Redeliver,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrainEventKind {
    /// 用户消息已经在 TaskStore 持久化，排队处理该任务的最新消息。
    TaskRequest {
        task_id: String,
    },
    WorkerAttention {
        node_id: u64,
        dispatch_id: String,
        waiting: bool,
    },
    WorkerDelivery {
        node_id: u64,
        dispatch_id: String,
        phase: DeliveryPhase,
    },
    AgentProgress {
        agent: String,
        text: String,
    },
    AgentFinished {
        node_id: u64,
        dispatch_id: Option<String>,
        agent: String,
        outcome: Outcome,
        excerpt: String,
        /// 摘要是 Agent 给出的结果；`false` 表示拿不到结果、退回成了用户发的话。
        from_result: bool,
    },
    ApprovalRequested(ApprovalRequest),
    /// 原问题；delegated 只能来自宿主明确的单题用户操作，不能从模型文本推断。
    QuestionRequested {
        agent: String,
        question: norma::protocol::PendingQuestion,
        delegated: bool,
    },
    /// 审批已被用户处理或已失效，大脑不必再判断。
    ApprovalClosed {
        id: String,
    },
    /// 用户离开一阵后回到窗口：让大脑把这段时间的关键进展汇总一次。
    UserReturned {
        away: std::time::Duration,
    },
    /// 前台对话转来的任务或问题。
    UserRequest {
        text: String,
    },
    /// 岛内全局幕僚对话的用户消息：回复必须以 `--kind reply` 回到岛里，而不是项目 Dock。
    StaffRequest {
        text: String,
    },
    /// 岛上的“汇报”按钮：宿主已捕获有界的跨工作空间概览（数据，不是指令），要求大脑总结后以 reply 回传。
    ReportRequested {
        /// 岛上这次汇报请求的编号；回复要用 `--reply-to` 带回，才会被朗读。
        request_id: u64,
        overview: String,
    },
    ContextRequest {
        text: String,
        reference: norma::protocol::ContextRef,
    },
    /// 用户在概览里忽略了大脑的提议；只记录，不单独叫醒大脑。
    ProposalDeclined {
        agent: String,
        title: String,
    },
    /// 用户发给某个 Agent 的任务，记作目标；只记录，不单独叫醒大脑。
    Goal {
        agent: String,
        text: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrainStatusKind {
    /// Live task feedback; never persisted as conversation history.
    TaskActivity {
        task_id: String,
        turn: u64,
        update: TaskUpdate,
    },
    TaskChanged(norma::protocol::Task),
    TaskError {
        reason: String,
    },
    Ready,
    BackendDown {
        reason: String,
    },
    /// 审批会话处理完一条审批；调用方检查它是否仍在等待，若是则转人工。
    ApprovalTurnEnded {
        id: String,
    },
    /// ACP 过程里的一条记录，供活动窗口显示。
    Trace(AcpTrace),
}

/// 是什么让大脑开了这一轮。持久化用 `as_str` 存稳定的英文键。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TriggerKind {
    AgentFinished,
    QuestionRequest,
    ApprovalRequest,
    UserRequest,
    UserReturned,
    ReportRequest,
    /// 定时摘要等没有外部事件的唤醒。
    Scheduled,
    Unknown,
}

impl TriggerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentFinished => "agent_finished",
            Self::QuestionRequest => "question_request",
            Self::ApprovalRequest => "approval_request",
            Self::UserRequest => "user_request",
            Self::UserReturned => "user_returned",
            Self::ReportRequest => "report_request",
            Self::Scheduled => "scheduled",
            Self::Unknown => "unknown",
        }
    }

    /// 不认识的键（新版本写入、旧版本读取）退回 `Unknown`。
    pub fn parse(key: &str) -> Self {
        [
            Self::AgentFinished,
            Self::QuestionRequest,
            Self::ApprovalRequest,
            Self::UserRequest,
            Self::UserReturned,
            Self::ReportRequest,
            Self::Scheduled,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == key)
        .unwrap_or(Self::Unknown)
    }
}

/// 触发源：种类，加上相关的 Agent 名（多个用顿号连接；与 Agent 无关时为空）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Trigger {
    pub kind: TriggerKind,
    pub who: String,
}

impl Trigger {
    pub fn new(kind: TriggerKind, who: impl Into<String>) -> Self {
        Self {
            kind,
            who: who.into(),
        }
    }

    pub fn unknown() -> Self {
        Self::new(TriggerKind::Unknown, "")
    }
}

/// Ephemeral progress of one task turn, independent of merged audit traces.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TaskUpdate {
    Queued,
    Started { message_count: usize },
    Update(crate::acp::trace::TraceItem),
    Finished { failed: bool },
}
