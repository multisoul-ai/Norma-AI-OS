//! 幕僚自己派活的策略。纯函数，不读存储也不依赖 UI。

use norma::protocol::{BrainOp, StaffOp};

/// 高风险类别。命中任意一类，幕僚都不能直接派活，只能提议等用户采用。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum RiskCategory {
    /// 推送、合并、部署、发布。
    Release,
    /// 删除或其他不可逆的数据操作。
    Destructive,
    /// 密钥、凭据、提权、项目目录以外。
    Sensitive,
    /// 对外沟通、产生费用。
    External,
}

/// 关键词只降低概率，不是安全边界：改写、拼接、混写都能绕过。
const KEYWORDS: [(RiskCategory, &[&str]); 4] = [
    (
        RiskCategory::Release,
        &[
            "push",
            "merge",
            "deploy",
            "release",
            "publish",
            "terraform apply",
            "kubectl apply",
            "部署",
            "发布",
            "发版",
            "上线",
            "推送",
            "合并",
        ],
    ),
    (
        RiskCategory::Destructive,
        &[
            "rm -rf",
            "rm -fr",
            "rm -r",
            "git clean",
            "branch -d",
            "drop table",
            "drop database",
            "truncate",
            "reset --hard",
            "delete",
            "删除",
            "清空",
            "清库",
        ],
    ),
    (
        RiskCategory::Sensitive,
        &[
            "secret",
            "credential",
            "password",
            "api key",
            "private key",
            "bearer",
            "access token",
            "auth token",
            ".env",
            "| sh",
            "| bash",
            "chmod",
            "chown",
            "/etc/",
            "sudo",
            "~/.ssh",
            "密钥",
            "凭据",
            "密码",
            "提权",
        ],
    ),
    (
        RiskCategory::External,
        &[
            "email",
            "slack",
            "payment",
            "purchase",
            "billing",
            "邮件",
            "发消息",
            "付款",
            "购买",
            "花钱",
        ],
    ),
];

/// 去掉零宽字符、全角转半角、空白折叠并转小写，让简单的混淆写法不能绕过关键词。
pub(super) fn normalize(text: &str) -> String {
    let half_width = text.chars().filter_map(|c| match c {
        '\u{200b}'..='\u{200d}' | '\u{2060}' | '\u{feff}' => None,
        '\u{ff01}'..='\u{ff5e}' => char::from_u32(c as u32 - 0xfee0),
        c => Some(c),
    });
    half_width
        .collect::<String>()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// 返回文本命中的全部风险类别，按类别顺序，不重复。
pub fn risk_categories(text: &str) -> Vec<RiskCategory> {
    let text = normalize(text);
    KEYWORDS
        .iter()
        .filter(|(_, words)| words.iter().any(|word| text.contains(word)))
        .map(|(category, _)| *category)
        .collect()
}

/// 每个任务最多自动派发的次数；用户采用任意一条提议后清零。
pub const AUTO_DISPATCH_LIMIT: u32 = 3;

/// 派活时需要的任务状态。`auto` 是设置里的「后续任务」是否为「自动」。
#[derive(Clone, Copy, Debug)]
pub struct AssignContext<'a> {
    pub auto: bool,
    /// 最近一个已出结果的 Worker 的 Agent；还没有时为 `None`。
    pub last_finished_agent: Option<&'a str>,
    pub auto_dispatches: u32,
}

/// 不能直接派发的原因，按判断顺序只报第一个。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposeReason {
    AutonomyAsk,
    Risk(Vec<RiskCategory>),
    NotFollowUp,
    LimitReached,
}

