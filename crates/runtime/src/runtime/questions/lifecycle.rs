//! Process identity remains stable across native /new; question runs belong to one session.
use super::*;
use norma::agent_hook::AgentHookEvent;

impl Runtime {
    pub(in crate::runtime) fn prepare_question_restore(&mut self) -> Vec<NodeId> {
        let nodes: Vec<_> = self
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                let CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                (!spec.mobile_control
                    && !self.live.contains_key(&node.id)
                    && !self.mobile_runs.contains_key(&node.id)
                    && matches!(
                        spec.profile,
                        canvas::AgentProfile::Claude
                            | canvas::AgentProfile::Codex
                            | canvas::AgentProfile::Cursor
                            | canvas::AgentProfile::OpenCode
                    ))
                .then_some(node.id)
            })
            .collect();
        for node in &nodes {
            self.reserve_native_run(*node);
        }
        nodes
    }

    pub(in crate::runtime) fn finish_question_restore(&mut self, prepared: Vec<NodeId>) {
        let failed: Vec<_> = prepared
            .into_iter()
            .filter(|node| !self.live.contains_key(node))
            .collect();
        for node in failed {
            self.mobile_runs.remove(&node);
            self.remove_node_questions(node);
        }
    }

    pub(crate) fn native_question_process_id(&self, node: NodeId) -> Option<&str> {
        self.questions.processes.get(&node).map(String::as_str)
    }

    pub(in crate::runtime) fn reserve_question_process(&mut self, node: NodeId, id: String) {
        self.questions.native_hosts.remove(&node);
        self.questions.opencode_hosts.remove(&node);
        self.questions.cursor_hosts.remove(&node);
        self.close_question_session(node);
        self.questions.processes.insert(node, id);
        self.questions.retired_sessions.remove(&node);
        self.questions.ended_sessions.remove(&node);
        self.questions.resume_expected.remove(&node);
    }

    pub(in crate::runtime) fn question_resume_written(
        &mut self,
        node: NodeId,
        requested: bool,
        cancelled: bool,
    ) {
        if requested {
            self.questions
                .resume_expected
                .insert(node, std::time::Instant::now());
        }
        if cancelled {
            self.questions.resume_expected.remove(&node);
        }
    }

    pub(in crate::runtime) fn accept_question_lifecycle(&mut self, event: &AgentHookEvent) -> bool {
        // Generic server plugins observe child sessions too. Only the owned TUI may change
        // the active session; a session.created Hook cannot rebind this instance.
        if !self.opencode_accepts_hook_session(event) {
            return false;
        }
        let Some(process) = self.native_question_process_id(event.node_id) else {
            return true; // Legacy/managed sessions keep their existing lifecycle path.
        };
        if event
            .control_run_id
            .as_deref()
            .is_some_and(|id| id != process)
        {
            return false;
        }
        if event.control_run_id.is_none()
            || !matches!(
                event.profile,
                norma::protocol::AgentProfile::Claude
                    | norma::protocol::AgentProfile::Codex
                    | norma::protocol::AgentProfile::OpenCode
                    | norma::protocol::AgentProfile::Cursor
            )
        {
            return true;
        }
        let Some(session) = event.session_id.as_deref() else {
            return false;
        };
        let resuming = event.provider_event == "SessionStart"
            && event.raw_json["source"] == "resume"
            && self
                .questions
                .resume_expected
                .get(&event.node_id)
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(300));
        if self
            .questions
            .retired_sessions
            .get(&event.node_id)
            .is_some_and(|ids| ids.contains(session))
            && !resuming
        {
            return false;
        }
        if event.provider_event == "SessionStart" {
            self.questions.resume_expected.remove(&event.node_id);
            self.questions.ended_sessions.remove(&event.node_id);
            if resuming {
                if let Some(retired) = self.questions.retired_sessions.get_mut(&event.node_id) {
                    retired.remove(session);
                }
            }
        }
        let previous = self.agent_session_id(event.node_id);
        match event.provider_event.as_str() {
            "SessionStart" if previous.as_deref() != Some(session) => {
                self.close_question_session(event.node_id);
                if let Some(previous) = previous {
                    self.questions
                        .retired_sessions
                        .entry(event.node_id)
                        .or_default()
                        .insert(previous);
                }
            }
            "SessionEnd" if previous.as_deref() == Some(session) => {
                self.close_question_session(event.node_id);
                self.questions
                    .ended_sessions
                    .insert(event.node_id, session.into());
            }
            "UserPromptSubmit" => {
                self.questions.resume_expected.remove(&event.node_id);
            }
            _ => {}
        }
        true
    }

    pub(super) fn close_question_session(&mut self, node: NodeId) {
        let mut changed = self.questions.fallback.remove_node(node);
        for entry in &mut self.questions.entries {
            if entry.snapshot.question.target.node_id == node && entry.snapshot.state.blocks_input()
            {
                changed = true;
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
                entry.snapshot.error = Some("question_session_ended".into());
            }
        }
        let removed = self.questions.connections.remove(&node).is_some();
        if changed || removed {
            self.refresh_question_projection(node);
        }
    }
}
