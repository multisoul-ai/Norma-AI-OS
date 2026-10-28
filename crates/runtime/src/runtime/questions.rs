//! Live questions are independent of ordinary (possibly late) task-state events.
use super::Runtime;
use canvas::{CanvasNodeKind, NodeId};
use norma::protocol::{
    PendingQuestion, QuestionAnswer, QuestionRef, QuestionSnapshot, QuestionState,
};
use std::collections::HashMap;
use terminal::TerminalStatus;
mod claude;
mod claude_receipts;
mod codex_host;
mod cursor_host;
mod delivery;
mod fallback;
mod history;
mod lifecycle;
mod opencode_host;

#[derive(Default)]
pub(super) struct Questions {
    fallback: fallback::FallbackQuestions,
    native_hosts: HashMap<NodeId, codex_host::OwnedHost>,
    opencode_hosts: HashMap<NodeId, opencode_host::OwnedHost>,
    cursor_hosts: HashMap<NodeId, cursor_host::OwnedHost>,
    native_session_changed: bool,
    pub(super) processes: HashMap<NodeId, String>,
    retired_sessions: HashMap<NodeId, std::collections::HashSet<String>>,
    ended_sessions: HashMap<NodeId, String>,
    resume_expected: HashMap<NodeId, std::time::Instant>,
    early_native_results:
        std::collections::VecDeque<(std::time::Instant, norma::agent_hook::AgentHookEvent)>,
    connections: HashMap<NodeId, QuestionRef>,
    entries: Vec<Entry>,
    history: history::QuestionHistory,
    provider_states: HashMap<NodeId, crate::AgentTaskState>,
}

/// The App supplies a synchronous, in-memory context gate; it stays locked through one PTY step.
/// It may deny existing authorization, never create one. No IO belongs in this callback.
pub type QuestionTerminalGuard = std::sync::Arc<
    dyn Fn(&mut dyn FnMut() -> Result<bool, &'static str>) -> Result<bool, &'static str>
        + Send
        + Sync,
>;

struct Entry {
    snapshot: QuestionSnapshot,
    submission: Option<QuestionSubmission>,
    hook: Option<norma::agent_hook::HookQuestion>,
    terminal_guard: Option<QuestionTerminalGuard>,
    receipt_pending: Option<String>,
}

impl Entry {
    fn public_snapshot(&self) -> QuestionSnapshot {
        let mut snapshot = self.snapshot.clone();
        // Internally Submitted reserves the send before IO; externally report actual transport.
        if snapshot.state == QuestionState::Submitted {
            if let Some(hook) = &self.hook {
                use norma::agent_hook::QuestionHookStatus;
                match hook.status(std::time::Instant::now()) {
                    _ if hook.transport_sent() => {}
                    _ if hook.native_send_status()
                        == norma::agent_hook::NativeSendStatus::Unknown =>
                    {
                        snapshot.state = QuestionState::Unknown;
                        snapshot
                            .error
                            .get_or_insert_with(|| "question_transport_closed".into());
                    }
                    QuestionHookStatus::Cancelled if hook.send_rejection().is_some() => {
                        snapshot.state = QuestionState::Failed;
                        snapshot.error = hook.send_rejection().map(str::to_owned);
                    }
                    QuestionHookStatus::Cancelled => {
                        snapshot.state = QuestionState::Unknown;
                        snapshot
                            .error
                            .get_or_insert_with(|| "question_transport_closed".into());
                    }
                    _ => snapshot.state = QuestionState::Submitting,
                }
            }
        }
        snapshot
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        if let Some(hook) = &self.hook {
            hook.cancel();
        }
    }
}

/// Host-only send qualification. Not deserializable from a CLI/model supplied origin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QuestionSubmission {
    target: QuestionRef,
    delivery_id: String,
    answers: Vec<QuestionAnswer>,
}

impl QuestionSubmission {
    pub fn target(&self) -> &QuestionRef {
        &self.target
    }
    pub fn delivery_id(&self) -> &str {
        &self.delivery_id
    }
    pub fn answers(&self) -> &[QuestionAnswer] {
        &self.answers
    }
}

fn same_connection(a: &QuestionRef, b: &QuestionRef) -> bool {
    a.workspace_id == b.workspace_id
        && a.project_id == b.project_id
        && a.node_id == b.node_id
        && a.provider == b.provider
        && a.run_id == b.run_id
        && a.connection_id == b.connection_id
        && a.session_id == b.session_id
}

