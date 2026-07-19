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

#[derive(Debug)]
pub(crate) struct RemovedPendingSubmission {
    pub(crate) receipt: AgentDispatchReceipt,
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

    pub(crate) fn remove_task(&mut self, task_id: u64) -> Option<RemovedPendingSubmission> {
        let node_id = self.submissions.iter().find_map(|(node_id, pending)| {
            (pending.receipt.task_id == task_id).then_some(*node_id)
        })?;
        self.ready_after.remove(&node_id);
        let pending = self
            .submissions
            .remove(&node_id)
            .expect("task lookup and removal use the same pending submission map");
        Some(RemovedPendingSubmission {
            receipt: pending.receipt,
        })
    }

    pub(crate) fn task_stage(&self, task_id: u64) -> Option<(NodeId, bool)> {
        self.submissions.iter().find_map(|(node_id, pending)| {
            (pending.receipt.task_id == task_id).then_some((
                *node_id,
                pending.stage == PendingSubmissionStage::SubmitEnter,
            ))
        })
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
#[path = "../../../tests/unit/agent_pending_tests.rs"]
mod tests;
