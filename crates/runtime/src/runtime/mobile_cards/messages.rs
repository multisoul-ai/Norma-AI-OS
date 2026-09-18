//! Provider-confirmed mobile message submission.
use super::*;
use crate::{AgentTaskState, NodeInput};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct NativeInput {
    pub(super) sequence: u64,
    submitted: Option<u64>,
    started: bool,
    pub(super) dirty: bool,
    pub(super) approval_menu_open: bool,
    pub(super) resume: native_resume::ResumeInput,
}
impl NativeInput {
    pub(crate) fn observe(&mut self, input: &NodeInput, menu_navigation: bool) -> bool {
        if matches!(input, NodeInput::Text(s) | NodeInput::Paste(s) if s.is_empty()) {
            return false;
        }
        self.advance();
        if menu_navigation {
            // Menu confirmation is not a prompt submission. Invalidate the old screen's
            // navigation context immediately, before its closing redraw/Hook can arrive.
            self.approval_menu_open = matches!(input, NodeInput::Key(key) if matches!(key.key, terminal::Key::Up | terminal::Key::Down));
            return false;
        }
        self.approval_menu_open = false;
        let submit = matches!(input, NodeInput::Key(key) if key.key == terminal::Key::Enter && !key.ctrl && !key.alt && !key.shift);
        // These keys cannot create text in an empty composer. History recall/Tab remain unknown.
        // An existing draft stays protected: a deletion key alone does not prove it is now empty.
        self.dirty |= !submit
            && !matches!(input, NodeInput::Key(key) if matches!(key.key,
            terminal::Key::Escape | terminal::Key::Backspace | terminal::Key::Left | terminal::Key::Right));
        if submit {
            self.submitted = Some(self.sequence);
        }
        self.resume.observe(input)
    }
    pub(crate) fn acknowledge_start(&mut self) {
        if !self.started {
            self.acknowledge();
        }
    }
    pub(crate) fn acknowledge(&mut self) {
        self.started = true;
        self.approval_menu_open = false;
        // A late Provider hook must not clear the next draft typed after Enter.
        if self.submitted.take() == Some(self.sequence) {
            self.dirty = false;
            self.resume.reset_command();
        }
    }
    pub(super) fn new_session(&mut self) {
        self.started = false;
        self.approval_menu_open = false;
    }
    pub(super) fn submitted_sequence(&self) -> Option<u64> {
        self.submitted
    }
    pub(super) fn acknowledge_resume(&mut self) {
        self.submitted = Some(self.sequence);
        self.acknowledge();
    }
    pub(super) fn advance(&mut self) {
        self.sequence = self.sequence.saturating_add(1);
    }
}

enum Stage {
    Terminal {
        text: String,
        enter_at: Instant,
        entered: bool,
        render: Option<(u64, bool)>,
    },
    Codex(Receiver<Result<Value, String>>),
}
pub(crate) struct Submission {
    stage: Stage,
    deadline: Instant,
    reply: Option<Sender<Result<Value, String>>>,
}
impl Submission {
    fn finish(mut self, result: Result<Value, String>) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.try_send(result);
        }
    }
}
impl Drop for Submission {
    fn drop(&mut self) {
        if let Some(reply) = self.reply.take() {
            let _ = reply.try_send(Err("outcome_unknown".into()));
        }
    }
}

impl MobileRun {
    pub(super) fn accept_mobile_message(&mut self, event: &norma::agent_hook::AgentHookEvent) {
        let Some(submission) = self.message.as_ref() else {
            return;
        };
        let Stage::Terminal { text, entered, .. } = &submission.stage else {
            return;
        };
        // Claude 2.1.202 expands pasted Tab to four spaces; preserve every other byte.
        let provider_text = if event.profile == norma::protocol::AgentProfile::Claude {
            text.replace('\t', "    ")
        } else if self.native {
            text.trim().to_owned()
        } else {
            text.clone()
        };
        let result = if *entered
            && event.raw_json["prompt"].as_str() == Some(&provider_text)
            && Instant::now() < submission.deadline
        {
            event.turn_id.as_deref().filter(|id| !id.is_empty()).map(|turn| {
                json!({"node_id":event.node_id,"run_id":self.run_id,"session_id":self.session_id,"turn_id":turn})
            }).ok_or_else(|| "outcome_unknown".into())
        } else {
            Err("outcome_unknown".into())
        };
        if result.is_ok() {
            self.native_input.dirty = false;
        }
        self.message
            .take()
            .expect("pending submission")
            .finish(result);
    }
}

