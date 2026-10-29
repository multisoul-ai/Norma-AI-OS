//! Cursor: exact-session background store observations plus one verified live-PTY step at a time.
use super::*;
use crate::runtime::terminal_interaction::TerminalStepLease;
use norma::agent_hook::{
    AgentHookEvent, HookQuestion, HookQuestionRequest, NativeQuestionReply, NativeSendStatus,
};
use norma::cursor::{
    host::CursorQuestionHost,
    questions::CursorQuestion,
    screen::{CursorQuestionFrame, CursorQuestionStep},
    store::CursorStoreSnapshot,
};
use norma::protocol::{AgentProfile, QuestionRequestId};
use serde_json::json;
use std::time::{Duration, Instant};

pub(super) struct OwnedHost {
    host: CursorQuestionHost,
    active: Option<Active>,
    started: bool,
}
struct Active {
    ticket: HookQuestion,
    question: CursorQuestion,
    reply: Option<NativeQuestionReply>,
    lease: Option<TerminalStepLease>,
    sequence: Option<u64>,
    last_write: Option<Instant>,
    last_frame: Option<(CursorQuestionFrame, CursorQuestionStep)>,
    next_question: usize,
    stopped: bool,
}
impl Runtime {
    pub(in crate::runtime) fn cursor_launch_pending(&self, node: NodeId) -> bool {
        self.questions
            .cursor_hosts
            .get(&node)
            .is_some_and(|owned| !owned.started)
    }

