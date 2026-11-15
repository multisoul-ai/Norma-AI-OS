//! 用户从任务里移除 Worker：记录搬到 `removed_workers`，对话历史不动；撤销时原位放回。
use super::{Task, Worker};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RemovedWorker {
    pub worker: Worker,
    /// 移除前在 `workers` 里的位置，撤销时放回原处。
    pub index: usize,
}

impl Task {
    /// 把 Worker 从任务里拿走；之后它的事件不再被任务接收。找不到返回 false。
    pub fn remove_worker(&mut self, dispatch_id: &str) -> bool {
        let Some(index) = self
            .workers
            .iter()
            .position(|w| w.dispatch_id == dispatch_id)
        else {
            return false;
        };
        let worker = self.workers.remove(index);
        self.removed_workers.push(RemovedWorker { worker, index });
        self.updated_at = super::timestamp();
        true
    }

    /// 撤销移除：放回原位置（任务期间 Worker 数量变少时放到末尾）。
    pub fn restore_worker(&mut self, dispatch_id: &str) -> bool {
        let Some(at) = self
            .removed_workers
            .iter()
            .position(|removed| removed.worker.dispatch_id == dispatch_id)
        else {
            return false;
        };
        let removed = self.removed_workers.remove(at);
        let index = removed.index.min(self.workers.len());
        self.workers.insert(index, removed.worker);
        self.updated_at = super::timestamp();
        true
    }
}
