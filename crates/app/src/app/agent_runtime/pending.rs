use crate::app::MultisoulApp;
use agent::AgentDispatchReceipt;
use canvas::NodeId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::command_support::{submit_agent_enter, type_agent_task};
mod realtime;

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
    RecoveryRequired,
}

#[derive(Debug, Default)]
pub(crate) struct PendingAgentSubmissions {
    ready_after: HashMap<NodeId, Instant>,
    submissions: HashMap<NodeId, PendingAgentSubmission>,
    realtime: HashMap<u64, realtime::Delivery>,
}

#[derive(Debug)]
pub(crate) struct RemovedPendingSubmission {
    pub(crate) receipt: AgentDispatchReceipt,
}

impl PendingAgentSubmissions {
    pub(crate) fn input_in_progress(&self, node_id: NodeId) -> bool {
        self.submissions.contains_key(&node_id)
    }

    pub(crate) fn has_pending_for_node(&self, node_id: NodeId) -> bool {
        self.ready_after.contains_key(&node_id) || self.submissions.contains_key(&node_id)
    }

    pub(crate) fn mark_starting(&mut self, node_id: NodeId) {
        self.mark_starting_at(node_id, Instant::now());
    }

    fn mark_starting_at(&mut self, node_id: NodeId, started_at: Instant) {
        self.ready_after
            .insert(node_id, started_at + AGENT_STARTUP_GRACE);
    }

    /// 暂缓向该卡片输入任务，直到 `until`（想法流星飞到卡片之前，任务不能先开始）。
    /// 已有更晚的等待时间时保留更晚的那个。
    pub(crate) fn hold_until(&mut self, node_id: NodeId, until: Instant) {
        let entry = self.ready_after.entry(node_id).or_insert(until);
        *entry = (*entry).max(until);
    }

    /// 派发失败时撤销自己加的暂缓：只在等待时间仍是 `until`、且没有待输入任务时才清除，
    /// 避免留下一条永远不会被消费的等待记录（它会让手机端对该卡片一直返回“未就绪”）。
    pub(crate) fn release_hold(&mut self, node_id: NodeId, until: Instant) {
        if self.ready_after.get(&node_id) == Some(&until)
            && !self.submissions.contains_key(&node_id)
        {
            self.ready_after.remove(&node_id);
        }
    }

    pub(crate) fn should_wait(&mut self, node_id: NodeId) -> bool {
        self.should_wait_at(node_id, Instant::now())
    }

    fn should_wait_at(&mut self, node_id: NodeId, now: Instant) -> bool {
        if self.submissions.contains_key(&node_id) {
            return true;
        }
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
        self.submissions
            .entry(receipt.node_id)
            .or_insert(PendingAgentSubmission {
                receipt,
                stage: PendingSubmissionStage::TypeTask,
            });
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
        if self.submissions.contains_key(&node_id) {
            return;
        }
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
        self.realtime.retain(|_, item| item.node != node_id);
    }

    pub(crate) fn remove_task(&mut self, task_id: u64) -> Option<RemovedPendingSubmission> {
        self.realtime.remove(&task_id);
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
            (pending.receipt.task_id == task_id)
                .then_some((*node_id, pending.stage != PendingSubmissionStage::TypeTask))
        })
    }

    pub(crate) fn clear(&mut self) {
        self.ready_after.clear();
        self.submissions.clear();
        self.realtime.clear();
    }
}

impl MultisoulApp {
    pub(crate) fn submit_due_pending_agent_tasks(
        &mut self,
        now: Instant,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let node_ids = self.pending_agent_submissions.due_node_ids(now);
        node_ids.into_iter().fold(false, |changed, node_id| {
            self.submit_pending_agent_task_for_ready_node(node_id, cx) || changed
        })
    }

    pub(crate) fn submit_pending_agent_task_for_ready_node(
        &mut self,
        node_id: NodeId,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        tracing::info!(node_id, "agent task ready: trying to submit pending task");
        if self.runtime.agent_task_input_held(node_id)
            || matches!(
                self.runtime.agent_task_state(node_id),
                Some(
                    runtime::AgentTaskState::WaitingApproval { .. }
                        | runtime::AgentTaskState::WaitingQuestion
                )
            )
            || self
                .pending_agent_submissions
                .submissions
                .get(&node_id)
                .is_some_and(|pending| pending.stage == PendingSubmissionStage::RecoveryRequired)
        {
            return false;
        }
        let Some(pending) = self.pending_agent_submissions.take_ready(node_id) else {
            return false;
        };
        if self.mobile_delivery_pending(node_id)
            || !self.prepare_realtime_submission(pending.receipt.task_id, cx)
        {
            tracing::info!(
                node_id,
                "agent task submission deferred: delivery capture in progress"
            );
            // Keep the exact stage: a task already pasted must not be pasted a second time.
            self.pending_agent_submissions
                .submissions
                .insert(node_id, pending);
            self.pending_agent_submissions
                .ready_after
                .insert(node_id, Instant::now() + Duration::from_millis(50));
            return false;
        }
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
                        Some(ui::i18n::LocalizedText::new("agent.retry_typing"));
                    self.pending_agent_submissions.requeue(receipt);
                    false
                }
            };
        }
        match submit_agent_enter(&mut self.runtime, receipt.node_id) {
            Ok(()) => {
                self.realtime_submission_sent(receipt.task_id, cx);
                if let Some(coordinator) = self.agent_task_coordinator.as_mut() {
                    coordinator.mark_task_submitted(receipt.task_id);
                    if let Some(next) = coordinator.next_unsubmitted(receipt.definition_id) {
                        self.pending_agent_submissions.enqueue_immediate(next);
                    }
                }
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
                        Some(ui::i18n::LocalizedText::new("agent.retry_submission"));
                    self.pending_agent_submissions.requeue(receipt);
                } else {
                    let message = error.to_string();
                    self.workspace_state.error = Some(unsafe_submission_recovery_message(&message));
                    if let Some(coordinator) = self.agent_task_coordinator.as_mut() {
                        coordinator
                            .fail_active_preserving_busy(receipt.definition_id, receipt.task_id);
                    }
                    if let Some(writer) = self.persistence.agent_dispatch_writer() {
                        writer.fail_next_queued(receipt.definition_id, message);
                    }
                    // Unknown input contents block later delivery until the card is closed.
                    self.pending_agent_submissions.submissions.insert(
                        node_id,
                        PendingAgentSubmission {
                            receipt,
                            stage: PendingSubmissionStage::RecoveryRequired,
                        },
                    );
                }
                false
            }
        }
    }
}

pub(super) fn unsafe_submission_recovery_message(message: &str) -> ui::i18n::LocalizedText {
    ui::i18n::LocalizedText::new("agent.recover_submission").with_arg("reason", message)
}

#[cfg(test)]
#[path = "../../../tests/unit/agent_pending_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/agent_pending_cancellation_tests.rs"]
mod cancellation_tests;