impl ProposeReason {
    /// 回给模型的结构化原因，不含面向用户的文案。
    pub fn code(&self) -> String {
        match self {
            Self::AutonomyAsk => "autonomy_ask".into(),
            Self::NotFollowUp => "not_follow_up".into(),
            Self::LimitReached => "limit_reached".into(),
            Self::Risk(categories) => {
                let names: Vec<_> = categories
                    .iter()
                    .map(|c| format!("{c:?}").to_lowercase())
                    .collect();
                format!("risk:{}", names.join(","))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AssignDecision {
    Dispatch,
    Propose(ProposeReason),
}

/// 直接派发要同时满足：设置为自动、无风险、派给刚完成任务的同一个 Agent、次数未超限。
pub fn decide_assign(context: &AssignContext, agent: &str, text: &str) -> AssignDecision {
    if !context.auto {
        return AssignDecision::Propose(ProposeReason::AutonomyAsk);
    }
    let risks = risk_categories(text);
    if !risks.is_empty() {
        return AssignDecision::Propose(ProposeReason::Risk(risks));
    }
    if context.last_finished_agent != Some(agent) {
        return AssignDecision::Propose(ProposeReason::NotFollowUp);
    }
    if context.auto_dispatches >= AUTO_DISPATCH_LIMIT {
        return AssignDecision::Propose(ProposeReason::LimitReached);
    }
    AssignDecision::Dispatch
}

/// 续派已有 Worker：用户这一轮发过话就不再问（也不要求它刚完成、不计自动次数），
/// 否则按「后续任务」设置；风险词照旧降级为提议。
/// 事件触发的回合（没有用户发话）仍受自动次数上限约束，避免「Worker 完成 → 续派 → 完成」无限循环。
pub fn decide_continue(
    auto: bool,
    user_turn: bool,
    auto_dispatches: u32,
    text: &str,
) -> AssignDecision {
    if !user_turn && !auto {
        return AssignDecision::Propose(ProposeReason::AutonomyAsk);
    }
    if !user_turn && auto_dispatches >= AUTO_DISPATCH_LIMIT {
        return AssignDecision::Propose(ProposeReason::LimitReached);
    }
    let risks = risk_categories(text);
    if !risks.is_empty() {
        return AssignDecision::Propose(ProposeReason::Risk(risks));
    }
    AssignDecision::Dispatch
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Verdict {
    Allow,
    /// `code` 是结构化的拒绝码，宿主在展示边界翻译成中英文，并给出替代做法。
    Denied {
        code: &'static str,
    },
}

/// 幕僚能执行哪些命令。`task_done` 是该任务当前是否已完成，只影响归档。
///
/// 对 `BrainOp` 穷举匹配：以后新增命令必须先决定幕僚能不能用。
pub fn authorize_staff(op: &BrainOp, task_done: bool) -> Verdict {
    let denied = |code| Verdict::Denied { code };
    match op {
        BrainOp::Agents
        | BrainOp::Approvals
        | BrainOp::Questions
        | BrainOp::History { .. }
        | BrainOp::Propose { .. }
        | BrainOp::Report { .. }
        | BrainOp::ProposeAnswer { .. }
        | BrainOp::Context { .. } => Verdict::Allow,
        BrainOp::Decide { .. } => denied("staff.denied.decide"),
        BrainOp::Answer { .. } => denied("staff.denied.answer"),
        BrainOp::Assign { .. } => denied("staff.denied.legacy_assign"),
        BrainOp::Task { op } => match op {
            StaffOp::Phase { .. } | StaffOp::Propose { .. } | StaffOp::Assign { .. } => {
                Verdict::Allow
            }
            // 回复正文只来自流式文字；`task reply` 会造成同一句话两个来源。
            StaffOp::Reply { .. } => denied("staff.denied.reply"),
            StaffOp::Create { .. } | StaffOp::List => denied("staff.denied.task_scope"),
            StaffOp::Message { .. } => denied("staff.denied.user_message"),
            StaffOp::Adopt { .. } => denied("staff.denied.adopt"),
            // 暂只给用户；以后开放给幕僚时改这里，并在 skill 里加命令。
            StaffOp::RemoveWorker { .. } | StaffOp::RestoreWorker { .. } => {
                denied("staff.denied.remove_worker")
            }
            StaffOp::Archive {
                archived: false, ..
            } => denied("staff.denied.unarchive"),
            StaffOp::Archive { .. } if !task_done => denied("staff.denied.archive_not_done"),
            StaffOp::Archive { .. } => Verdict::Allow,
        },
    }
}
