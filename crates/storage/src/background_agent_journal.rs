use crate::AgentTaskStore;
use agent::{AuditEntry, CardTaskJournal};
use std::sync::mpsc::{self, Sender};

/// 将 Agent 审计写入移出 GPUI/Runtime 调用线程。
///
/// Sender 使用无界 channel，append 只做内存消息投递；SQLite 开启、migration 和写入均在
/// 专用线程完成。线程句柄刻意 detach，关闭窗口时不会 join 阻塞前台；channel 关闭后 worker
/// 会先消费完队列再退出。
pub struct BackgroundAgentTaskJournal {
    sender: Sender<AuditEntry>,
    store: AgentTaskStore,
}

impl BackgroundAgentTaskJournal {
    pub fn new(store: AgentTaskStore) -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker_store = store.clone();
        std::thread::Builder::new()
            .name("norma-agent-journal".to_string())
            .spawn(move || {
                while let Ok(entry) = receiver.recv() {
                    if let Err(error) = worker_store.append_audit(entry) {
                        tracing::error!(%error, "failed to persist background agent audit entry");
                    }
                }
            })
            .expect("agent journal worker should spawn");
        Self { sender, store }
    }
}

impl CardTaskJournal for BackgroundAgentTaskJournal {
    fn append(&mut self, entry: AuditEntry) {
        let _ = self.sender.send(entry);
    }

    fn recover_incomplete(&mut self, project_id: &str) -> Vec<u64> {
        self.store
            .mark_incomplete_interrupted(project_id)
            .unwrap_or_default()
    }

    fn next_task_id(&self, project_id: &str) -> u64 {
        self.store.next_task_id(project_id)
    }
}
