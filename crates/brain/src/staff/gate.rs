//! 宿主按 token 认出调用者，再决定它能不能执行某条命令。纯函数，不依赖 UI。
use super::policy::{authorize_staff, Verdict};
use norma::protocol::BrainOp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrainRole {
    /// 主大脑和审批助手共用的身份。
    Main,
    Staff,
}

/// 一次大脑启动里下发的两个 token。
#[derive(Clone, Debug)]
pub struct Tokens {
    main: String,
    staff: String,
}

impl Tokens {
    pub fn new(main: &str, staff: &str) -> Self {
        Self {
            main: main.into(),
            staff: staff.into(),
        }
    }

    pub fn main(&self) -> &str {
        &self.main
    }

    /// 空 token 和陌生 token 都没有角色。
    pub fn role(&self, token: &str) -> Option<BrainRole> {
        if token.is_empty() {
            None
        } else if token == self.main {
            Some(BrainRole::Main)
        } else if token == self.staff {
            Some(BrainRole::Staff)
        } else {
            None
        }
    }
}

/// 宿主此刻知道的状态。
#[derive(Clone, Copy, Debug)]
pub struct GateContext<'a> {
    /// 幕僚当前这一轮正在处理的任务；没有进行中的轮次时为 `None`。
    pub active_task: Option<&'a str>,
    /// 被操作的任务是否已完成，只影响归档。
    pub task_done: bool,
}

/// 主大脑不受限制；幕僚必须在白名单内，且 task 命令只能操作当前这一轮的任务。
pub fn authorize(role: BrainRole, op: &BrainOp, context: &GateContext) -> Verdict {
    match role {
        BrainRole::Main => Verdict::Allow,
        BrainRole::Staff => {
            let listed = authorize_staff(op, context.task_done);
            if listed != Verdict::Allow {
                return listed;
            }
            match op {
                BrainOp::Task { op } if op.task_id() != context.active_task => Verdict::Denied {
                    code: "staff.denied.other_task",
                },
                _ => Verdict::Allow,
            }
        }
    }
}

const TOKEN_VAR: &str = "NORMA_BRAIN_TOKEN";

/// 幕僚会话的环境：沿用基础环境，只把大脑 token 换成幕僚 token，不留第二份。
pub fn staff_env(base: &[(String, String)], staff_token: &str) -> Vec<(String, String)> {
    let mut env: Vec<_> = base
        .iter()
        .filter(|(key, _)| key != TOKEN_VAR)
        .cloned()
        .collect();
    env.push((TOKEN_VAR.into(), staff_token.into()));
    env
}

/// 幕僚主动 report 的频率上限：每个任务在窗口内最多 `max` 条。
pub struct ReportLimiter {
    window: std::time::Duration,
    max: usize,
    sent: std::collections::HashMap<String, std::collections::VecDeque<std::time::Instant>>,
}

impl ReportLimiter {
    pub fn new(window: std::time::Duration, max: usize) -> Self {
        Self {
            window,
            max,
            sent: Default::default(),
        }
    }

    /// 放行时记一次；超出上限返回 false，不记。
    pub fn allow(&mut self, task: &str, now: std::time::Instant) -> bool {
        let sent = self.sent.entry(task.to_owned()).or_default();
        while sent
            .front()
            .is_some_and(|at| now.duration_since(*at) >= self.window)
        {
            sent.pop_front();
        }
        if sent.len() >= self.max {
            return false;
        }
        sent.push_back(now);
        true
    }
}
