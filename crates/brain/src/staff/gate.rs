//! 宿主按 token 认出调用者，再决定它能不能执行某条命令。纯函数，不依赖 UI。
use super::policy::{authorize_staff, Verdict};
use norma::protocol::BrainOp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrainRole {
    /// 主大脑和审批助手共用的身份。
    Main,
    Staff,
}

/// 幕僚会话的任务专属 token：每个会话（一个后端进程）只服务一个任务，
/// 宿主凭 token 就知道命令来自哪个任务，不必猜「当前在跑的是哪个」。
#[derive(Clone, Debug, Default)]
pub struct StaffTokens {
    bound: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
}

impl StaffTokens {
    /// 为任务铸一把新 token 并登记；会话结束时 `revoke`。
    pub fn mint(&self, task_id: &str) -> String {
        let token = uuid::Uuid::new_v4().simple().to_string();
        self.bind(&token, task_id);
        token
    }

    pub fn bind(&self, token: &str, task_id: &str) {
        self.map().insert(token.to_owned(), task_id.to_owned());
    }

    pub fn revoke(&self, token: &str) {
        self.map().remove(token);
    }

    pub fn task_of(&self, token: &str) -> Option<String> {
        self.map().get(token).cloned()
    }

    fn map(&self) -> std::sync::MutexGuard<'_, std::collections::HashMap<String, String>> {
        self.bound
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// 一次大脑启动里下发的 token：一个主大脑 token，加按任务铸造的幕僚 token。
#[derive(Clone, Debug)]
pub struct Tokens {
    main: String,
    staff: StaffTokens,
}

impl Tokens {
    pub fn new(main: &str) -> Self {
        Self::with_staff(main, StaffTokens::default())
    }

    pub fn with_staff(main: &str, staff: StaffTokens) -> Self {
        Self {
            main: main.into(),
            staff,
        }
    }

    pub fn main(&self) -> &str {
        &self.main
    }

    /// 幕僚 token 对应的任务；主大脑、陌生和空 token 都没有。
    pub fn staff_task(&self, token: &str) -> Option<String> {
        if token.is_empty() {
            None
        } else {
            self.staff.task_of(token)
        }
    }

    /// 空 token 和陌生 token 都没有角色。
    pub fn role(&self, token: &str) -> Option<BrainRole> {
        if token.is_empty() {
            None
        } else if token == self.main {
            Some(BrainRole::Main)
        } else if self.staff.task_of(token).is_some() {
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

/// 幕僚会话的环境：沿用基础环境，把大脑 token 换成幕僚 token，不留第二份。
/// 没有幕僚 token 时宁可不给任何 token，也不退回主大脑的 token。
pub fn staff_env(base: &[(String, String)], staff_token: Option<&str>) -> Vec<(String, String)> {
    let mut env: Vec<_> = base
        .iter()
        .filter(|(key, _)| key != TOKEN_VAR)
        .cloned()
        .collect();
    env.extend(staff_token.map(|token| (TOKEN_VAR.into(), token.into())));
    env
}

/// history 读的是 Agent 卡片的整段对话：只放行本任务派出过的那张卡。
pub fn history_in_task(task: &norma::protocol::Task, open_card: Option<u64>) -> bool {
    open_card.is_some_and(|card| {
        task.workers
            .iter()
            .any(|worker| worker.card_id == Some(card))
    })
}

/// 幕僚主动 report 的限制：每个任务每 10 分钟最多 2 条。
pub const REPORT_WINDOW: std::time::Duration = std::time::Duration::from_secs(600);
pub const REPORT_LIMIT: usize = 2;

/// history 一次最多读几轮。
pub const HISTORY_MAX_LAST: u32 = 20;

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
