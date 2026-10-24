//! 后台大脑通过 `norma brain …` 使用的命令。
//!
//! 这些命令不属于 Agent 编排：调用方不是 Agent 卡片，而是 App 为大脑生成的进程，
//! 用 App 启动时下发的一次性 token 鉴权。

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ReportPriority {
    #[default]
    Normal,
    /// 排到播报队列最前，用于回答用户和高风险确认。
    Now,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BrainDecision {
    Allow,
    Deny,
    /// 交给用户处理，不替用户决定。
    Escalate,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum BrainOp {
    Report {
        text: String,
        priority: ReportPriority,
    },
    Approvals,
    Decide {
        approval_id: String,
        decision: BrainDecision,
        reason: String,
    },
    Agents,
    Assign {
        agent: String,
        task: String,
    },
    /// 读某个 Agent 卡片最近的对话。`last` 缺省由 App 决定。
    History {
        agent: String,
        last: Option<u32>,
        grep: Option<String>,
    },
}
