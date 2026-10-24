//! 事件分流：去抖、定时摘要、高风险直通人工。纯逻辑，时间由调用方传入。

use crate::event::{ApprovalRequest, BrainEvent, Outcome};
use crate::lane::prompt;
use crate::risk::{risk_class, RiskClass};
use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};

const MAX_DIGEST_LINES: usize = 40;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Lane {
    Main,
    Approval,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    Prompt {
        lane: Lane,
        text: String,
        /// 审批提示带上审批标识，便于在发送前检查是否已关闭。
        approval_id: Option<String>,
    },
    /// 命中高风险规则：不经过大脑，直接转人工。
    Escalate { id: String, reason: String },
}

#[derive(Clone, Debug)]
pub struct RouterConfig {
    pub debounce: Duration,
    /// `None` 表示不发定时摘要。
    pub summary_every: Option<Duration>,
    /// 提示里要求大脑执行的 `norma` 命令路径，必须与权限闸门的白名单一致。
    pub norma_bin: String,
}

impl Default for RouterConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_secs(1),
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
    finished: Option<FinishedBatch>,
    digest: BTreeMap<String, String>,
    last_summary: Instant,
    open_approvals: HashSet<String>,
}

impl Router {
    pub fn new(config: RouterConfig, now: Instant) -> Self {
        Self {
            config,
            finished: None,
            digest: BTreeMap::new(),
            last_summary: now,
            open_approvals: HashSet::new(),
        }
    }

    pub fn is_open(&self, approval_id: &str) -> bool {
        self.open_approvals.contains(approval_id)
    }

    pub fn push(&mut self, event: BrainEvent, now: Instant) -> Vec<Action> {
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
            } => {
                self.digest.remove(&agent);
                let batch = self.finished.get_or_insert(FinishedBatch {
                    first_at: now,
                    lines: vec![],
                });
                batch
                    .lines
                    .push(prompt::finished_line(&agent, outcome, &excerpt));
                if outcome == Outcome::Failed && batch.first_at + self.config.debounce > now {
                    batch.first_at = now - self.config.debounce;
                }
                vec![]
            }
            BrainEvent::ApprovalRequested(request) => self.route_approval(request),
            BrainEvent::ApprovalClosed { id } => {
                self.open_approvals.remove(&id);
                vec![]
            }
            BrainEvent::UserRequest { text } => vec![Action::Prompt {
                lane: Lane::Main,
                text: prompt::user_request(&text),
                approval_id: None,
            }],
            // 目标由 Dispatcher 记录，随后续提示一起发给大脑，这里不触发一轮。
            BrainEvent::Goal { .. } => vec![],
        }
    }

    fn route_approval(&mut self, request: ApprovalRequest) -> Vec<Action> {
        if let RiskClass::High(reason) = risk_class(&request.subject) {
            return vec![Action::Escalate {
                id: request.id,
                reason: format!("{reason:?}"),
            }];
        }
        self.open_approvals.insert(request.id.clone());
        vec![Action::Prompt {
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
                actions.push(Action::Prompt {
                    lane: Lane::Main,
                    text: prompt::finished_batch(&batch.lines),
                    approval_id: None,
                });
                self.last_summary = now;
            }
        }
        if let Some(every) = self.config.summary_every {
            if now >= self.last_summary + every {
                self.last_summary = now;
                if !self.digest.is_empty() {
                    actions.push(Action::Prompt {
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
        let summary = (!self.digest.is_empty())
            .then(|| self.config.summary_every.map(|e| self.last_summary + e))
            .flatten();
        [finished, summary].into_iter().flatten().min()
    }
}

#[cfg(test)]
#[path = "../tests/unit/router_tests.rs"]
mod router_tests;
