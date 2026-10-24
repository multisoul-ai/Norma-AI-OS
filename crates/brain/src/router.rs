//! 事件分流：去抖、静默池、定时摘要、审批分发。纯逻辑，时间由调用方传入。

use crate::event::{ApprovalRequest, BrainEvent, Outcome};
use crate::lane::prompt;
use crate::triage::{triage, Verdict};
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

const MAX_DIGEST_LINES: usize = 40;
/// 回来摘要最多回顾多少条完成记录。
const MAX_ACTIVITY: usize = 30;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lane {
    Main,
    Approval,
}

/// 交给某条会话的一条提示。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Action {
    pub lane: Lane,
    pub text: String,
    /// 审批提示带上审批标识，便于在发送前检查是否已关闭。
    pub approval_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RouterConfig {
    /// 需要告知用户的事件的去抖窗口。
    pub debounce: Duration,
    /// 普通成功先进静默池，攒满这么久再交给大脑扫一遍。
    pub quiet_window: Duration,
    /// `None` 表示不发定时摘要。
    pub summary_every: Option<Duration>,
    /// 提示里要求大脑执行的 `norma` 命令路径，必须与权限闸门的白名单一致。
    pub norma_bin: String,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_secs(1),
            quiet_window: Duration::from_secs(30),
            summary_every: Some(Duration::from_secs(300)),
            norma_bin: "norma".into(),
        }
    }
}

struct FinishedBatch {
    first_at: Instant,
    lines: Vec<String>,
}

pub struct Router {
    config: RouterConfig,
    /// 需要告知用户的完成事件。
    finished: Option<FinishedBatch>,
    /// 静默池：正常完成、默认不打扰。
    quiet: Option<FinishedBatch>,
    digest: BTreeMap<String, String>,
    /// 最近的完成记录（含静默的），用于用户回来时的摘要。
    activity: std::collections::VecDeque<(Instant, String)>,
    last_summary: Instant,
    open_approvals: HashSet<String>,
}

impl Router {
    pub fn new(config: RouterConfig, now: Instant) -> Self {
        Self {
            config,
            finished: None,
            quiet: None,
            digest: BTreeMap::new(),
            activity: std::collections::VecDeque::new(),
            last_summary: now,
            open_approvals: HashSet::new(),
        }
    }

    pub fn is_open(&self, approval_id: &str) -> bool {
        self.open_approvals.contains(approval_id)
    }

    pub fn push(&mut self, event: BrainEvent, now: Instant) -> Vec<Action> {
        let verdict = triage(&event);
        match event {
            BrainEvent::AgentProgress { agent, text } => {
                self.digest.insert(agent, text);
                if self.digest.len() > MAX_DIGEST_LINES {
                    self.digest.pop_first();
                }
                vec![]
            }
            BrainEvent::AgentFinished {
                agent,
                outcome,
                excerpt,
                ..
            } => {
                self.digest.remove(&agent);
                self.activity
                    .push_back((now, prompt::finished_line(&agent, outcome, &excerpt)));
                while self.activity.len() > MAX_ACTIVITY {
                    self.activity.pop_front();
                }
                let slot = match verdict {
                    Verdict::Speak(_) => &mut self.finished,
                    Verdict::Quiet => &mut self.quiet,
                };
                let batch = slot.get_or_insert(FinishedBatch {
                    first_at: now,
                    lines: vec![],
                });
                batch
                    .lines
                    .push(prompt::finished_line(&agent, outcome, &excerpt));
                // 失败不等窗口：把批次的起点提前，下一次 poll 就发。
                if outcome == Outcome::Failed && batch.first_at + self.config.debounce > now {
                    batch.first_at = now - self.config.debounce;
                }
                vec![]
            }
            BrainEvent::UserReturned { away } => {
                let since = now.checked_sub(away).unwrap_or(now);
                let lines: Vec<String> = self
                    .activity
                    .iter()
                    .filter(|(at, _)| *at >= since)
                    .map(|(_, line)| line.clone())
                    .collect();
                if lines.is_empty() {
                    return vec![];
                }
                vec![Action {
                    lane: Lane::Main,
                    text: prompt::on_return(away.as_secs() / 60, &lines),
                    approval_id: None,
                }]
            }
            BrainEvent::ApprovalRequested(request) => self.route_approval(request),
            BrainEvent::ApprovalClosed { id } => {
                self.open_approvals.remove(&id);
                vec![]
            }
            BrainEvent::UserRequest { text } => vec![Action {
                lane: Lane::Main,
                text: prompt::user_request(&text),
                approval_id: None,
            }],
            // 目标由 Dispatcher 记录，随后续提示一起发给大脑，这里不触发一轮。
            BrainEvent::Goal { .. } => vec![],
        }
    }

    fn route_approval(&mut self, request: ApprovalRequest) -> Vec<Action> {
        self.open_approvals.insert(request.id.clone());
        vec![Action {
            lane: Lane::Approval,
            text: prompt::approval(&request, &self.config.norma_bin),
            approval_id: Some(request.id),
        }]
    }

    /// 刷新到期的去抖批次与定时摘要。
    pub fn poll(&mut self, now: Instant) -> Vec<Action> {
        let mut actions = Vec::new();
        if let Some(batch) = &self.finished {
            if now >= batch.first_at + self.config.debounce {
                let batch = self.finished.take().expect("批次刚检查过存在");
                actions.push(Action {
                    lane: Lane::Main,
                    text: prompt::urgent_batch(&batch.lines),
                    approval_id: None,
                });
                self.last_summary = now;
            }
        }
        if let Some(batch) = &self.quiet {
            if now >= batch.first_at + self.config.quiet_window {
                let batch = self.quiet.take().expect("静默批次刚检查过存在");
                actions.push(Action {
                    lane: Lane::Main,
                    text: prompt::quiet_batch(&batch.lines),
                    approval_id: None,
                });
                self.last_summary = now;
            }
        }
        if let Some(every) = self.config.summary_every {
            if now >= self.last_summary + every {
                self.last_summary = now;
                if !self.digest.is_empty() {
                    actions.push(Action {
                        lane: Lane::Main,
                        text: prompt::summary(&self.digest),
                        approval_id: None,
                    });
                    self.digest.clear();
                }
            }
        }
        actions
    }

    /// 下一次需要调用 `poll` 的时间。
    pub fn next_deadline(&self) -> Option<Instant> {
        let finished = self
            .finished
            .as_ref()
            .map(|b| b.first_at + self.config.debounce);
        let quiet = self
            .quiet
            .as_ref()
            .map(|b| b.first_at + self.config.quiet_window);
        let summary = (!self.digest.is_empty())
            .then(|| self.config.summary_every.map(|e| self.last_summary + e))
            .flatten();
        [finished, quiet, summary].into_iter().flatten().min()
    }
}

#[cfg(test)]
#[path = "../tests/unit/router_tests.rs"]
mod router_tests;
