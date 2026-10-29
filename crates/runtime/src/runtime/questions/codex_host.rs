//! Only an owned in-process host can attach a Codex connection. Question JSON never binds one.
use super::*;
use norma::agent_hook::{AgentHookEvent, NativeQuestion, NativeSendStatus};
use norma::codex::native_questions::{CodexNativeEvent, CodexQuestionHost};
use norma::codex::relay::{CodexAttachment, CodexQuestionLease, CodexRelayChange};
use norma::protocol::{AgentProfile, QuestionRequestId};
use serde_json::json;

#[cfg(test)]
#[path = "../../../tests/unit/codex_host_handover_tests.rs"]
mod handover_tests;

pub(super) struct OwnedHost {
    host: CodexQuestionHost,
    unavailable: bool,
    blocked: bool,
    attaching: bool,
}

impl Runtime {
    pub fn adopt_codex_question_host(
        &mut self,
        host: CodexQuestionHost,
    ) -> Result<(), &'static str> {
        let node = host.node_id();
        if self.native_question_process_id(node) != Some(host.process_id())
            || !matches!(self.document.node(node).map(|n| &n.kind), Some(CanvasNodeKind::Agent(spec)) if spec.profile == canvas::AgentProfile::Codex)
        {
            return Err("question_process_changed");
        }
        if self.questions.native_hosts.contains_key(&node) {
            return Err("question_host_already_owned");
        }
        self.questions.native_hosts.insert(
            node,
            OwnedHost {
                host,
                unavailable: false,
                blocked: false,
                attaching: false,
            },
        );
        Ok(())
    }

    /// Channel-only foreground drain. Provider IO and source/consumption parsing stay on the host.
    pub fn drain_native_question_hosts(&mut self, project_id: &str) -> bool {
        let cursor_changed = self.drain_cursor_question_hosts(project_id);
        let opencode_changed = self.drain_opencode_question_hosts(project_id);
        let ready: Vec<_> = self
            .live
            .values_mut()
            .filter_map(|live| live.take_native_question_host())
            .collect();
        let mut adopted = false;
        for host in ready {
            adopted |= match host {
                crate::live::NativeQuestionHost::Codex(host) => {
                    self.adopt_codex_question_host(host).is_ok()
                }
                crate::live::NativeQuestionHost::Cursor(host) => {
                    self.adopt_cursor_question_host(host, project_id).is_ok()
                }
                crate::live::NativeQuestionHost::OpenCode(host) => {
                    self.adopt_opencode_question_host(host, project_id).is_ok()
                }
            };
        }
        let mut events = Vec::new();
        let mut input_changes = Vec::new();
        for (node, owned) in &mut self.questions.native_hosts {
            let process = owned.host.process_id().to_owned();
            input_changes.push((*node, owned.host.input_pending()));
            for event in owned.host.events().try_iter().take(128) {
                events.push((*node, process.clone(), event));
            }
            if !owned.unavailable && !owned.host.is_available() {
                owned.unavailable = true;
                events.push((*node, process, CodexNativeEvent::Unavailable));
            }
        }
        let mut changed = adopted || opencode_changed || cursor_changed;
        for (node, process, event) in events {
            if self.native_question_process_id(node) != Some(process.as_str()) {
                continue;
            }
            changed |= match event {
                CodexNativeEvent::Attached(attachment) => self
                    .attach_codex_questions(node, &process, project_id, &attachment)
                    .is_ok(),
                CodexNativeEvent::SessionUsed(attachment) => {
                    if self.agent_session_id(node).as_deref() == Some(attachment.thread_id()) {
                        self.remember_codex_session(node, attachment.thread_id());
                    }
                    false
                }
                CodexNativeEvent::Question(ticket) => {
                    let result = self.register_native_question_hook(&ticket, project_id, true);
                    if let Err(reason) = result {
                        tracing::warn!(node, reason, "native Codex question registration rejected");
                    }
                    let accepted = result.is_ok();
                    if accepted {
                        self.remember_codex_session(node, ticket.adapter().session_id());
                    }
                    accepted
                }
                CodexNativeEvent::Changed(change) => {
                    self.observe_codex_question_change(node, &change)
                }
                CodexNativeEvent::DeliveryFailed { question, reason } => {
                    self.fail_codex_answer(node, &question, &reason)
                }
                CodexNativeEvent::Unavailable => self.codex_questions_unavailable(node),
            };
        }
        for (node, blocked) in input_changes {
            if let Some(owned) = self.questions.native_hosts.get_mut(&node) {
                if owned.blocked != blocked {
                    owned.blocked = blocked;
                    changed = true;
                    self.refresh_question_projection(node);
                }
            }
        }
        changed
    }

    pub(super) fn native_input_pending(&self, node: NodeId) -> bool {
        self.questions
            .native_hosts
            .get(&node)
            .is_some_and(|owned| !owned.attaching && owned.host.input_pending())
            && self
                .live
                .get(&node)
                .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
    }

    pub fn take_native_question_session_changed(&mut self) -> bool {
        std::mem::take(&mut self.questions.native_session_changed)
    }

    fn remember_codex_session(&mut self, node: NodeId, session: &str) {
        // A real question proves this turn has history. Do not persist an empty startup thread.
        self.questions.native_session_changed |= self.agent_session_bindings.bind_if_unclaimed(
            node,
            canvas::AgentProfile::Codex,
            session,
        );
    }

    fn attach_codex_questions(
        &mut self,
        node: NodeId,
        process: &str,
        project: &str,
        attachment: &CodexAttachment,
    ) -> Result<(), &'static str> {
        // The native attachment owns session binding; its already-buffered input RPC cannot
        // prevent that binding from being established on this foreground drain.
        if let Some(owned) = self.questions.native_hosts.get_mut(&node) {
            owned.attaching = true;
        }
        let result = self.bind_codex_attachment(node, process, project, attachment);
        if let Some(owned) = self.questions.native_hosts.get_mut(&node) {
            owned.attaching = false;
        }
        self.refresh_question_projection(node);
        if let Err(reason) = result {
            tracing::warn!(node, reason, "native Codex attachment rejected");
        }
        if result.is_ok() && attachment.resumed() {
            self.remember_codex_session(node, attachment.thread_id());
        }
        result
    }

    fn bind_codex_attachment(
        &mut self,
        node: NodeId,
        process: &str,
        project: &str,
        attachment: &CodexAttachment,
    ) -> Result<(), &'static str> {
        if attachment.resumed() {
            // This originates from the native TUI's actual resume response, not an external Hook.
            self.question_resume_written(node, true, false);
        }
        let event = AgentHookEvent {
            control_run_id: Some(process.into()),
            node_id: node,
            profile: AgentProfile::Codex,
            provider_event: "SessionStart".into(),
            session_id: Some(attachment.thread_id().into()),
            turn_id: None,
            tool_name: None,
            message: None,
            raw_json: json!({"source":if attachment.resumed() {"resume"} else {"startup"}}),
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
            provider: AgentProfile::Codex,
            run_id: self
                .mobile_agent_run_id(node)
                .ok_or("question_run_missing")?
                .into(),
            connection_id: attachment.connection_id().into(),
            session_id: attachment.thread_id().into(),
            turn_id: None,
            request_id: QuestionRequestId::String("native-attachment".into()),
            revision: 1,
        };
        self.bind_question_connection(&target)
    }

    fn codex_entry(&self, node: NodeId, lease: &CodexQuestionLease) -> Option<usize> {
        self.questions.entries.iter().position(|entry| {
            let target = &entry.snapshot.question.target;
            target.node_id == node && target.connection_id == lease.connection_id()
                && entry.snapshot.state.blocks_input() && self.validate_question_run(target).is_ok()
                && entry.hook.as_ref().is_some_and(|hook| matches!(hook.adapter(),NativeQuestion::Codex {question,..} if question == lease.question()))
        })
    }

    fn observe_codex_question_change(&mut self, node: NodeId, change: &CodexRelayChange) -> bool {
        let Some(index) = self.codex_entry(node, &change.question) else {
            return false;
        };
        if matches!(
            change.state,
            QuestionState::Pending | QuestionState::Submitting | QuestionState::Submitted
        ) {
            return true;
        }
        let entry = &self.questions.entries[index];
        if matches!(
            change.state,
            QuestionState::Answered | QuestionState::AnsweredElsewhere
        ) {
            // The registered host already validated the exact call/result/answers on its worker.
            let evidence = format!(
                "codex/rawResponseItem/completed:{}",
                change.question.question().item_id()
            );
            let ours = entry
                .submission
                .as_ref()
                .filter(|submission| {
                    change.state == QuestionState::Answered
                        && change.delivery_id.as_deref() == Some(submission.delivery_id())
                        && entry.hook.as_ref().is_some_and(|hook| {
                            matches!(
                                hook.native_send_status(),
                                NativeSendStatus::Sent | NativeSendStatus::Unknown
                            )
                        })
                })
                .cloned();
            if let Some(submission) = ours {
                return self
                    .confirm_question_answer(&submission, submission.answers(), &evidence)
                    .is_ok();
            }
            let entry = &mut self.questions.entries[index];
            entry.snapshot.state = QuestionState::AnsweredElsewhere;
            entry.snapshot.evidence = Some(evidence);
            entry.snapshot.error = None;
        } else {
            self.questions.entries[index].snapshot.state = if change.state == QuestionState::Expired
            {
                QuestionState::Unknown
            } else {
                change.state
            };
        }
        if let Some(hook) = &self.questions.entries[index].hook {
            hook.cancel();
        }
        self.refresh_question_projection(node);
        true
    }

    fn fail_codex_answer(
        &mut self,
        node: NodeId,
        lease: &CodexQuestionLease,
        reason: &str,
    ) -> bool {
        let Some(index) = self.codex_entry(node, lease) else {
            return false;
        };
        let entry = &mut self.questions.entries[index];
        entry.snapshot.state = if entry
            .hook
            .as_ref()
            .is_some_and(|hook| hook.native_send_status() == NativeSendStatus::Unsent)
        {
            QuestionState::Failed
        } else {
            QuestionState::Unknown
        };
        entry.snapshot.error = Some(reason.into());
        self.refresh_question_projection(node);
        true
    }

    fn codex_questions_unavailable(&mut self, node: NodeId) -> bool {
        let mut changed = false;
        for entry in &mut self.questions.entries {
            if entry.snapshot.question.target.node_id == node && entry.snapshot.state.blocks_input()
            {
                changed |= entry.snapshot.state != QuestionState::Unknown;
                entry.snapshot.state = QuestionState::Unknown;
                entry
                    .snapshot
                    .error
                    .get_or_insert_with(|| "question_transport_closed".into());
                if let Some(hook) = &entry.hook {
                    hook.cancel();
                }
            }
        }
        self.refresh_question_projection(node);
        changed
    }
}
