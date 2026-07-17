use crate::app::MultisoulApp;
use agent::AgentDispatchReceipt;
use canvas::NodeId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::commands::{submit_agent_enter, type_agent_task};

const AGENT_STARTUP_GRACE: Duration = Duration::from_secs(30);
const AGENT_SUBMISSION_RETRY_DELAY: Duration = Duration::from_millis(500);
const AGENT_ENTER_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug)]
struct PendingAgentSubmission {
    receipt: AgentDispatchReceipt,
    stage: PendingSubmissionStage,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingSubmissionStage {
    TypeTask,
    SubmitEnter,
}

#[derive(Debug, Default)]
pub(crate) struct PendingAgentSubmissions {
    ready_after: HashMap<NodeId, Instant>,
    submissions: HashMap<NodeId, PendingAgentSubmission>,
}

impl PendingAgentSubmissions {
    pub(crate) fn mark_starting(&mut self, node_id: NodeId) {
        self.mark_starting_at(node_id, Instant::now());
    }

    fn mark_starting_at(&mut self, node_id: NodeId, started_at: Instant) {
        self.ready_after
            .insert(node_id, started_at + AGENT_STARTUP_GRACE);
    }

    pub(crate) fn should_wait(&mut self, node_id: NodeId) -> bool {
        self.should_wait_at(node_id, Instant::now())
    }

    fn should_wait_at(&mut self, node_id: NodeId, now: Instant) -> bool {
        let Some(ready_after) = self.ready_after.get(&node_id).copied() else {
            return false;
        };
        if ready_after > now {
            return true;
        }
        self.ready_after.remove(&node_id);
        false
    }

    #[cfg(any(test, feature = "e2e-test-support"))]
    pub(crate) fn is_waiting(&self, node_id: NodeId) -> bool {
        self.ready_after.contains_key(&node_id)
    }

    pub(crate) fn mark_ready(&mut self, node_id: NodeId) -> bool {
        self.ready_after.remove(&node_id).is_some()
    }

    pub(crate) fn enqueue(&mut self, receipt: AgentDispatchReceipt) {
        self.submissions.insert(
            receipt.node_id,
            PendingAgentSubmission {
                receipt,
                stage: PendingSubmissionStage::TypeTask,
            },
        );
    }

    pub(crate) fn schedule_enter(&mut self, receipt: AgentDispatchReceipt) {
        self.schedule_enter_at(receipt, Instant::now());
    }

    fn schedule_enter_at(&mut self, receipt: AgentDispatchReceipt, typed_at: Instant) {
        self.ready_after
            .insert(receipt.node_id, typed_at + AGENT_ENTER_DELAY);
        self.submissions.insert(
            receipt.node_id,
            PendingAgentSubmission {
                receipt,
                stage: PendingSubmissionStage::SubmitEnter,
            },
        );
    }

    pub(crate) fn enqueue_immediate(&mut self, receipt: AgentDispatchReceipt) {
        let node_id = receipt.node_id;
        self.enqueue(receipt);
        self.ready_after.insert(node_id, Instant::now());
    }

    fn take_ready(&mut self, node_id: NodeId) -> Option<PendingAgentSubmission> {
        self.mark_ready(node_id);
        self.submissions.remove(&node_id)
    }

    fn requeue(&mut self, receipt: AgentDispatchReceipt) {
        self.requeue_at(receipt, Instant::now());
    }

