//! Runtime event queue and live node run-state registry.

use canvas::{AgentProfile, CanvasNodeKind, NodeId, NodeType};
use std::collections::{HashMap, HashSet, VecDeque};
use terminal::TerminalStatus;

use crate::runtime::Runtime;
use crate::EgoTaskState;

pub const MAX_PENDING_EVENTS: usize = 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeRunState {
    Initializing,
    Running,
    Finished { code: Option<i32> },
    Error { message: String },
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentTaskState {
    Ready,
    Thinking,
    RunningTool { tool_name: Option<String> },
    WaitingApproval { tool_name: Option<String> },
    Done,
    Failed { message: String },
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardAttentionKind {
    AgentTaskDone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardAttentionState {
    pub kind: CardAttentionKind,
    pub viewed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTaskUpdate {
    pub node_id: NodeId,
    pub profile: AgentProfile,
    pub provider_event: String,
    pub to: AgentTaskState,
    /// Optional display title for this task. Locked on first non-None value;
    /// subsequent updates with a value are ignored.
    pub task_title: Option<String>,
    /// Optional provider session id from agent hook payloads.
    pub session_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AgentTaskUpdateOutcome {
    pub task_state_changed: bool,
    pub session_binding_changed: bool,
}

impl AgentTaskUpdateOutcome {
    pub fn changed(self) -> bool {
        self.task_state_changed || self.session_binding_changed
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeEvent {
    NodeRegistered {
        node_id: NodeId,
        node_type: NodeType,
        state: NodeRunState,
    },
    NodeRunStateChanged {
        node_id: NodeId,
        from: NodeRunState,
        to: NodeRunState,
    },
    NodeRemoved {
        node_id: NodeId,
        previous_state: NodeRunState,
    },
    NodeOperationFailed {
        node_id: Option<NodeId>,
        operation: String,
        message: String,
    },
    AgentTaskStateChanged {
        node_id: NodeId,
        from: AgentTaskState,
        to: AgentTaskState,
        source_event: String,
    },
    EgoTaskStateChanged {
        node_id: NodeId,
        from: Option<EgoTaskState>,
        to: EgoTaskState,
        operation: String,
    },
    EventsDropped {
        count: usize,
    },
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeEvents {
    states: HashMap<NodeId, NodeRunState>,
    agent_task_states: HashMap<NodeId, AgentTaskState>,
    agent_task_titles: HashMap<NodeId, String>,
    /// CLI 自己生成的会话名（Claude ai-title、Codex thread_name）；优先于首条提示词作为标题。
    agent_session_titles: HashMap<NodeId, String>,
    /// 已按绑定会话读过会话名的节点；会话名被清掉（重启会话）时同步清除，允许重读。
    session_title_probed: HashSet<NodeId>,
    agent_latest_prompts: HashMap<NodeId, String>,
    card_attention: HashMap<NodeId, CardAttentionState>,
    queue: VecDeque<RuntimeEvent>,
}

impl RuntimeEvents {
    pub(crate) fn state(&self, node_id: NodeId) -> Option<NodeRunState> {
        self.states.get(&node_id).cloned()
    }

    pub(crate) fn agent_task_state(&self, node_id: NodeId) -> Option<AgentTaskState> {
        self.agent_task_states.get(&node_id).cloned()
    }

    pub(crate) fn agent_task_title(&self, node_id: NodeId) -> Option<String> {
        self.agent_session_titles
            .get(&node_id)
            .or_else(|| self.agent_task_titles.get(&node_id))
            .cloned()
    }

    /// 返回标题是否变化；调用方据此决定是否刷新界面。
    pub(crate) fn set_agent_session_title(&mut self, node_id: NodeId, title: String) -> bool {
        self.agent_session_titles.insert(node_id, title.clone()) != Some(title)
    }

    fn forget_session_title(&mut self, node_id: NodeId) {
        self.agent_session_titles.remove(&node_id);
        self.session_title_probed.remove(&node_id);
    }

    /// 首次对该节点探测时返回 true；会话名被清掉后可再次探测。
    pub(crate) fn claim_session_title_probe(&mut self, node_id: NodeId) -> bool {
        self.session_title_probed.insert(node_id)
    }

    pub(crate) fn has_agent_session_title(&self, node_id: NodeId) -> bool {
        self.agent_session_titles.contains_key(&node_id)
    }

    pub(crate) fn set_agent_task_title_if_absent(&mut self, node_id: NodeId, title: String) {
        self.agent_task_titles.entry(node_id).or_insert(title);
    }

    pub(crate) fn card_attention(&self, node_id: NodeId) -> Option<CardAttentionState> {
        self.card_attention.get(&node_id).copied()
    }

    pub(crate) fn pending_len(&self) -> usize {
        self.queue.len()
    }

    pub(crate) fn take(&mut self) -> Vec<RuntimeEvent> {
        self.queue.drain(..).collect()
    }

    pub(crate) fn register(&mut self, node_id: NodeId, node_type: NodeType) {
        if node_type == NodeType::Agent {
            self.agent_task_states.remove(&node_id);
            self.agent_task_titles.remove(&node_id);
            self.forget_session_title(node_id);
            self.agent_latest_prompts.remove(&node_id);
            self.card_attention.remove(&node_id);
        }
        let state = NodeRunState::Running;
        self.states.insert(node_id, state.clone());
        self.push(RuntimeEvent::NodeRegistered {
            node_id,
            node_type,
            state,
        });
    }

    pub(crate) fn update_from_terminal_status(&mut self, node_id: NodeId, status: TerminalStatus) {
        let to = node_run_state_from_terminal_status(status);
        let Some(from) = self.states.get(&node_id).cloned() else {
            return;
        };
        if from == to {
            return;
        }
        self.states.insert(node_id, to.clone());
        self.push(RuntimeEvent::NodeRunStateChanged { node_id, from, to });
    }

    pub(crate) fn remove(&mut self, node_id: NodeId) {
        self.agent_task_states.remove(&node_id);
        self.agent_task_titles.remove(&node_id);
        self.forget_session_title(node_id);
        self.agent_latest_prompts.remove(&node_id);
        self.card_attention.remove(&node_id);
        if let Some(previous_state) = self.states.remove(&node_id) {
            self.push(RuntimeEvent::NodeRemoved {
                node_id,
                previous_state,
            });
        }
    }

    pub(crate) fn remove_all(&mut self) {
        self.agent_task_states.clear();
        self.agent_task_titles.clear();
        self.agent_session_titles.clear();
        self.session_title_probed.clear();
        self.agent_latest_prompts.clear();
        self.card_attention.clear();
        let removed: Vec<_> = self.states.drain().collect();
        for (node_id, previous_state) in removed {
            self.push(RuntimeEvent::NodeRemoved {
                node_id,
                previous_state,
            });
        }
    }

    pub(crate) fn operation_failed(
        &mut self,
        node_id: Option<NodeId>,
        operation: &str,
        message: String,
    ) {
        self.push(RuntimeEvent::NodeOperationFailed {
            node_id,
            operation: operation.to_string(),
            message,
        });
    }

    pub(crate) fn ego_task_state_changed(
        &mut self,
        node_id: NodeId,
        from: Option<EgoTaskState>,
        to: EgoTaskState,
        operation: &str,
    ) {
        self.push(RuntimeEvent::EgoTaskStateChanged {
            node_id,
            from,
            to,
            operation: operation.to_string(),
        });
    }

    pub(crate) fn update_agent_task_state(
        &mut self,
        node_id: NodeId,
        to: AgentTaskState,
        source_event: String,
    ) -> bool {
        let previous = self.agent_task_states.get(&node_id).cloned();
        if previous == Some(AgentTaskState::Ended)
            && is_active_agent_task_state(&to)
            && !matches!(source_event.as_str(), "UserPromptSubmit" | "turn/started")
        {
            return false;
        }
        if previous.as_ref() == Some(&to) {
            return false;
        }
        // Clear task title when a new session starts (Ready = SessionStart).
        // This allows the next UserPromptSubmit to write a fresh title.
        if to == AgentTaskState::Ready {
            self.agent_task_titles.remove(&node_id);
            self.forget_session_title(node_id);
            self.agent_latest_prompts.remove(&node_id);
        }
        let from = previous.unwrap_or(AgentTaskState::Ready);
        self.update_attention_for_agent_task(node_id, &to);
        self.agent_task_states.insert(node_id, to.clone());
        self.push(RuntimeEvent::AgentTaskStateChanged {
            node_id,
            from,
            to,
            source_event,
        });
        true
    }

    pub(crate) fn mark_card_viewed(&mut self, node_id: NodeId) -> bool {
        let Some(attention) = self.card_attention.get_mut(&node_id) else {
            return false;
        };
        if attention.viewed {
            return false;
        }
        attention.viewed = true;
        true
    }

    fn push(&mut self, event: RuntimeEvent) {
        if self.queue.len() >= MAX_PENDING_EVENTS {
            self.queue.pop_front();
            self.queue
                .push_back(RuntimeEvent::EventsDropped { count: 1 });
            return;
        }
        self.queue.push_back(event);
    }

    fn update_attention_for_agent_task(&mut self, node_id: NodeId, to: &AgentTaskState) {
        match to {
            AgentTaskState::Done => {
                self.card_attention.insert(
                    node_id,
                    CardAttentionState {
                        kind: CardAttentionKind::AgentTaskDone,
                        viewed: false,
                    },
                );
            }
            AgentTaskState::Ready
            | AgentTaskState::Thinking
            | AgentTaskState::RunningTool { .. }
            | AgentTaskState::WaitingApproval { .. }
            | AgentTaskState::Failed { .. }
            | AgentTaskState::Ended => {
                self.card_attention.remove(&node_id);
            }
        }
    }
}

fn is_active_agent_task_state(state: &AgentTaskState) -> bool {
    matches!(
        state,
        AgentTaskState::Thinking
            | AgentTaskState::RunningTool { .. }
            | AgentTaskState::WaitingApproval { .. }
    )
}

impl Runtime {
    pub fn node_run_state(&self, node_id: NodeId) -> Option<NodeRunState> {
        self.events.state(node_id)
    }

    pub fn take_events(&mut self) -> Vec<RuntimeEvent> {
        self.events.take()
    }

    pub fn pending_event_len(&self) -> usize {
        self.events.pending_len()
    }

    pub fn agent_task_state(&self, node_id: NodeId) -> Option<AgentTaskState> {
        self.events.agent_task_state(node_id)
    }

    pub fn agent_task_title(&self, node_id: NodeId) -> Option<String> {
        self.events.agent_task_title(node_id)
    }

    /// 记录 CLI 的会话名；标题没变时返回 false。
    pub fn set_agent_session_title(&mut self, node_id: NodeId, title: String) -> bool {
        self.events.set_agent_session_title(node_id, title)
    }

    pub fn has_agent_session_title(&self, node_id: NodeId) -> bool {
        self.events.has_agent_session_title(node_id)
    }

    pub fn agent_latest_prompt(&self, node_id: NodeId) -> Option<String> {
        self.events.agent_latest_prompts.get(&node_id).cloned()
    }

    pub fn card_attention(&self, node_id: NodeId) -> Option<CardAttentionState> {
        self.events.card_attention(node_id)
    }

    pub fn mark_card_viewed(&mut self, node_id: NodeId) -> bool {
        if self.document.node(node_id).is_none() {
            return false;
        }
        self.events.mark_card_viewed(node_id)
    }

    pub fn apply_agent_task_update(&mut self, update: AgentTaskUpdate) -> bool {
        self.apply_agent_task_update_with_outcome(update).changed()
    }

    pub fn apply_agent_task_update_with_outcome(
        &mut self,
        update: AgentTaskUpdate,
    ) -> AgentTaskUpdateOutcome {
        let Some(node) = self.document.node(update.node_id) else {
            tracing::debug!(
                node_id = update.node_id,
                "agent task update ignored: missing node"
            );
            return AgentTaskUpdateOutcome::default();
        };
        match &node.kind {
            CanvasNodeKind::Agent(agent) if agent.profile == update.profile => {
                if agent.mobile_control
                    && (update.session_id.is_none()
                        || self.agent_session_id(update.node_id).as_deref()
                            != update.session_id.as_deref())
                {
                    return AgentTaskUpdateOutcome::default();
                }
                // Empty controlled sessions may have no Provider history yet. Persist only
                // after the first real prompt, so reopen never resumes a fabricated history.
                let persist_session =
                    !agent.mobile_control || update.provider_event == "UserPromptSubmit";
                let cancel_permissions = agent.mobile_control
                    && matches!(
                        update.provider_event.as_str(),
                        "UserPromptSubmit" | "Stop" | "StopFailure" | "SessionEnd"
                    );
                let session_binding_changed = persist_session
                    && update
                        .session_id
                        .as_deref()
                        .filter(|session_id| !session_id.is_empty())
                        .is_some_and(|session_id| {
                            self.agent_session_bindings.bind_if_unclaimed(
                                update.node_id,
                                update.profile,
                                session_id,
                            )
                        });
                if cancel_permissions {
                    self.cancel_mobile_approvals(update.node_id);
                }
                if let Some(title) = update.task_title {
                    let latest = &mut self.events.agent_latest_prompts;
                    latest.insert(update.node_id, title.clone());
                    self.events
                        .set_agent_task_title_if_absent(update.node_id, title);
                }
                let task_state_changed = self.events.update_agent_task_state(
                    update.node_id,
                    update.to,
                    update.provider_event,
                );
                AgentTaskUpdateOutcome {
                    task_state_changed,
                    session_binding_changed,
                }
            }
            CanvasNodeKind::Agent(agent) => {
                tracing::debug!(
                    node_id = update.node_id,
                    expected = ?agent.profile,
                    actual = ?update.profile,
                    "agent task update ignored: profile mismatch"
                );
                AgentTaskUpdateOutcome::default()
            }
            CanvasNodeKind::Rect(_)
            | CanvasNodeKind::Term(_)
            | CanvasNodeKind::Preview(_)
            | CanvasNodeKind::Browser(_) => {
                tracing::debug!(
                    node_id = update.node_id,
                    "agent task update ignored: target is not an agent node"
                );
                AgentTaskUpdateOutcome::default()
            }
        }
    }
}

fn node_run_state_from_terminal_status(status: TerminalStatus) -> NodeRunState {
    match status {
        TerminalStatus::Running => NodeRunState::Running,
        TerminalStatus::Exited { code: Some(0) } => NodeRunState::Finished { code: Some(0) },
        TerminalStatus::Exited { code: Some(code) } => NodeRunState::Error {
            message: format!("process exited with status {code}"),
        },
        TerminalStatus::Exited { code: None } => NodeRunState::Finished { code: None },
    }
}

#[cfg(test)]
#[path = "../tests/unit/events_queue_tests.rs"]
mod tests;