impl Runtime {
    /// Retain the last observed identity after completion clears the active prompt. Never infer success.
    pub fn mobile_latest_turn_key(&self, node: NodeId) -> Option<storage::MobileTurnKey> {
        let run = self.mobile_runs.get(&node)?;
        let (profile, turn) = if let Some(host) = &run.codex {
            ("Codex", host.latest_turn_id.as_ref()?)
        } else {
            (
                match self.document.node(node).map(|node| &node.kind) {
                    Some(CanvasNodeKind::Agent(spec)) => spec.profile.display_title(),
                    _ => return None,
                },
                run.latest_prompt_id.as_ref()?,
            )
        };
        Some(storage::MobileTurnKey {
            node_id: node,
            profile: profile.into(),
            run_id: run.run_id.clone()?,
            session_id: run.session_id.clone()?,
            turn_id: turn.clone(),
        })
    }
    /// A capture may only finish against this unchanged, quiescent task identity.
    pub fn mobile_delivery_can_capture(&self, key: &storage::MobileTurnKey) -> bool {
        self.mobile_latest_turn_key(key.node_id).as_ref() == Some(key)
            && matches!(
                self.agent_task_state(key.node_id),
                Some(AgentTaskState::Done | AgentTaskState::Failed { .. } | AgentTaskState::Ended)
            )
            && self.mobile_runs.get(&key.node_id).is_some_and(|run| {
                run.message.is_none()
                    && run.native_input.submitted.is_none()
                    && run.prompt_id.is_none()
                    && run.codex.as_ref().is_none_or(|host| host.turn_id.is_none())
            })
    }
}

