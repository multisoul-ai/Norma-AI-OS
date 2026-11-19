//! Claude 幕僚最多 4 个会话。都在忙时新任务排队；有空闲会话才驱逐最久没动的那个。
use super::worker::Job;
use crate::staff::gate::StaffTokens;
use norma::protocol::BrainScope;
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// 同时活着的 Claude 会话上限。第 5 个任务要等一个空出来。
pub const CLAUDE_SESSION_LIMIT: usize = 4;

/// 会话入口的键：同一个任务号在不同路由下是不同的会话。
pub(crate) type SlotKey = (BrainScope, String);

pub(crate) enum Submit {
    /// 直接开始。`evicted` 是被腾出的空闲会话，调用方要停掉它的线程。
    Run {
        job: Box<Job>,
        evicted: Option<SlotKey>,
    },
    /// 4 个都在忙，先排队。面板用 `staff.worker_queued`。
    Wait,
}

struct Slot {
    scope: BrainScope,
    task: String,
    idle_at: Option<Instant>,
}

pub(crate) struct StaffPool {
    limit: usize,
    slots: Vec<Slot>,
    waiting: VecDeque<Job>,
    tokens: Option<StaffTokens>,
    held: HashMap<String, String>,
}

impl StaffPool {
    pub(crate) fn new(limit: usize, tokens: Option<StaffTokens>) -> Self {
        Self {
            limit,
            slots: Vec::new(),
            waiting: VecDeque::new(),
            tokens,
            held: HashMap::new(),
        }
    }

    pub(crate) fn note(&mut self, task: &str, token: &str) {
        self.held.insert(task.to_owned(), token.to_owned());
    }

    pub(crate) fn submit(&mut self, job: Job) -> Submit {
        let Some(task) = job.task_id.clone() else {
            return Submit::Run {
                job: Box::new(job),
                evicted: None,
            };
        };
        if self.holds(&job.scope, &task) {
            self.mark_busy(&task);
            return Submit::Run {
                job: Box::new(job),
                evicted: None,
            };
        }
        if self.slots.len() < self.limit {
            self.claim(&job.scope, &task);
            return Submit::Run {
                job: Box::new(job),
                evicted: None,
            };
        }
        if let Some(old) = self.oldest_idle() {
            self.evict(&old.1);
            self.claim(&job.scope, &task);
            return Submit::Run {
                job: Box::new(job),
                evicted: Some(old),
            };
        }
        self.waiting.push_back(job);
        Submit::Wait
    }

    /// 这个任务刚结束一轮，正在等下一条。有人排队就把最久的空闲会话交出去。
    /// 返回 `(被驱逐的会话, 接下来要开始的任务)`。
    pub(crate) fn on_idle(&mut self, task: &str) -> Option<(SlotKey, Job)> {
        self.mark_idle(task);
        if self.waiting.is_empty() {
            return None;
        }
        let old = self.oldest_idle()?;
        self.evict(&old.1);
        let job = self.waiting.pop_front()?;
        self.claim_for(&job);
        Some((old, job))
    }

    /// 任务完成或归档：关掉它的会话，并丢掉它还在排队的任务。有人排队就让下一个开始。
    pub(crate) fn close(&mut self, task: &str) -> Option<Job> {
        let freed = self.evict(task);
        self.waiting
            .retain(|job| job.task_id.as_deref() != Some(task));
        if !freed {
            return None;
        }
        let job = self.waiting.pop_front()?;
        self.claim_for(&job);
        Some(job)
    }

    /// 路由被释放：它名下的会话和排队任务都作废，空出来的位置交给别的排队任务。
    pub(crate) fn release_scope(&mut self, scope: &BrainScope) -> Vec<Job> {
        self.waiting.retain(|job| &job.scope != scope);
        let tasks: Vec<String> = self
            .slots
            .iter()
            .filter(|slot| &slot.scope == scope)
            .map(|slot| slot.task.clone())
            .collect();
        tasks.iter().filter_map(|task| self.close(task)).collect()
    }

    /// 会话是否还占着名额。被驱逐的线程看到 false 就该退出。
    pub(crate) fn holds(&self, scope: &BrainScope, task: &str) -> bool {
        self.slots
            .iter()
            .any(|slot| &slot.scope == scope && slot.task == task)
    }

    #[cfg(test)]
    pub(crate) fn slots_for_test(&self, task: &str) -> usize {
        self.slots.iter().filter(|slot| slot.task == task).count()
    }

    fn claim_for(&mut self, job: &Job) {
        if let Some(task) = &job.task_id {
            self.claim(&job.scope, task);
        }
    }

    fn claim(&mut self, scope: &BrainScope, task: &str) {
        if !self.holds(scope, task) {
            self.slots.push(Slot {
                scope: scope.clone(),
                task: task.to_owned(),
                idle_at: None,
            });
        }
    }

    pub(crate) fn mark_busy(&mut self, task: &str) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.task == task) {
            slot.idle_at = None;
        }
    }

    fn mark_idle(&mut self, task: &str) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.task == task) {
            if slot.idle_at.is_none() {
                slot.idle_at = Some(Instant::now());
            }
        }
    }

    fn oldest_idle(&self) -> Option<SlotKey> {
        self.slots
            .iter()
            .filter(|slot| slot.idle_at.is_some())
            .min_by_key(|slot| slot.idle_at)
            .map(|slot| (slot.scope.clone(), slot.task.clone()))
    }

    /// 撤销 token 并释放名额；返回是否真的释放了一个名额。
    fn evict(&mut self, task: &str) -> bool {
        if let Some(token) = self.held.remove(task) {
            if let Some(tokens) = &self.tokens {
                tokens.revoke(&token);
            }
        }
        let before = self.slots.len();
        self.slots.retain(|slot| slot.task != task);
        self.slots.len() < before
    }
}

#[cfg(test)]
#[path = "../../tests/unit/staff_pool_tests.rs"]
mod tests;
