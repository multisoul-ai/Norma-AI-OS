//! 大脑的输入事件与输出状态。

use crate::acp::trace::AcpTrace;
use crate::risk::ApprovalSubject;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrainEvent {
    AgentProgress {
        agent: String,
        text: String,
    },
    AgentFinished {
        agent: String,
        outcome: Outcome,
        excerpt: String,
        /// 摘要是 Agent 给出的结果；`false` 表示拿不到结果、退回成了用户发的话。
        from_result: bool,
    },
    ApprovalRequested(ApprovalRequest),
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
pub enum BrainStatus {
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
