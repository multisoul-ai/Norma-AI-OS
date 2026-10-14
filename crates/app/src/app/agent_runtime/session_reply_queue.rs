use agent::{AgentReplyReceipt, AgentReplyStatus};
use canvas::{CanvasNodeKind, NodeId};
use runtime::AgentTaskState;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use super::command_support::{clear_agent_task_input, submit_agent_enter, type_agent_task};
use crate::app::MultisoulApp;

const AGENT_REPLY_ENTER_DELAY: Duration = Duration::from_millis(100);

#[derive(Clone, Debug)]
struct QueuedSessionReply {
    receipt: AgentReplyReceipt,
    stage: ReplyStage,
    ready_after: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReplyStage {
    WaitingForInputReady,
    SubmitEnter,
}

#[derive(Debug, Default)]
pub(crate) struct AgentSessionReplyQueue {
    next_reply_id: u64,
    entries: VecDeque<QueuedSessionReply>,
    connector_reply_ids: HashSet<u64>,
    connector_prompts: HashMap<(NodeId, i64, [u8; 32]), usize>,
}

impl AgentSessionReplyQueue {
    pub(crate) fn mark_connector_reply(
        &mut self,
        result: &agent::AdapterResult,
        definition_id: i64,
        message: &str,
    ) {
        match (
            result.reply_id,
            result.node_id,
            result.reply_status.as_deref(),
        ) {
            (Some(id), _, Some("queued")) => {
                self.connector_reply_ids.insert(id);
            }
            (_, Some(node), Some("sent")) => {
                self.record_connector_prompt(node, definition_id, message)
            }
            _ => {}
        }
    }
    fn record_connector_prompt(&mut self, node: NodeId, definition: i64, message: &str) {
        if let Some(hash) =
            norma::agent_hook::extract_prompt_fingerprint(&serde_json::json!({"prompt":message}))
        {
            let count = self
                .connector_prompts
                .entry((node, definition, hash))
                .or_default();
            *count = count.saturating_add(1);
        }
    }
    pub(crate) fn consume_connector_prompt(
        &mut self,
        node: NodeId,
        definition: i64,
        hash: Option<[u8; 32]>,
    ) -> bool {
        let Some(hash) = hash else {
            return false;
        };
        let key = (node, definition, hash);
        let Some(count) = self.connector_prompts.get_mut(&key) else {
            return false;
        };
        *count -= 1;
        if *count == 0 {
            self.connector_prompts.remove(&key);
        }
        true
    }

    pub(crate) fn has_pending_for_node(&self, node_id: NodeId) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.receipt.node_id == node_id)
    }

    pub(crate) fn interrupt_all(&mut self) -> Vec<AgentReplyReceipt> {
        self.connector_reply_ids.clear();
        self.connector_prompts.clear();
        let mut interrupted = Vec::new();
        while let Some(mut entry) = self.entries.pop_front() {
            entry.receipt.status = AgentReplyStatus::Interrupted;
            interrupted.push(entry.receipt);
        }
        interrupted
    }

    pub(crate) fn peek_reply(&self, reply_id: u64) -> Option<&AgentReplyReceipt> {
        self.entries
            .iter()
            .find(|entry| entry.receipt.reply_id == reply_id)
            .map(|entry| &entry.receipt)
    }

    pub(crate) fn cancel_reply_id(&mut self, reply_id: u64) -> Option<AgentReplyReceipt> {
        if let Some(index) = self
            .entries
            .iter()
            .position(|entry| entry.receipt.reply_id == reply_id)
        {
            let entry = self.entries.get(index).expect("index just found");
            if matches!(entry.stage, ReplyStage::SubmitEnter) {
                return None;
            }
            self.connector_reply_ids.remove(&reply_id);
            let mut entry = self.entries.remove(index).expect("index just found");
            entry.receipt.status = AgentReplyStatus::Cancelled;
            return Some(entry.receipt);
        }
        None
    }

    pub(crate) fn cancel_agent(&mut self, agent: &str) -> Vec<AgentReplyReceipt> {
        let mut cancelled = Vec::new();
        let mut kept = VecDeque::new();
        while let Some(mut entry) = self.entries.pop_front() {
            let same_agent = entry.receipt.name.eq_ignore_ascii_case(agent.trim());
            if same_agent && !matches!(entry.stage, ReplyStage::SubmitEnter) {
                self.connector_reply_ids.remove(&entry.receipt.reply_id);
                entry.receipt.status = AgentReplyStatus::Cancelled;
                cancelled.push(entry.receipt);
            } else {
                kept.push_back(entry);
            }
        }
        self.entries = kept;
        cancelled
    }

    fn allocate_id(&mut self) -> u64 {
        self.next_reply_id = self.next_reply_id.saturating_add(1).max(1);
        self.next_reply_id
    }
}