    pub fn set_question_terminal_guard(
        &mut self,
        submission: &QuestionSubmission,
        guard: QuestionTerminalGuard,
    ) -> Result<(), &'static str> {
        self.validate_question_connection(submission.target())?;
        if submission.target().provider != AgentProfile::Cursor {
            return Err("question_not_terminal_transport");
        }
        let entry = self.question_submission_mut(submission)?;
        if entry.snapshot.state != QuestionState::Submitting {
            return Err("question_not_submitting");
        }
        entry.terminal_guard = Some(guard);
        Ok(())
    }

    pub fn adopt_cursor_question_host(
        &mut self,
        host: CursorQuestionHost,
        project: &str,
    ) -> Result<(), &'static str> {
        let node = host.node_id();
        if self.native_question_process_id(node) != Some(host.process_id())
            || !matches!(self.document.node(node).map(|n|&n.kind),Some(CanvasNodeKind::Agent(spec)) if spec.profile==canvas::AgentProfile::Cursor)
        {
            return Err("question_process_changed");
        }
        if self.questions.cursor_hosts.contains_key(&node) {
            return Err("question_host_already_owned");
        }
        let event = AgentHookEvent {
            node_id: node,
            profile: AgentProfile::Cursor,
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
            provider: AgentProfile::Cursor,
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
        self.questions.cursor_hosts.insert(
            node,
            OwnedHost {
                host,
                active: None,
                started: false,
            },
        );
        Ok(())
    }

    pub(super) fn drain_cursor_question_hosts(&mut self, project: &str) -> bool {
        let nodes = self
            .questions
            .cursor_hosts
            .keys()
            .copied()
            .collect::<Vec<_>>();
        let mut changed = false;
        for node in nodes {
            let Some(mut owned) = self.questions.cursor_hosts.remove(&node) else {
                continue;
            };
            if !owned.started {
                owned.started = self.native_terminal_frame(node).is_ok_and(|frame| {
                    frame.rows.iter().any(|row| {
                        let line = row.trim();
                        line.starts_with("→ Plan, search, build anything")
                            || line.starts_with("→ Add a follow-up")
                    })
                });
                changed |= owned.started;
            }
            if self.native_question_process_id(node) != Some(owned.host.process_id())
                || self.agent_session_id(node).as_deref() != Some(owned.host.session_id())
                || !self
                    .live
                    .get(&node)
                    .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
            {
                if let Some(active) = &mut owned.active {
                    self.stop_cursor_active(node, active, "question_session_changed");
                }
                changed = true;
                continue;
            }
            if let Some((at, snapshot)) = owned.host.poll() {
                if at.elapsed() <= Duration::from_millis(250) {
                    match snapshot {
                        Ok(snapshot) => {
                            changed |= self.observe_cursor_store(&mut owned, project, at, snapshot)
                        }
                        Err(reason) => {
                            if let Some(active) = &mut owned.active {
                                if !active.stopped {
                                    self.stop_cursor_active(node, active, reason);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
            self.questions.cursor_hosts.insert(node, owned);
        }
        changed
    }

    fn observe_cursor_store(
        &mut self,
        owned: &mut OwnedHost,
        project: &str,
        at: Instant,
        snapshot: CursorStoreSnapshot,
    ) -> bool {
        let node = owned.host.node_id();
        if let Some(active) = &mut owned.active {
            if active.last_write.is_some_and(|written| at < written) {
                return false;
            }
            if active.sequence.is_some_and(|sequence| {
                self.native_terminal_frame(node)
                    .is_ok_and(|frame| frame.input_sequence != sequence)
            }) && !active.stopped
            {
                self.stop_cursor_active(node, active, "question_native_takeover");
            }
            if let Some(result) = snapshot
                .results
                .iter()
                .find(|result| active.question.confirms_closed(result))
            {
                let ours = !active.stopped
                    && active.ticket.native_send_status() == NativeSendStatus::Sent
                    && active.reply.as_ref().is_some_and(|reply| {
                        active.question.confirms_answer(result, &reply.answers)
                    });
                let state = if active.question.confirms_cancelled(result) {
                    QuestionState::Cancelled
                } else if ours {
                    QuestionState::Answered
                } else if active.ticket.native_send_status() != NativeSendStatus::Unsent {
                    QuestionState::ConsumedUnattributed
                } else {
                    QuestionState::AnsweredElsewhere
                };
                let delivery = if ours {
                    active.reply.as_ref().map(|reply| reply.delivery_id.clone())
                } else {
                    None
                };
                if ours || state == QuestionState::Cancelled {
                    if let (Some(run), Some(sequence)) =
                        (self.mobile_runs.get_mut(&node), active.sequence)
                    {
                        run.acknowledge_question_input(sequence);
                    }
                }
                let changed = self.observe_owned_question_change(
                    node,
                    active.ticket.id(),
                    state,
                    delivery,
                    Some(format!("cursor/store-root:{}", snapshot.root_id)),
                    None,
                );
                active.ticket.stop_terminal_send();
                owned.active = None;
                return changed;
            }
            if !snapshot
                .pending
                .iter()
                .any(|question| question == &active.question)
            {
                if !active.stopped {
                    self.stop_cursor_active(node, active, "question_source_changed");
                    return true;
                }
                return false;
            }
            if active.stopped || active.ticket.native_send_status() == NativeSendStatus::Sent {
                return false;
            }
            match self.advance_cursor_answer(node, active) {
                Ok(changed) => changed,
                Err(reason) => {
                    self.stop_cursor_active(node, active, reason);
                    true
                }
            }
        } else {
            // Concurrent native forms or unsupported shapes remain owned by the original UI.
            if snapshot.pending.len() != 1 {
                return false;
            }
            let question = snapshot
                .pending
                .into_iter()
                .next()
                .expect("one pending question");
            let event = AgentHookEvent {
                node_id: node,
                profile: AgentProfile::Cursor,
                control_run_id: Some(owned.host.process_id().into()),
                provider_event: "NormaQuestion".into(),
                session_id: Some(owned.host.session_id().into()),
                turn_id: None,
                tool_name: Some("AskQuestion".into()),
                message: None,
                raw_json: json!({"connection_id":owned.host.connection_id(),"source":question.original_tool()}),
            };
            let Ok(ticket) = HookQuestion::new(
                HookQuestionRequest {
                    run_id: owned.host.process_id().into(),
                    event,
                },
                Instant::now(),
            ) else {
                return false;
            };
            if self
                .register_native_question_hook(&ticket, project, false)
                .is_err()
            {
                ticket.cancel();
                return false;
            }
            self.questions.native_session_changed |= self.agent_session_bindings.bind_if_unclaimed(
                node,
                canvas::AgentProfile::Cursor,
                owned.host.session_id(),
            );
            owned.active = Some(Active {
                ticket,
                question,
                reply: None,
                lease: None,
                sequence: None,
                last_write: None,
                last_frame: None,
                next_question: 0,
                stopped: false,
            });
            owned.started = true;
            true
        }
    }

    fn advance_cursor_answer(
        &mut self,
        node: NodeId,
        active: &mut Active,
    ) -> Result<bool, &'static str> {
        if active.reply.is_none() {
            let Some(raw) = active.ticket.take_response(Instant::now()) else {
                return Ok(false);
            };
            let reply: NativeQuestionReply =
                serde_json::from_value(raw).map_err(|_| "question_invalid_answer")?;
            reply.validate(active.ticket.adapter())?;
            let frame = self.native_terminal_frame(node)?;
            active.lease = Some(TerminalStepLease::new(&frame, Instant::now()));
            active.sequence = Some(frame.input_sequence);
            active.reply = Some(reply);
        }
        if self.mobile_terminal_is_controlled(node) {
            return Err("question_native_takeover");
        }
        let frame = self.native_terminal_frame(node)?;
        let now = Instant::now();
        let lease = active.lease.as_mut().ok_or("question_lease_missing")?;
        if !lease.ready(&frame, now)? {
            return Ok(false);
        }
        let view = CursorQuestionFrame::parse(&active.question, &frame.rows)
            .ok_or("question_terminal_layout_changed")?;
        if let Some((previous, step)) = &active.last_frame {
            if previous == &view {
                if active
                    .last_write
                    .is_some_and(|at| at.elapsed() >= Duration::from_secs(5))
                {
                    return Err("question_terminal_no_progress");
                }
                return Ok(false);
            }
            if !previous.confirms_step(step, &view) {
                return Err("question_terminal_layout_changed");
            }
            active.last_frame = None;
        }
        if view.question_index() != active.next_question {
            return Err("question_terminal_layout_changed");
        }
        let reply = active.reply.as_ref().ok_or("question_invalid_answer")?;
        let answer = reply
            .answers
            .iter()
            .find(|answer| answer.key == active.question.items()[view.question_index()].key)
            .ok_or("question_invalid_answer")?;
        let step = view.next_step(&active.question, answer)?;
        let enter = matches!(step, CursorQuestionStep::Enter);
        let complete = enter && view.question_index() + 1 == active.question.items().len();
        let observed_step = step.clone();
        let input = match step {
            CursorQuestionStep::Text(text) => crate::NodeInput::Paste(text),
            key => crate::NodeInput::Key(terminal::KeyInput {
                key: match key {
                    CursorQuestionStep::Up => terminal::Key::Up,
                    CursorQuestionStep::Down => terminal::Key::Down,
                    CursorQuestionStep::Space => terminal::Key::Char(' '),
                    CursorQuestionStep::Enter => terminal::Key::Enter,
                    CursorQuestionStep::Text(_) => unreachable!("text handled above"),
                },
                ctrl: false,
                alt: false,
                shift: false,
            }),
        };
        let entry = self
            .questions
            .entries
            .iter()
            .find(|entry| {
                entry
                    .hook
                    .as_ref()
                    .is_some_and(|hook| hook.id() == active.ticket.id())
            })
            .ok_or("question_not_found")?;
        let target = entry.snapshot.question.target.clone();
        let guard = entry.terminal_guard.clone();
        self.validate_question_connection(&target)?;
        let mut input = Some(input);
        let mut write = || {
            let input = input.take().ok_or("question_step_already_written")?;
            active
                .ticket
                .with_native_send(&reply.delivery_id, now, || {
                    self.write_node_input(node, input)
                        .map_err(|_| std::io::Error::other("question_terminal_write_failed"))?;
                    Ok(complete)
                })
                .map_err(|_| "question_terminal_write_failed")
        };
        if let Some(guard) = guard {
            guard(&mut write)?;
        } else {
            write()?;
        }
        let sequence = self.native_terminal_frame(node)?.input_sequence;
        lease.written(&frame, sequence, now);
        active.sequence = Some(sequence);
        active.last_write = Some(Instant::now());
        active.last_frame = Some((view, observed_step));
        if enter {
            active.next_question += 1;
        }
        Ok(true)
    }

    fn stop_cursor_active(&mut self, node: NodeId, active: &mut Active, reason: &str) {
        active.stopped = true;
        active.ticket.stop_terminal_send();
        self.observe_owned_question_change(
            node,
            active.ticket.id(),
            QuestionState::Unknown,
            active.reply.as_ref().map(|reply| reply.delivery_id.clone()),
            None,
            Some(reason.into()),
        );
    }

    pub(in crate::runtime) fn stop_cursor_question_input(&mut self, node: NodeId) {
        if let Some(mut owned) = self.questions.cursor_hosts.remove(&node) {
            if let Some(active) = &mut owned.active {
                if !active.stopped {
                    self.stop_cursor_active(node, active, "question_native_takeover");
                }
            }
            self.questions.cursor_hosts.insert(node, owned);
        }
    }
}
impl Drop for OwnedHost {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active.ticket.stop_terminal_send();
        }
    }
}