    fn requeue_at(&mut self, receipt: AgentDispatchReceipt, failed_at: Instant) {
        self.ready_after
            .insert(receipt.node_id, failed_at + AGENT_SUBMISSION_RETRY_DELAY);
        self.submissions.insert(
            receipt.node_id,
            PendingAgentSubmission {
                receipt,
                stage: PendingSubmissionStage::TypeTask,
            },
        );
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

    pub(crate) fn remove_node(&mut self, node_id: NodeId) {
        self.ready_after.remove(&node_id);
        self.submissions.remove(&node_id);
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
        let Some(pending) = self.pending_agent_submissions.take_ready(node_id) else {
            return false;
        };
        let receipt = pending.receipt;
        if pending.stage == PendingSubmissionStage::TypeTask {
            return match type_agent_task(&mut self.runtime, receipt.node_id, &receipt.task) {
                Ok(()) => {
                    self.pending_agent_submissions.schedule_enter(receipt);
                    true
                }
                Err(error) => {
                    tracing::error!(
                        definition_id = receipt.definition_id,
                        task_id = receipt.task_id,
                        %error,
                        "failed to type Agent task after CLI became ready"
                    );
                    self.workspace_state.error =
                        Some("Agent 任务尚未输入，正在等待 CLI 就绪后自动重试".to_string());
                    self.pending_agent_submissions.requeue(receipt);
                    false
                }
            };
        }
        match submit_agent_enter(&mut self.runtime, receipt.node_id) {
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
                    let message = error.to_string();
                    self.workspace_state.error = Some(unsafe_submission_recovery_message(&message));
                    if let Some(writer) = self.persistence.agent_dispatch_writer() {
                        writer.fail_next_queued(receipt.definition_id, message);
                    }
                }
                false
            }
        }
    }
}