impl Runtime {
    /// Validates without starting a process. Empty cards may start only after the App accepts a real prompt.
    pub fn validate_mobile_message(
        &mut self,
        expected: &norma::mobile::CardSnapshot,
        text: &str,
    ) -> Result<(), &'static str> {
        if text.trim().is_empty()
            || text.len() > 32 * 1024
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err("invalid_message");
        }
        self.drain_mobile_codex();
        let current = self
            .mobile_card_snapshots()
            .into_iter()
            .find(|c| c.node_id == expected.node_id)
            .ok_or("not_found")?;
        if !current.mobile_control {
            return Err("mobile_control_required");
        }
        if &current != expected {
            return Err("card_changed");
        }
        if !matches!(
            current.state.as_str(),
            "idle" | "ready" | "done" | "ended" | "failed"
        ) {
            return Err("state_not_ready");
        }
        if let Some(run) = self.mobile_runs.get(&expected.node_id) {
            if run.message.is_some() || run.interrupt.is_some() || run.control.is_cancelled() {
                return Err("state_not_ready");
            }
            // ponytail: no Provider draft-buffer API; unknown edits stay protected until confirmed.
            // Replace conservative tracking with Provider draft acknowledgement when available.
            if run.native_input.dirty {
                return Err("desktop_input_pending");
            }
            // Ready Hook can precede PTY mode output. Never turn LF/Tab into CLI keystrokes.
            if run.codex.is_none()
                && text.contains(['\n', '\t'])
                && !self
                    .live
                    .get(&expected.node_id)
                    .is_some_and(|live| live.terminal_bracketed_paste_enabled())
            {
                return Err("terminal_not_ready");
            }
        }
        Ok(())
    }

    /// Returns a receipt only when Codex acknowledges its RPC or Claude emits the exact PromptSubmit.
    /// Authorization, queue/coordinator checks and cold startup belong to the calling App gateway.
    pub fn send_mobile_message(
        &mut self,
        expected: &norma::mobile::CardSnapshot,
        text: String,
    ) -> Result<Receiver<Result<Value, String>>, String> {
        self.validate_mobile_message(expected, &text)
            .map_err(str::to_owned)?;
        let node = expected.node_id;
        let run = self
            .mobile_runs
            .get(&node)
            .filter(|r| r.run_id.is_some() && (r.session_id.is_some() || r.is_native()))
            .ok_or("session_changed")?;
        if !self
            .live
            .get(&node)
            .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
        {
            return Err("session_changed".into());
        }
        let now = Instant::now();
        let render = run.codex.is_none().then(|| match self.node_view(node) {
            Some(crate::NodeView::Term { revision, .. }) => (revision, false),
            _ => (0, false),
        });
        let stage = if run.codex.is_some() {
            let request = self.request_mobile_codex_message(node, &text)?;
            // Until the RPC is confirmed, another send could otherwise steer an unobserved turn.
            self.mobile_runs
                .get_mut(&node)
                .expect("run")
                .native_input
                .dirty = true;
            Stage::Codex(request)
        } else if run.prompt_id.is_none() {
            // Bypass public input only for this reserved paste; desktop input is locked until Provider acknowledgement.
            self.mobile_runs
                .get_mut(&node)
                .expect("run")
                .native_input
                .dirty = true;
            self.mobile_runs
                .get_mut(&node)
                .expect("run")
                .native_input
                .advance();
            self.live
                .get_mut(&node)
                .expect("terminal")
                .input(NodeInput::Paste(text.clone()))
                .map_err(|_| "outcome_unknown")?;
            // 100ms matches the native reply queue's paste-to-Enter delay.
            Stage::Terminal {
                text,
                enter_at: now + Duration::from_millis(100),
                entered: false,
                render,
            }
        } else {
            return Err("state_not_ready".into());
        };
        let (reply, receipt) = bounded(1);
        self.mobile_runs.get_mut(&node).expect("run").message = Some(Submission {
            stage,
            deadline: now + Duration::from_secs(30),
            reply: Some(reply),
        });
        Ok(receipt)
    }

    /// Only nonblocking RPC polling, in-memory state and the existing PTY input seam occur here.
    pub(crate) fn advance_mobile_messages(&mut self, now: Instant) -> bool {
        let mut changed = false;
        for (node, run) in &mut self.mobile_runs {
            let native = run.is_native();
            let Some(submission) = &mut run.message else {
                continue;
            };
            let result = if now >= submission.deadline || run.control.is_cancelled() {
                if let Stage::Terminal {
                    entered, render, ..
                } = &submission.stage
                {
                    tracing::warn!(
                        node_id = *node,
                        entered,
                        ?render,
                        cancelled = run.control.is_cancelled(),
                        "terminal submission unconfirmed"
                    );
                }
                Some(Err("outcome_unknown".into()))
            } else {
                match &mut submission.stage {
                Stage::Codex(receiver) => match receiver.try_recv() {
                    Ok(Ok(value)) => Some(value["turn"]["id"].as_str().filter(|id| !id.is_empty()).map(|turn| json!({"node_id":node,"run_id":run.run_id,"session_id":run.session_id,"turn_id":turn})).ok_or_else(|| "outcome_unknown".into())),
                    Ok(Err(_)) => Some(Err("outcome_unknown".into())),
                    Err(TryRecvError::Disconnected) => Some(Err("outcome_unknown".into())),
                    Err(TryRecvError::Empty) => None,
                },
                Stage::Terminal { enter_at, entered, render, .. } if !*entered => {
                    if let Some((revision, painted)) = render {
                        if let Some(crate::NodeView::Term { revision: current, .. }) = self.live.get(node).and_then(|live| live.view()) {
                            if !*painted && *revision != current { *revision = current; *painted = true; *enter_at = now + Duration::from_millis(100); }
                        }
                        // Both native CLIs can paint the pasted draft after 100ms under load. Submit once,
                        // after the first render and the native Enter delay; later redraws cannot starve it.
                        if !*painted || now < *enter_at { continue; }
                    } else if now < *enter_at { continue; }
                    if run.prompt_id.is_some() || !(matches!(self.events.agent_task_state(*node), Some(AgentTaskState::Ready | AgentTaskState::Done | AgentTaskState::Ended | AgentTaskState::Failed { .. })) || (native && self.events.agent_task_state(*node).is_none())) {
                        Some(Err("outcome_unknown".into()))
                    } else {
                        *entered = true;
                        let input = NodeInput::Key(terminal::KeyInput { key: terminal::Key::Enter, ctrl: false, alt: false, shift: false });
                        run.native_input.advance();
                        match self.live.get_mut(node).map(|live| live.input(input)) {
                            Some(Ok(())) => { changed = true; None },
                            _ => Some(Err("outcome_unknown".into())),
                        }
                    }
                }
                _ => None,
            }
            };
            if let Some(result) = result {
                if result.is_ok() {
                    run.native_input.dirty = false;
                }
                run.message.take().expect("submission").finish(result);
                changed = true;
            }
        }
        changed
    }
}
