//! Bind only a host-owned instance; HTTP question content cannot attach itself to a card.
use super::*;
use norma::agent_hook::{AgentHookEvent, NativeSendStatus};
use norma::opencode::native_questions::{OpenCodeNativeEvent, OpenCodeQuestionHost};
use norma::protocol::{AgentProfile, QuestionRequestId};
use serde_json::json;

pub(super) struct OwnedHost {
    host: OpenCodeQuestionHost,
    blocked: bool,
    unavailable: bool,
}

impl Runtime {
    pub(in crate::runtime) fn opencode_accepts_hook_session(&self, event: &AgentHookEvent) -> bool {
        self.questions
            .opencode_hosts
            .get(&event.node_id)
            .is_none_or(|owned| {
                event
                    .session_id
                    .as_deref()
                    .is_none_or(|session| owned.host.matches_session(session))
            })
    }

    pub fn adopt_opencode_question_host(
        &mut self,
        host: OpenCodeQuestionHost,
        project: &str,
    ) -> Result<(), &'static str> {
        let node = host.node_id();
        if self.native_question_process_id(node) != Some(host.process_id())
            || !matches!(self.document.node(node).map(|node|&node.kind),Some(CanvasNodeKind::Agent(spec)) if spec.profile==canvas::AgentProfile::OpenCode)
        {
            return Err("question_process_changed");
        }
        if self.questions.opencode_hosts.contains_key(&node) {
            return Err("question_host_already_owned");
        }
        let event = AgentHookEvent {
            node_id: node,
            profile: AgentProfile::OpenCode,
            control_run_id: Some(host.process_id().into()),
            provider_event: "SessionStart".into(),
            session_id: Some(host.session_id().into()),
            turn_id: None,
            tool_name: None,
            message: None,
            raw_json: json!({"source":"startup"}),
        };
        if !self.observe_mobile_hook(&event) {
            return Err("question_session_changed");
        }
        let target = QuestionRef {
            workspace_id: self
                .agent_hook_config
                .as_ref()
                .ok_or("question_workspace_missing")?
                .workspace_id
                .clone(),
            project_id: project.into(),
            node_id: node,
            provider: AgentProfile::OpenCode,
            run_id: self
                .mobile_agent_run_id(node)
                .ok_or("question_run_missing")?
                .into(),
            connection_id: host.connection_id().into(),
            session_id: host.session_id().into(),
            turn_id: None,
            request_id: QuestionRequestId::String("native-attachment".into()),
            revision: 1,
        };
        self.bind_question_connection(&target)?;
        self.questions.opencode_hosts.insert(
            node,
            OwnedHost {
                host,
                blocked: false,
                unavailable: false,
            },
        );
        Ok(())
    }

    pub(super) fn opencode_input_pending(&self, node: NodeId) -> bool {
        self.questions
            .opencode_hosts
            .get(&node)
            .is_some_and(|owned| owned.host.input_pending())
            && self
                .live
                .get(&node)
                .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
    }

    pub(super) fn drain_opencode_question_hosts(&mut self, project: &str) -> bool {
        let mut events = Vec::new();
        let mut projections = Vec::new();
        for (node, owned) in &mut self.questions.opencode_hosts {
            let process = owned.host.process_id().to_owned();
            for event in owned.host.events().try_iter().take(128) {
                events.push((*node, process.clone(), event));
            }
            if !owned.unavailable && !owned.host.is_available() {
                owned.unavailable = true;
                events.push((*node, process, OpenCodeNativeEvent::Unavailable));
            }
            let blocked = owned.host.input_pending();
            if owned.blocked != blocked {
                owned.blocked = blocked;
                projections.push(*node);
            }
        }
        let mut changed = !projections.is_empty();
        for (node, process, event) in events {
            if self.native_question_process_id(node) != Some(process.as_str()) {
                continue;
            }
            changed |= match event {
                OpenCodeNativeEvent::ForegroundChanged {
                    session,
                    connection,
                } => self.follow_opencode_foreground(node, session, connection, project),
                OpenCodeNativeEvent::Question(ticket) => {
                    let accepted = self
                        .register_native_question_hook(&ticket, project, true)
                        .is_ok();
                    if accepted {
                        self.questions.native_session_changed |=
                            self.agent_session_bindings.bind_if_unclaimed(
                                node,
                                canvas::AgentProfile::OpenCode,
                                ticket.adapter().session_id(),
                            );
                    } else {
                        ticket.cancel();
                    }
                    accepted
                }
                OpenCodeNativeEvent::Changed {
                    ticket_id,
                    state,
                    delivery_id,
                    evidence,
                    error,
                } => self.observe_owned_question_change(
                    node,
                    &ticket_id,
                    state,
                    delivery_id,
                    evidence,
                    error,
                ),
                OpenCodeNativeEvent::Unavailable => {
                    let mut changed = false;
                    for entry in &mut self.questions.entries {
                        if entry.snapshot.question.target.node_id == node
                            && entry.snapshot.state.blocks_input()
                        {
                            entry.snapshot.state = QuestionState::Unknown;
                            entry.snapshot.error = Some("question_transport_closed".into());
                            if let Some(hook) = &entry.hook {
                                hook.cancel();
                            }
                            changed = true;
                        }
                    }
                    changed
                }
            };
            projections.push(node);
        }
        projections.sort_unstable();
        projections.dedup();
        for node in projections {
            self.refresh_question_projection(node);
        }
        changed
    }

    fn follow_opencode_foreground(
        &mut self,
        node: NodeId,
        session: Option<String>,
        connection: String,
        project: &str,
    ) -> bool {
        // This event comes only from the owned TUI route observer, never a generic server Hook.
        self.close_question_session(node);
        self.agent_session_bindings.remove(node);
        self.questions.native_session_changed = true;
        let Some(run) = self.mobile_runs.get_mut(&node) else {
            return true;
        };
        if run.follow_question_session(session.clone()).is_err() {
            return true;
        }
        let Some(session) = session else {
            return true;
        };
        self.questions.ended_sessions.remove(&node);
        if let Some(retired) = self.questions.retired_sessions.get_mut(&node) {
            retired.remove(&session);
        }
        self.agent_session_bindings.bind_if_unclaimed(
            node,
            canvas::AgentProfile::OpenCode,
            &session,
        );
        let Some(config) = &self.agent_hook_config else {
            return true;
        };
        let Some(run) = self.mobile_agent_run_id(node) else {
            return true;
        };
        let target = QuestionRef {
            workspace_id: config.workspace_id.clone(),
            project_id: project.into(),
            node_id: node,
            provider: AgentProfile::OpenCode,
            run_id: run.into(),
            connection_id: connection,
            session_id: session,
            turn_id: None,
            request_id: QuestionRequestId::String("native-attachment".into()),
            revision: 1,
        };
        let _ = self.bind_question_connection(&target);
        true
    }

    pub(super) fn opencode_connection_current(&self, target: &QuestionRef) -> bool {
        self.questions
            .opencode_hosts
            .get(&target.node_id)
            .is_none_or(|owned| {
                owned
                    .host
                    .matches_connection(&target.session_id, &target.connection_id)
            })
    }

    pub(super) fn observe_owned_question_change(
        &mut self,
        node: NodeId,
        ticket_id: &str,
        state: QuestionState,
        delivery: Option<String>,
        evidence: Option<String>,
        error: Option<String>,
    ) -> bool {
        let Some(index) = self.questions.entries.iter().position(|entry| {
            entry.snapshot.question.target.node_id == node
                && entry
                    .hook
                    .as_ref()
                    .is_some_and(|ticket| ticket.id() == ticket_id)
                && entry.snapshot.state.blocks_input()
                && (self
                    .validate_question_run(&entry.snapshot.question.target)
                    .is_ok()
                    || (entry.snapshot.question.target.provider == AgentProfile::OpenCode
                        && self.questions.opencode_hosts.contains_key(&node)
                        && entry.hook.as_ref().is_some_and(|hook| {
                            self.native_question_process_id(node)
                                == Some(hook.request().run_id.as_str())
                        })))
        }) else {
            return false;
        };
        if matches!(state, QuestionState::Submitting | QuestionState::Submitted) {
            return true;
        }
        let entry = &self.questions.entries[index];
        if state == QuestionState::Answered {
            let submission = entry
                .submission
                .as_ref()
                .filter(|submission| {
                    Some(submission.delivery_id()) == delivery.as_deref()
                        && entry
                            .hook
                            .as_ref()
                            .is_some_and(|hook| hook.native_send_status() == NativeSendStatus::Sent)
                })
                .cloned();
            if let (Some(submission), Some(evidence)) = (submission, evidence.as_deref()) {
                if self.validate_question_run(submission.target()).is_ok() {
                    return self
                        .confirm_question_answer(&submission, submission.answers(), evidence)
                        .is_ok();
                }
                // Only this still-owned OpenCode process can settle an older foreground lease.
                // The host verified original session/message/call/input/answers with its retained
                // client. Settlement never rebinds the card or grants a new send qualification.
                if evidence.trim().is_empty() {
                    return false;
                }
                let entry = &mut self.questions.entries[index];
                entry.snapshot.state = QuestionState::Answered;
                entry.snapshot.evidence = Some(evidence.into());
                entry.snapshot.error = None;
                self.refresh_question_projection(node);
                return true;
            }
            return false;
        }
        let entry = &mut self.questions.entries[index];
        entry.snapshot.state = state;
        entry.snapshot.delivery_id = delivery;
        entry.snapshot.evidence = evidence;
        entry.snapshot.error = error;
        if let Some(hook) = &entry.hook {
            hook.cancel();
        }
        self.refresh_question_projection(node);
        true
    }
}