pub(super) fn unsafe_submission_recovery_message(message: &str) -> String {
    format!(
        "Agent 任务提交失败：{message}。输入区状态不确定；请连续关闭两次此 Agent 卡片以中断并清空会话，再重新激活后派发任务"
    )
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
    ///   startup grace = 30,000ms（真实 Codex 在 MCP 启动超过 15 秒时仍未接受输入）；
    ///   early probe = 30,000ms - 1ms = 29,999ms；due probe = 30,000ms。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在固定 Instant 标记 node 7 正在启动并排入 1 项任务；
    ///   2. 在 29,999ms 查询可兜底提交节点；
    ///   3. 在 30,000ms 再次查询可兜底提交节点。
    ///
    /// 预期结果：
    ///   - 正断言：30,000ms 时 node 7 恰好可提交；
    ///   - 负断言：29,999ms 时结果为空，且不包含无关 node 8。
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

        let early = pending.due_node_ids(started_at + Duration::from_millis(29_999));
        let due = pending.due_node_ids(started_at + Duration::from_millis(30_000));

        assert!(
            early.is_empty(),
            "启动 29,999ms 时不得提前提交，实际 due nodes：{early:?}"
        );
        assert_eq!(due, vec![7], "启动满 30,000ms 后应只有 node 7 进入兜底提交");
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
    ///   due probe = startup grace 30,000ms，两个节点原本都应进入兜底提交。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在固定 Instant 将 node 7、node 8 标记为启动中并分别排入任务；
    ///   2. 模拟关闭 node 7，清理其 pending 状态；
    ///   3. 在 30,000ms 查询仍可兜底提交的节点。
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
        let due = pending.due_node_ids(started_at + Duration::from_millis(30_000));

        assert!(
            !pending.is_waiting(7),
            "关闭 node 7 后不得继续保留其启动等待标记"
        );
        assert!(
            !due.contains(&7),
            "关闭 node 7 后不得继续兜底提交其任务，实际 due nodes：{due:?}"
        );
        assert!(
            pending.is_waiting(8),
            "清理 node 7 不得误删仍存活 node 8 的启动等待标记"
        );
        assert_eq!(due, vec![8], "清理 node 7 后应只保留 node 8 的待发送任务");
    }

    /// ART-READY-3：启动宽限已过且没有待发送任务时，后续派发应把 Agent 视为已就绪。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   startup grace = 30,000ms；early probe = 30,000ms - 1ms = 29,999ms；
    ///   expiry probe = 30,000ms；pending tasks = 0。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在固定 Instant 标记 node 7 正在启动，但不排入任务；
    ///   2. 在 29,999ms 查询是否仍需等待；
    ///   3. 在 30,000ms 再查询，并检查过期标记是否被清除。
    ///
    /// 预期结果：
    ///   - 正断言：29,999ms 时仍需等待；
    ///   - 负断言：30,000ms 后不再等待，也不残留启动标记。
    #[test]
    fn expired_startup_marker_does_not_delay_later_dispatch() {
        let started_at = Instant::now();
        let mut pending = PendingAgentSubmissions::default();
        pending.mark_starting_at(7, started_at);

        let waits_early = pending.should_wait_at(7, started_at + Duration::from_millis(29_999));
        let waits_after_expiry =
            pending.should_wait_at(7, started_at + Duration::from_millis(30_000));

        assert!(waits_early, "启动 29,999ms 时 node 7 仍应等待 CLI 就绪");
        assert!(
            !waits_after_expiry,
            "启动满 30,000ms 且无 pending task 时，node 7 应立即视为可提交"
        );
        assert!(
            !pending.is_waiting(7),
            "启动标记过期后必须清除，避免后续派发错误返回 Queued"
        );
        assert!(
            !pending.is_waiting(8),
            "过期检查不得产生未登记的 node 8 启动标记"
        );
    }

    /// ART-READY-4：可安全重试的提交失败必须等待完整 500ms 后重新进入可提交集合。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   retry delay = 500ms；early probe = 500ms - 1ms = 499ms；
    ///   due probe = 500ms；pending tasks = 1。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在固定失败时刻重新排入 node 7 的任务；
    ///   2. 在 499ms 查询可重试节点；
    ///   3. 在 500ms 再次查询可重试节点。
    ///
    /// 预期结果：
    ///   - 正断言：500ms 时 node 7 恰好可重试；
    ///   - 负断言：499ms 时不得提前重试，也不得产生 node 8。
    #[test]
    fn retry_safe_failure_waits_for_full_retry_delay() {
        let failed_at = Instant::now();
        let mut pending = PendingAgentSubmissions::default();
        pending.requeue_at(
            AgentDispatchReceipt {
                task_id: 1,
                definition_id: 3,
                name: "Alan".to_string(),
                node_id: 7,
                task: "检查项目".to_string(),
                state: AgentDispatchState::SubmittedManaged,
            },
            failed_at,
        );

        let early = pending.due_node_ids(failed_at + Duration::from_millis(499));
        let due = pending.due_node_ids(failed_at + Duration::from_millis(500));

        assert!(
            early.is_empty(),
            "提交失败 499ms 时不得提前重试，实际 due nodes：{early:?}"
        );
        assert_eq!(due, vec![7], "提交失败满 500ms 后应只有 node 7 可安全重试");
        assert!(
            !due.contains(&8),
            "重试查询不得产生未登记的 node 8，实际 due nodes：{due:?}"
        );
    }

    /// ART-READY-5：正文写入后必须等待完整 100ms，才能发送 Enter。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   task bytes = 12（“检查项目”的 UTF-8 为 4 × 3 bytes）；
    ///   enter delay = 100ms；early probe = 100ms - 1ms = 99ms；
    ///   due probe = 100ms；pending stage = SubmitEnter。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在固定正文写入时刻把 node 7 切换到 SubmitEnter 阶段；
    ///   2. 在 99ms 查询可发送 Enter 的节点；
    ///   3. 在 100ms 再次查询。
    ///
    /// 预期结果：
    ///   - 正断言：100ms 时只有 node 7 可发送 Enter；
    ///   - 负断言：99ms 时不得提前发送，也不得产生 node 8。
    #[test]
    fn enter_submission_waits_for_paste_batch_to_settle() {
        let typed_at = Instant::now();
        let mut pending = PendingAgentSubmissions::default();
        pending.schedule_enter_at(
            AgentDispatchReceipt {
                task_id: 1,
                definition_id: 3,
                name: "Alan".to_string(),
                node_id: 7,
                task: "检查项目".to_string(),
                state: AgentDispatchState::SubmittedManaged,
            },
            typed_at,
        );

        let early = pending.due_node_ids(typed_at + Duration::from_millis(99));
        let due = pending.due_node_ids(typed_at + Duration::from_millis(100));

        assert!(
            early.is_empty(),
            "正文写入 99ms 时不得提前发送 Enter，实际 due nodes：{early:?}"
        );
        assert_eq!(
            due,
            vec![7],
            "正文写入满 100ms 后应只有 node 7 可发送 Enter"
        );
        assert!(
            !due.contains(&8),
            "Enter 查询不得产生未登记的 node 8，实际 due nodes：{due:?}"
        );
    }
}
