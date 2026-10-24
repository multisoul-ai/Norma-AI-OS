//! 队列推进逻辑：批量、退避、持久化。时间注入，线程在 `worker.rs` 驱动。

use std::collections::VecDeque;

use crate::queue::{DiskQueue, Queued, MAX_QUEUED};
use crate::sink::{SendOutcome, Sink};

pub const BATCH_SIZE: usize = 20;
pub const SEND_INTERVAL_MS: u64 = 30_000;
const MAX_BACKOFF_MS: u64 = 3_600_000;

pub struct Pipeline<S: Sink> {
    sink: S,
    store: DiskQueue,
    queue: VecDeque<Queued>,
    enabled: bool,
    last_attempt_ms: u64,
    next_attempt_ms: u64,
    failures: u32,
}

impl<S: Sink> Pipeline<S> {
    pub fn new(sink: S, store: DiskQueue, enabled: bool) -> Self {
        let queue = if enabled {
            store.load()
        } else {
            VecDeque::new()
        };
        if !enabled {
            store.clear();
        }
        Self {
            sink,
            store,
            queue,
            enabled,
            last_attempt_ms: 0,
            next_attempt_ms: 0,
            failures: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.queue.clear();
            self.store.clear();
        }
    }

    pub fn push(&mut self, item: Queued) {
        if !self.enabled {
            return;
        }
        if self.queue.len() >= MAX_QUEUED {
            self.queue.pop_front();
        }
        self.queue.push_back(item);
    }

    /// 到达批量阈值或发送间隔，且不在退避期内时发送一批。
    pub fn tick(&mut self, now_ms: u64) {
        let due = self.queue.len() >= BATCH_SIZE
            || now_ms.saturating_sub(self.last_attempt_ms) >= SEND_INTERVAL_MS;
        if due && now_ms >= self.next_attempt_ms {
            self.send_one_batch(now_ms);
        }
    }

    /// 尽力清空队列（退出 / 失活时调用）；失败的事件落盘留待下次启动。
    pub fn flush(&mut self, now_ms: u64) {
        while self.enabled && !self.queue.is_empty() {
            if !self.send_one_batch(now_ms) {
                break;
            }
        }
        self.persist();
    }

    /// 返回本批是否被服务端处理（Sent 或 Drop）。
    fn send_one_batch(&mut self, now_ms: u64) -> bool {
        if !self.enabled || self.queue.is_empty() {
            return false;
        }
        self.last_attempt_ms = now_ms;
        let take = self.queue.len().min(BATCH_SIZE);
        let batch: Vec<Queued> = self.queue.iter().take(take).cloned().collect();
        let outcome = self.sink.send(&batch);
        tracing::info!(
            count = batch.len(),
            ?outcome,
            left = self.queue.len(),
            "analytics batch sent"
        );
        match outcome {
            SendOutcome::Sent | SendOutcome::Drop => {
                self.queue.drain(..take);
                self.failures = 0;
                self.next_attempt_ms = 0;
                self.persist();
                true
            }
            SendOutcome::Retry => {
                self.failures = self.failures.saturating_add(1);
                let backoff = (SEND_INTERVAL_MS << self.failures.min(7)).min(MAX_BACKOFF_MS);
                self.next_attempt_ms = now_ms + backoff;
                self.persist();
                false
            }
        }
    }

    fn persist(&self) {
        self.store.save(&self.queue);
    }
}
