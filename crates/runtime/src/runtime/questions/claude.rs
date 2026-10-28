//! Own the original native Hook ticket. No PTY input and no new agent task is generated here.
use super::*;
use norma::agent_hook::{AgentHookEvent, HookQuestion, QuestionHookStatus};
use norma::protocol::AgentProfile;
use std::time::Instant;

impl Runtime {
    /// Relinquish the original Hook before locating the TUI. Never infer that the user answered.
    pub fn release_question_to_native(&mut self, target: &QuestionRef) -> Result<(), &'static str> {
        self.validate_question_connection(target)?;
        if target.provider == AgentProfile::Cursor {
            self.stop_cursor_question_input(target.node_id);
        }
        let entry = self
            .questions
            .entries
            .iter_mut()
            .find(|entry| entry.snapshot.question.target == *target)
            .ok_or("question_not_found")?;
        if !entry.snapshot.state.blocks_input() {
            return Err("question_already_handled");
        }
        let hook = entry.hook.as_ref().ok_or("question_hook_missing")?;
        let revoked = hook.try_cancel();
        if !matches!(
            hook.adapter().provider(),
            AgentProfile::Claude | AgentProfile::Cursor
        ) && !revoked
        {
            return Err("question_submission_in_flight");
        }
        if hook.status(Instant::now()) != QuestionHookStatus::Responded {
            if matches!(
                entry.snapshot.state,
                QuestionState::Submitted | QuestionState::Unknown
            ) {
                entry.snapshot.state = QuestionState::Unknown;
            } else {
                entry.snapshot.state = QuestionState::Pending;
                entry.submission = None;
            }
            entry.snapshot.error = Some("question_native_takeover".into());
        }
        Ok(())
    }

    pub fn accept_native_question(
        &mut self,
        hook: HookQuestion,
        project_id: &str,
    ) -> Result<QuestionRef, &'static str> {
        let result = self.register_native_question_hook(&hook, project_id, false);
        if result.is_err() {
            self.retain_claude_question_blocker(&hook.request().event);
            hook.cancel();
        } else {
            self.reconcile_early_native_results(&hook);
        }
        result
    }

    pub(super) fn register_native_question_hook(
        &mut self,
        hook: &HookQuestion,
        project_id: &str,
        keep_closed: bool,
    ) -> Result<QuestionRef, &'static str> {
        let request = hook.request();
        if self.native_question_process_id(request.event.node_id) != Some(request.run_id.as_str()) {
            return Err("question_process_changed");
        }
        if self
            .questions
            .ended_sessions
            .get(&request.event.node_id)
            .is_some_and(|session| session == hook.adapter().session_id())
        {
            return Err("question_session_ended");
        }
        if !keep_closed && hook.status(Instant::now()) != QuestionHookStatus::Pending {
            return Err("question_hook_closed");
        }
        let target = QuestionRef {
            workspace_id: self
                .agent_hook_config
                .as_ref()
                .ok_or("question_workspace_missing")?
                .workspace_id
                .clone(),
            project_id: project_id.into(),
            node_id: request.event.node_id,
            provider: hook.adapter().provider(),
            run_id: self
                .mobile_agent_run_id(request.event.node_id)
                .ok_or("question_run_missing")?
                .into(),
            connection_id: hook.adapter().connection_id(&request.run_id),
            session_id: hook.adapter().session_id().into(),
            turn_id: request.event.turn_id.clone(),
            request_id: hook.adapter().request_id(),
            revision: 1,
        };
        // Claude's connection derives from the owned process. Native transports attach separately.
        if target.provider == AgentProfile::Claude {
            self.bind_question_connection(&target)?;
        } else {
            self.validate_question_connection(&target)?;
        }
        let question = hook.adapter().question(target.clone())?;
        self.register_question(question)?;
        let entry = self
            .questions
            .entries
            .iter_mut()
            .find(|entry| entry.snapshot.question.target == target)
            .ok_or("question_not_found")?;
        if let Some(previous) = &entry.hook {
            if previous.id() == hook.id() {
                return Ok(target);
            }
            return Err("question_hook_already_registered");
        }
        if entry.snapshot.state != QuestionState::Pending {
            return Err("question_already_handled");
        }
        entry.hook = Some(hook.clone());
        if hook.status(Instant::now()) != QuestionHookStatus::Pending {
            entry.snapshot.state = QuestionState::Unknown;
            entry.snapshot.error = Some("question_transport_closed".into());
        }
        Ok(target)
    }

    pub fn native_question_available(&self, target: &QuestionRef) -> bool {
        self.validate_question_connection(target).is_ok()
            && self.questions.entries.iter().any(|entry| {
                entry.snapshot.question.target == *target
                    && matches!(
                        entry.snapshot.state,
                        QuestionState::Pending | QuestionState::Failed
                    )
                    && entry.hook.as_ref().is_some_and(|hook| {
                        hook.status(Instant::now()) == QuestionHookStatus::Pending
                            && match hook.adapter() {
                                norma::agent_hook::NativeQuestion::Cursor { question, .. } => self
                                    .native_terminal_frame(target.node_id)
                                    .ok()
                                    .is_some_and(|frame| {
                                        norma::cursor::screen::CursorQuestionFrame::parse(
                                            question,
                                            &frame.rows,
                                        )
                                        .is_some()
                                    }),
                                _ => true,
                            }
                    })
            })
    }

    /// Invoke after the durable intent is committed. Cancellation wins before bytes become visible.
    pub fn deliver_native_question(
        &mut self,
        submission: &QuestionSubmission,
    ) -> Result<(), &'static str> {
        self.validate_question_connection(submission.target())?;
        let hook = self
            .question_submission_mut(submission)?
            .hook
            .clone()
            .ok_or("question_hook_missing")?;
        if hook.status(Instant::now()) != QuestionHookStatus::Pending {
            return Err("question_hook_closed");
        }
        self.start_question_delivery(submission)?;
        if !hook.submit_delivery(
            submission.answers(),
            submission.delivery_id(),
            Instant::now(),
        )? {
            let entry = self.question_submission_mut(submission)?;
            entry.snapshot.state = QuestionState::Failed;
            entry.snapshot.error = Some("question_hook_closed".into());
            return Err("question_hook_closed");
        }
        Ok(())
    }

    pub fn observe_native_question_result(&mut self, event: &AgentHookEvent) -> bool {
        let supported = match event.profile {
            AgentProfile::Claude => {
                matches!(
                    event.provider_event.as_str(),
                    "PostToolUse" | "PostToolUseFailure"
                ) && event.tool_name.as_deref() == Some("AskUserQuestion")
            }
            AgentProfile::Codex => {
                event.provider_event == "NormaQuestionResult"
                    && event.tool_name.as_deref() == Some("request_user_input")
            }
            _ => false,
        };
        if !supported {
            return false;
        }
        let fallback_closed = self.release_claude_question_blocker(event);
        let found = self.questions.entries.iter().position(|entry| {
            let target = &entry.snapshot.question.target;
            target.node_id == event.node_id
                && target.provider == event.profile
                && entry.snapshot.state.blocks_input()
                && entry.hook.as_ref().is_some_and(|hook| {
                    event.control_run_id.as_deref() == Some(hook.request().run_id.as_str())
                })
                && event.session_id.as_deref() == Some(target.session_id.as_str())
                && target
                    .turn_id
                    .as_ref()
                    .is_none_or(|turn| event.turn_id.as_ref() == Some(turn))
                && self.validate_question_run(target).is_ok()
                && entry.hook.as_ref().is_some_and(|hook| {
                    hook.adapter().confirms_consumption(&event.raw_json)
                        || hook.adapter().confirms_failure(&event.raw_json)
                })
        });
        let Some(index) = found else {
            self.remember_early_native_result(event);
            return fallback_closed;
        };
        if event.provider_event == "PostToolUseFailure" {
            let entry = &mut self.questions.entries[index];
            // The question itself is closed. Failed is reserved for a failed delivery while
            // the original question can still be waiting in the TUI.
            entry.snapshot.state = QuestionState::Cancelled;
            entry.snapshot.evidence = Some("claude/PostToolUseFailure:AskUserQuestion".into());
            entry.snapshot.error = Some(
                if event.raw_json["is_interrupt"] == true {
                    "question_interrupted"
                } else {
                    "question_tool_failed"
                }
                .into(),
            );
            if let Some(hook) = &entry.hook {
                hook.cancel();
            }
            self.refresh_question_projection(event.node_id);
            return true;
        }
        let entry = &self.questions.entries[index];
        let ours = entry
            .submission
            .as_ref()
            .filter(|submission| {
                matches!(
                    entry.snapshot.state,
                    QuestionState::Submitted | QuestionState::Unknown
                ) && entry.hook.as_ref().is_some_and(|hook| {
                    hook.owns_consumption(&event.raw_json, submission.delivery_id())
                        && hook
                            .adapter()
                            .confirms_answer(&event.raw_json, submission.answers())
                })
            })
            .cloned();
        let evidence = if event.profile == AgentProfile::Codex {
            format!(
                "codex/rawResponseItem/completed:{}",
                event.raw_json["event"]["params"]["item"]["call_id"]
                    .as_str()
                    .unwrap_or("")
            )
        } else {
            "claude/PostToolUse:AskUserQuestion".into()
        };
        if let Some(submission) = ours {
            return self
                .confirm_question_answer(&submission, submission.answers(), &evidence)
                .is_ok();
        }
        let entry = &mut self.questions.entries[index];
        entry.snapshot.state = QuestionState::AnsweredElsewhere;
        entry.snapshot.evidence = Some(evidence);
        entry.snapshot.error = None;
        if let Some(hook) = &entry.hook {
            hook.cancel();
        }
        self.refresh_question_projection(event.node_id);
        true
    }
}