impl MultisoulApp {
    pub(crate) fn enqueue_or_send_session_reply(
        &mut self,
        definition_id: i64,
        name: String,
        node_id: NodeId,
        session_id: String,
        message: String,
    ) -> Result<AgentReplyReceipt, String> {
        let reply_id = self.session_reply_queue.allocate_id();
        let mut receipt = AgentReplyReceipt {
            reply_id,
            definition_id,
            name,
            node_id,
            session_id,
            message,
            status: AgentReplyStatus::Queued,
        };

        if session_reply_input_ready(self.runtime.agent_task_state(node_id).as_ref())
            && !self.runtime.agent_task_input_held(node_id)
            && !self.pending_agent_submissions.input_in_progress(node_id)
            && !self.session_reply_queue.has_pending_for_node(node_id)
        {
            type_agent_task(&mut self.runtime, node_id, &receipt.message)
                .map_err(|error| error.to_string())?;
            match submit_agent_enter(&mut self.runtime, node_id) {
                Ok(()) => {
                    receipt.status = AgentReplyStatus::Sent;
                    Ok(receipt)
                }
                Err(error) => {
                    let _ = clear_agent_task_input(&mut self.runtime, node_id);
                    Err(error.to_string())
                }
            }
        } else {
            self.session_reply_queue
                .entries
                .push_back(QueuedSessionReply {
                    receipt: receipt.clone(),
                    stage: ReplyStage::WaitingForInputReady,
                    ready_after: Instant::now(),
                });
            Ok(receipt)
        }
    }

    pub(crate) fn advance_session_reply_queue(&mut self) {
        let now = Instant::now();
        let mut index = 0;
        let mut visited = HashSet::new();
        while index < self.session_reply_queue.entries.len() {
            let Some(entry) = self.session_reply_queue.entries.get(index).cloned() else {
                break;
            };
            if !visited.insert(entry.receipt.node_id)
                || self
                    .pending_agent_submissions
                    .input_in_progress(entry.receipt.node_id)
            {
                index += 1;
                continue;
            }
            if entry.ready_after > now {
                index += 1;
                continue;
            }
            if self.runtime.agent_task_input_held(entry.receipt.node_id)
                || !session_reply_input_ready(
                    self.runtime
                        .agent_task_state(entry.receipt.node_id)
                        .as_ref(),
                )
            {
                index += 1;
                continue;
            }
            match entry.stage {
                ReplyStage::WaitingForInputReady => {
                    if !self.session_still_matches(&entry.receipt) {
                        self.fail_reply_at(index);
                        continue;
                    }
                    match type_agent_task(
                        &mut self.runtime,
                        entry.receipt.node_id,
                        &entry.receipt.message,
                    ) {
                        Ok(()) => {
                            if let Some(slot) = self.session_reply_queue.entries.get_mut(index) {
                                slot.stage = ReplyStage::SubmitEnter;
                                slot.ready_after = Instant::now() + AGENT_REPLY_ENTER_DELAY;
                            }
                            index += 1;
                        }
                        Err(_) => self.fail_reply_at(index),
                    }
                }
                ReplyStage::SubmitEnter => {
                    if !self.session_still_matches(&entry.receipt) {
                        let _ = clear_agent_task_input(&mut self.runtime, entry.receipt.node_id);
                        self.fail_reply_at(index);
                        continue;
                    }
                    match submit_agent_enter(&mut self.runtime, entry.receipt.node_id) {
                        Ok(()) => {
                            if self
                                .session_reply_queue
                                .connector_reply_ids
                                .remove(&entry.receipt.reply_id)
                            {
                                self.session_reply_queue.record_connector_prompt(
                                    entry.receipt.node_id,
                                    entry.receipt.definition_id,
                                    &entry.receipt.message,
                                );
                            }
                            self.session_reply_queue.entries.remove(index);
                        }
                        Err(_) => {
                            let _ =
                                clear_agent_task_input(&mut self.runtime, entry.receipt.node_id);
                            self.fail_reply_at(index);
                        }
                    }
                }
            }
        }
    }

    fn fail_reply_at(&mut self, index: usize) {
        if let Some(mut entry) = self.session_reply_queue.entries.remove(index) {
            self.session_reply_queue
                .connector_reply_ids
                .remove(&entry.receipt.reply_id);
            entry.receipt.status = AgentReplyStatus::Failed;
        }
    }

