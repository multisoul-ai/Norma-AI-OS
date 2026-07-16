use crate::app::MultisoulApp;
use agent::AgentDispatchReceipt;
use canvas::NodeId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::commands::submit_agent_task;

const AGENT_STARTUP_GRACE: Duration = Duration::from_secs(5);
const AGENT_SUBMISSION_RETRY_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Default)]
pub(crate) struct PendingAgentSubmissions {
    ready_after: HashMap<NodeId, Instant>,
    submissions: HashMap<NodeId, AgentDispatchReceipt>,
}

impl PendingAgentSubmissions {
    pub(crate) fn mark_starting(&mut self, node_id: NodeId) {
        self.mark_starting_at(node_id, Instant::now());
    }

    fn mark_starting_at(&mut self, node_id: NodeId, started_at: Instant) {
        self.ready_after
            .insert(node_id, started_at + AGENT_STARTUP_GRACE);
    }

    pub(crate) fn should_wait(&self, node_id: NodeId) -> bool {
        self.ready_after.contains_key(&node_id)
    }

    pub(crate) fn mark_ready(&mut self, node_id: NodeId) -> bool {
        self.ready_after.remove(&node_id).is_some()
    }

    pub(crate) fn enqueue(&mut self, receipt: AgentDispatchReceipt) {
        self.submissions.insert(receipt.node_id, receipt);
    }

    fn take_ready(&mut self, node_id: NodeId) -> Option<AgentDispatchReceipt> {
        self.mark_ready(node_id);
        self.submissions.remove(&node_id)
    }

    fn requeue(&mut self, receipt: AgentDispatchReceipt) {
        self.ready_after.insert(
            receipt.node_id,
            Instant::now() + AGENT_SUBMISSION_RETRY_DELAY,
        );
        self.submissions.insert(receipt.node_id, receipt);
    }

    fn due_node_ids(&self, now: Instant) -> Vec<NodeId> {
        let mut due = self
            .ready_after
            .iter()
            .filter_map(|(node_id, ready_after)| {
                (*ready_after <= now && self.submissions.contains_key(node_id)).then_some(*node_id)
            })
            .collect::<Vec<_>>();
        due.sort_unstable();
        due
    }

    pub(crate) fn clear(&mut self) {
        self.ready_after.clear();
        self.submissions.clear();
    }
}

impl MultisoulApp {
    pub(crate) fn submit_due_pending_agent_tasks(&mut self, now: Instant) -> bool {
        let node_ids = self.pending_agent_submissions.due_node_ids(now);
        node_ids.into_iter().fold(false, |changed, node_id| {
            self.submit_pending_agent_task_for_ready_node(node_id) || changed
        })
    }