fn same_request(a: &QuestionRef, b: &QuestionRef) -> bool {
    a.workspace_id == b.workspace_id
        && a.project_id == b.project_id
        && a.node_id == b.node_id
        && a.provider == b.provider
        && (a.run_id == b.run_id || a.provider == norma::protocol::AgentProfile::OpenCode)
        && a.session_id == b.session_id
        && a.turn_id == b.turn_id
        && a.request_id == b.request_id
}

impl Runtime {
    fn validate_question_run(&self, target: &QuestionRef) -> Result<(), &'static str> {
        target.validate()?;
        let Some(CanvasNodeKind::Agent(spec)) = self.document.node(target.node_id).map(|n| &n.kind)
        else {
            return Err("question_node_missing");
        };
        if spec.profile.display_title() != format!("{:?}", target.provider)
            || !self
                .live
                .get(&target.node_id)
                .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
            || self
                .agent_hook_config
                .as_ref()
                .is_none_or(|config| config.workspace_id != target.workspace_id)
            || self.mobile_agent_run_id(target.node_id) != Some(target.run_id.as_str())
            || self.agent_session_id(target.node_id).as_deref() != Some(target.session_id.as_str())
            || self
                .questions
                .retired_sessions
                .get(&target.node_id)
                .is_some_and(|sessions| sessions.contains(&target.session_id))
        {
            return Err("question_session_changed");
        }
        Ok(())
    }

    /// Called by the owning host when it attaches a live provider transport, never from question JSON.
    pub fn bind_question_connection(&mut self, target: &QuestionRef) -> Result<(), &'static str> {
        self.validate_question_run(target)?;
        if self
            .questions
            .connections
            .get(&target.node_id)
            .is_some_and(|old| same_connection(old, target))
        {
            return Ok(());
        }
        for entry in &mut self.questions.entries {
            if entry.snapshot.question.target.node_id == target.node_id
                && entry.snapshot.state.blocks_input()
            {
                if let Some(hook) = &entry.hook {
                    hook.cancel();
                }
                entry.snapshot.state = if matches!(
                    entry.snapshot.state,
                    QuestionState::Submitted | QuestionState::Unknown
                ) {
                    QuestionState::Unknown
                } else {
                    QuestionState::Expired
                };
            }
        }
        let replaced = self
            .questions
            .connections
            .insert(target.node_id, target.clone())
            .is_some();
        if replaced {
            self.refresh_question_projection(target.node_id);
        }
        Ok(())
    }

    /// Historical receipts remain queryable, but only the owning live connection can act on a question.
    pub fn question_is_current(&self, target: &QuestionRef) -> bool {
        self.validate_question_connection(target).is_ok()
    }

    fn validate_question_connection(&self, target: &QuestionRef) -> Result<(), &'static str> {
        self.validate_question_run(target)?;
        if !self.opencode_connection_current(target) {
            return Err("question_foreground_changed");
        }
        if !self
            .questions
            .connections
            .get(&target.node_id)
            .is_some_and(|binding| same_connection(binding, target))
        {
            return Err("question_connection_changed");
        }
        Ok(())
    }

    pub fn register_question(&mut self, question: PendingQuestion) -> Result<(), &'static str> {
        question.validate()?;
        self.validate_question_connection(&question.target)?;
        if let Some(existing) = self.question(&question.target) {
            return if existing.question == question {
                Ok(())
            } else {
                Err("question_revision_conflict")
            };
        }
        self.questions.history.check(&question.target)?;
        for entry in &self.questions.entries {
            let old = &entry.snapshot;
            if !same_request(&old.question.target, &question.target) {
                continue;
            }
            if matches!(old.state, QuestionState::Submitted | QuestionState::Unknown) {
                return Err("question_reconciliation_required");
            }
            if matches!(
                old.state,
                QuestionState::Answered
                    | QuestionState::AnsweredElsewhere
                    | QuestionState::ConsumedUnattributed
            ) {
                return Err("question_already_answered");
            }
            if same_connection(&old.question.target, &question.target)
                && old.question.target.revision >= question.target.revision
            {
                return Err("question_revision_stale");
            }
        }
        self.questions.compact_history();
        if self
            .questions
            .entries
            .iter()
            .filter(|entry| entry.snapshot.state.blocks_input())
            .count()
            >= 256
        {
            return Err("question_capacity");
        }
        for entry in &mut self.questions.entries {
            if same_request(&entry.snapshot.question.target, &question.target)
                && entry.snapshot.state.blocks_input()
            {
                entry.snapshot.state = QuestionState::Expired;
            }
        }
        let node_id = question.target.node_id;
        self.questions
            .provider_states
            .entry(node_id)
            .or_insert_with(|| {
                self.events
                    .agent_task_state(node_id)
                    .unwrap_or(crate::AgentTaskState::Ready)
            });
        self.questions.entries.push(Entry {
            snapshot: QuestionSnapshot {
                question,
                state: QuestionState::Pending,
                delivery_id: None,
                error: None,
                evidence: None,
            },
            submission: None,
            hook: None,
            terminal_guard: None,
            receipt_pending: None,
        });
        self.events.update_agent_task_state(
            node_id,
            crate::AgentTaskState::WaitingQuestion,
            "question/asked".into(),
        );
        Ok(())
    }

    pub fn has_pending_question(&self, node: NodeId) -> bool {
        self.native_input_pending(node)
            || self.opencode_input_pending(node)
            || self.questions.fallback.blocks(node)
            || self.questions.entries.iter().any(|entry| {
                entry.snapshot.question.target.node_id == node
                    && entry.snapshot.state.blocks_input()
                    && self
                        .validate_question_run(&entry.snapshot.question.target)
                        .is_ok()
            })
    }

    pub(crate) fn question_accepts_task_session(
        &self,
        node: NodeId,
        session: Option<&str>,
    ) -> bool {
        !self.has_pending_question(node)
            || session.is_none()
            || self.agent_session_id(node).as_deref() == session
    }

    pub(super) fn remove_node_questions(&mut self, node: NodeId) {
        self.questions.native_hosts.remove(&node);
        self.questions.opencode_hosts.remove(&node);
        self.questions.cursor_hosts.remove(&node);
        self.questions.fallback.remove_node(node);
        self.questions.processes.remove(&node);
        self.questions.retired_sessions.remove(&node);
        self.questions.ended_sessions.remove(&node);
        self.questions.resume_expected.remove(&node);
        self.questions
            .early_native_results
            .retain(|(_, event)| event.node_id != node);
        self.questions.connections.remove(&node);
        self.questions.provider_states.remove(&node);
        self.questions.history.remove_node(node);
        self.questions
            .entries
            .retain(|entry| entry.snapshot.question.target.node_id != node);
    }

    pub(crate) fn project_question_task_state(
        &mut self,
        node: NodeId,
        to: crate::AgentTaskState,
    ) -> crate::AgentTaskState {
        if to != crate::AgentTaskState::WaitingQuestion {
            self.questions.provider_states.insert(node, to.clone());
        }
        if self.has_pending_question(node) {
            crate::AgentTaskState::WaitingQuestion
        } else {
            to
        }
    }

    pub fn questions(&self) -> Vec<QuestionSnapshot> {
        self.questions
            .entries
            .iter()
            .map(Entry::public_snapshot)
            .collect()
    }

    pub fn question_snapshot(&self, target: &QuestionRef) -> Option<QuestionSnapshot> {
        self.questions
            .entries
            .iter()
            .find(|entry| entry.snapshot.question.target == *target)
            .map(Entry::public_snapshot)
    }

    pub fn question(&self, target: &QuestionRef) -> Option<&QuestionSnapshot> {
        self.questions
            .entries
            .iter()
            .find(|entry| entry.snapshot.question.target == *target)
            .map(|entry| &entry.snapshot)
    }

    /// Must follow host authorization. Claim alone never writes Provider input or implies consumption.
    pub fn claim_question_answer(
        &mut self,
        target: &QuestionRef,
        answers: Vec<QuestionAnswer>,
        delivery_id: &str,
    ) -> Result<Option<QuestionSubmission>, &'static str> {
        self.validate_question_connection(target)?;
        if delivery_id.trim().is_empty() {
            return Err("invalid_delivery_id");
        }
        let entry = self
            .questions
            .entries
            .iter_mut()
            .find(|entry| entry.snapshot.question.target == *target)
            .ok_or("question_not_found")?;
        entry.snapshot.question.validate_answers(&answers)?;
        if !matches!(
            entry.snapshot.state,
            QuestionState::Pending | QuestionState::Failed
        ) {
            return Ok(None);
        }
        if entry.hook.as_ref().is_some_and(|hook| {
            hook.status(std::time::Instant::now()) != norma::agent_hook::QuestionHookStatus::Pending
        }) {
            return Err("question_hook_closed");
        }
        let submission = QuestionSubmission {
            target: target.clone(),
            delivery_id: delivery_id.into(),
            answers,
        };
        entry.snapshot.state = QuestionState::Submitting;
        entry.snapshot.delivery_id = Some(delivery_id.into());
        entry.submission = Some(submission.clone());
        entry.receipt_pending = Some(delivery_id.into());
        Ok(Some(submission))
    }
}