    fn session_still_matches(&self, receipt: &AgentReplyReceipt) -> bool {
        let Some(node) = self
            .runtime
            .document
            .nodes()
            .iter()
            .find(|node| node.id == receipt.node_id)
        else {
            return false;
        };
        let CanvasNodeKind::Agent(spec) = &node.kind else {
            return false;
        };
        if spec.definition_id != Some(receipt.definition_id)
            || !self
                .agent_definitions
                .definitions()
                .iter()
                .any(|definition| definition.id == receipt.definition_id && definition.enabled)
        {
            return false;
        }
        // Empty receipt session = card-bound converse before Hook SessionStart.
        if receipt.session_id.is_empty() {
            return true;
        }
        match self.runtime.agent_session_id(receipt.node_id) {
            None => true,
            Some(current) => current == receipt.session_id,
        }
    }
}

fn session_reply_input_ready(state: Option<&AgentTaskState>) -> bool {
    match state {
        None => true,
        Some(
            AgentTaskState::Ready
            | AgentTaskState::Done
            | AgentTaskState::Failed { .. }
            | AgentTaskState::Thinking
            | AgentTaskState::RunningTool { .. },
        ) => true,
        Some(AgentTaskState::WaitingApproval { .. } | AgentTaskState::Ended) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent::AgentReplyStatus;

    fn receipt(reply_id: u64, name: &str) -> AgentReplyReceipt {
        AgentReplyReceipt {
            reply_id,
            definition_id: 1,
            name: name.to_string(),
            node_id: 11,
            session_id: "s1".to_string(),
            message: "按照 A 方案执行".to_string(),
            status: AgentReplyStatus::Queued,
        }
    }

    /// 排队中的回复可按 reply_id 撤回；进入 SubmitEnter 后不可撤。
    ///
    /// 数据构造：queue 含 WaitingForInputReady(#1) 与 SubmitEnter(#2)。
    /// 执行：cancel_reply_id(1) 成功；cancel_reply_id(2) 返回 None。
    /// 预期：#1 status=Cancelled；#2 仍留在队列。
    #[test]
    fn cancel_reply_id_only_removes_waiting_entries() {
        let mut queue = AgentSessionReplyQueue::default();
        queue.entries.push_back(QueuedSessionReply {
            receipt: receipt(1, "Alan"),
            stage: ReplyStage::WaitingForInputReady,
            ready_after: Instant::now(),
        });
        queue.entries.push_back(QueuedSessionReply {
            receipt: receipt(2, "Alan"),
            stage: ReplyStage::SubmitEnter,
            ready_after: Instant::now(),
        });

        let cancelled = queue
            .cancel_reply_id(1)
            .expect("WaitingForInputReady 应可撤回");
        assert_eq!(
            cancelled.status,
            AgentReplyStatus::Cancelled,
            "撤回回执应为 Cancelled"
        );
        assert!(
            queue.cancel_reply_id(2).is_none(),
            "SubmitEnter 阶段不得撤回"
        );
        assert_eq!(
            queue.entries.len(),
            1,
            "仅应移除 Waiting 条目，实际 len={}",
            queue.entries.len()
        );
        assert_eq!(
            queue.entries[0].receipt.reply_id, 2,
            "SubmitEnter 条目应仍留在队列"
        );
    }

    /// 运行中的会话可接收消息，审批和已结束状态仍禁止输入。
    ///
    /// 数据构造：无状态、Ready、Thinking、RunningTool、WaitingApproval、Ended。
    /// 执行过程：逐一判断会话是否允许写入正文与 Enter。
    /// 预期结果：Thinking/RunningTool 交给 CLI 接收；审批与结束状态不得写入。
    #[test]
    fn input_ready_accepts_running_but_excludes_approval_and_ended_states() {
        assert!(session_reply_input_ready(None), "无状态应视为可输入");
        assert!(
            session_reply_input_ready(Some(&AgentTaskState::Ready)),
            "Ready 应可输入"
        );
        assert!(
            session_reply_input_ready(Some(&AgentTaskState::Thinking)),
            "Thinking 应直接发送，由 Agent CLI 管理消息队列"
        );
        assert!(
            session_reply_input_ready(Some(&AgentTaskState::RunningTool {
                tool_name: Some("Bash".to_string()),
            })),
            "RunningTool 应直接发送，不等待当前工具结束"
        );
        assert!(
            !session_reply_input_ready(Some(&AgentTaskState::WaitingApproval {
                tool_name: Some("Bash".to_string()),
            })),
            "审批期间不得把消息和 Enter 当作批准输入"
        );
        assert!(
            !session_reply_input_ready(Some(&AgentTaskState::Ended)),
            "Ended 不得立即发送"
        );
    }
}