    pub(crate) fn submit_pending_agent_task_for_ready_node(&mut self, node_id: NodeId) -> bool {
        let Some(receipt) = self.pending_agent_submissions.take_ready(node_id) else {
            return false;
        };
        match submit_agent_task(&mut self.runtime, receipt.node_id, &receipt.task) {
            Ok(()) => {
                if let Some(writer) = self.persistence.agent_dispatch_writer() {
                    writer.submit_next_queued(receipt.definition_id);
                }
                true
            }
            Err(error) => {
                tracing::error!(
                    definition_id = receipt.definition_id,
                    task_id = receipt.task_id,
                    %error,
                    "failed to submit Agent task after CLI became ready"
                );
                if error.retry_safe() {
                    self.workspace_state.error =
                        Some("Agent 任务尚未提交，正在等待 CLI 就绪后自动重试".to_string());
                    self.pending_agent_submissions.requeue(receipt);
                } else {
                    if let Some(coordinator) = self.agent_task_coordinator.as_mut() {
                        coordinator.reject_dispatch(receipt.definition_id, &[receipt.task_id]);
                    }
                    if let Some(writer) = self.persistence.agent_dispatch_writer() {
                        writer.fail_next_queued(receipt.definition_id, error.to_string());
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent::AgentDispatchState;
    use std::time::{Duration, Instant};

    /// ART-READY-1：没有 SessionStart 时，pending 任务必须等待完整启动窗口后才进入兜底提交。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   node_id = 7，pending tasks = 1；
    ///   startup grace = 5,000ms（Codex 真实 PTY 延迟 5 秒可提交的通过对照）；
    ///   early probe = 5,000ms - 1ms = 4,999ms；due probe = 5,000ms。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在固定 Instant 标记 node 7 正在启动并排入 1 项任务；
    ///   2. 在 4,999ms 查询可兜底提交节点；
    ///   3. 在 5,000ms 再次查询可兜底提交节点。
    ///
    /// 预期结果：
    ///   - 正断言：5,000ms 时 node 7 恰好可提交；
    ///   - 负断言：4,999ms 时结果为空，且不包含无关 node 8。
    #[test]
    fn pending_submission_fallback_waits_for_full_startup_grace() {
        let started_at = Instant::now();
        let mut pending = PendingAgentSubmissions::default();
        pending.mark_starting_at(7, started_at);
        pending.enqueue(AgentDispatchReceipt {
            task_id: 1,
            definition_id: 3,
            name: "Alan".to_string(),
            node_id: 7,
            task: "检查项目".to_string(),
            state: AgentDispatchState::SubmittedManaged,
        });

        let early = pending.due_node_ids(started_at + Duration::from_millis(4_999));
        let due = pending.due_node_ids(started_at + Duration::from_millis(5_000));

        assert!(
            early.is_empty(),
            "启动 4,999ms 时不得提前提交，实际 due nodes：{early:?}"
        );
        assert_eq!(due, vec![7], "启动满 5,000ms 后应只有 node 7 进入兜底提交");
        assert!(
            !due.contains(&8),
            "兜底查询不得产生未登记的 node 8，实际 due nodes：{due:?}"
        );
    }

    /// ART-READY-2：关闭尚未就绪的 Agent 时，必须同时移除其启动标记和待发送任务。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   node 7 = 即将关闭的 Agent，pending tasks = 1；
    ///   node 8 = 仍存活的 Agent，pending tasks = 1；
    ///   due probe = startup grace 5,000ms，两个节点原本都应进入兜底提交。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在固定 Instant 将 node 7、node 8 标记为启动中并分别排入任务；
    ///   2. 模拟关闭 node 7，清理其 pending 状态；
    ///   3. 在 5,000ms 查询仍可兜底提交的节点。
    ///
    /// 预期结果：
    ///   - 正断言：node 8 的任务仍保留并可提交；
    ///   - 负断言：node 7 不再等待，也不会在关闭后被兜底提交。
    #[test]
    fn remove_node_discards_only_closed_agent_submission() {
        let started_at = Instant::now();
        let mut pending = PendingAgentSubmissions::default();
        pending.mark_starting_at(7, started_at);
        pending.mark_starting_at(8, started_at);
        for (task_id, definition_id, node_id, name) in [(1, 3, 7, "Alan"), (2, 4, 8, "Codex")] {
            pending.enqueue(AgentDispatchReceipt {
                task_id,
                definition_id,
                name: name.to_string(),
                node_id,
                task: "检查项目".to_string(),
                state: AgentDispatchState::SubmittedManaged,
            });
        }

        pending.remove_node(7);
        let due = pending.due_node_ids(started_at + Duration::from_millis(5_000));

        assert!(
            !pending.should_wait(7),
            "关闭 node 7 后不得继续保留其启动等待标记"
        );
        assert!(
            !due.contains(&7),
            "关闭 node 7 后不得继续兜底提交其任务，实际 due nodes：{due:?}"
        );
        assert!(
            pending.should_wait(8),
            "清理 node 7 不得误删仍存活 node 8 的启动等待标记"
        );
        assert_eq!(due, vec![8], "清理 node 7 后应只保留 node 8 的待发送任务");
    }
}
