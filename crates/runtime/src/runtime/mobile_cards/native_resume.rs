//! Codex defers SessionStart until the first prompt after /resume.
use super::*;
use crate::{AgentTaskState, NodeInput, NodeView};
use terminal::Key;

#[derive(Default)]
pub(super) struct ResumeInput {
    // Only retain the /resume prefix, never arbitrary prompt text or session arguments.
    prefix: String,
    unknown: bool,
    pub(super) pending: Option<PendingResume>,
}

pub(super) struct PendingResume {
    revision: u64,
    sequence: u64,
    cancelled: bool,
    previous_session: Option<String>,
    previous_turn: Option<String>,
    pub(super) ready: bool,
}

impl PendingResume {
    pub(super) fn accepts_hook(&self, event: &norma::agent_hook::AgentHookEvent) -> bool {
        match event.provider_event.as_str() {
            "SessionStart" => true,
            "UserPromptSubmit" => event.turn_id.is_some() && event.turn_id != self.previous_turn,
            _ => false,
        }
    }
}

impl ResumeInput {
    pub(super) fn observe(&mut self, input: &NodeInput) -> bool {
        match input {
            NodeInput::Key(key) if key.key == Key::Enter && !key.ctrl && !key.alt && !key.shift => {
                let resume = !self.unknown && self.prefix.trim_end() == "/resume";
                self.reset_command();
                return resume;
            }
            NodeInput::Text(text) | NodeInput::Paste(text) => {
                for ch in text.chars() {
                    self.character(ch);
                }
            }
            NodeInput::Key(key) if !key.ctrl && !key.alt => match key.key {
                Key::Char(ch) => self.character(ch),
                Key::Backspace if !self.unknown && self.prefix != "/resume " => {
                    self.prefix.pop();
                }
                _ => self.unknown = true,
            },
            _ => self.unknown = true,
        }
        false
    }

    fn character(&mut self, ch: char) {
        if self.unknown {
            return;
        }
        if ch.is_control() {
            self.unknown = true;
        } else if self.prefix != "/resume " {
            self.prefix.push(ch);
            if !"/resume ".starts_with(&self.prefix) {
                self.unknown = true;
            }
        }
        if self.unknown {
            self.prefix.clear();
        }
    }

    pub(super) fn reset_command(&mut self) {
        self.prefix.clear();
        self.unknown = false;
    }
}

impl MobileRun {
    // Rotate identity in place: dropping MobileRun cancels its owned PTY as well.
    pub(super) fn reset_native_session(&mut self, run_id: String) {
        self.finish_terminal_interrupt(Err("native session replaced".into()));
        self.message.take();
        self.run_id = Some(run_id);
        self.session_id = None;
        self.prompt_id = None;
        self.latest_prompt_id = None;
        self.retry_task = Default::default();
        self.transcript_path = None;
        self.native_approvals = Default::default();
        self.task_clock = Default::default();
        self.task_steps = Default::default();
        self.task_summary = Default::default();
        self.native_input.new_session();
    }
}

impl Runtime {
    pub(crate) fn track_native_resume_input(
        &mut self,
        node: NodeId,
        requested: bool,
        cancelled: bool,
    ) {
        if !matches!(self.document.node(node).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if spec.profile == AgentProfile::Codex)
            || !self.mobile_runs.get(&node).is_some_and(|run| run.native)
        {
            return;
        }
        let pending = self.mobile_runs.get(&node).is_some_and(|run| {
            run.native_input
                .resume
                .pending
                .as_ref()
                .is_some_and(|p| !p.ready)
        });
        if !requested && !pending {
            return;
        }
        let Some(NodeView::Term { revision, .. }) = self.node_view(node) else {
            return;
        };
        if requested {
            // /resume is a local CLI command, not a queued task while a turn is executing.
            if self
                .mobile_runs
                .get(&node)
                .is_some_and(|run| run.prompt_id.is_some())
            {
                return;
            }
            let Ok(run_id) = fresh_control_id() else {
                return;
            };
            let previous_session = self.agent_session_id(node);
            let run = self.mobile_runs.get_mut(&node).expect("native run checked");
            let previous_turn = run.latest_prompt_id.clone();
            run.reset_native_session(run_id);
            run.native_input.resume.pending = Some(PendingResume {
                revision,
                sequence: run.native_input.sequence,
                cancelled: false,
                previous_session,
                previous_turn,
                ready: false,
            });
            self.agent_session_bindings.remove(node);
            self.events.update_agent_task_state(
                node,
                AgentTaskState::Ready,
                "native/resume".into(),
            );
        } else if let Some(run) = self.mobile_runs.get_mut(&node) {
            if let Some(pending) = run
                .native_input
                .resume
                .pending
                .as_mut()
                .filter(|p| !p.ready)
            {
                pending.revision = revision;
                pending.sequence = run.native_input.sequence;
                pending.cancelled = cancelled;
            }
        }
    }

    pub(crate) fn finish_native_resume(&mut self, node: NodeId) -> bool {
        let Some(run) = self.mobile_runs.get(&node) else {
            return false;
        };
        let Some(pending) = run
            .native_input
            .resume
            .pending
            .as_ref()
            .filter(|p| !p.ready)
        else {
            return false;
        };
        if run.native_input.sequence != pending.sequence
            || !(pending.cancelled
                || run.native_input.submitted_sequence() == Some(pending.sequence))
            || !matches!(self.node_view(node), Some(NodeView::Term { revision, .. }) if revision != pending.revision)
            || !self.native_prompt_ready(node)
        {
            return false;
        }
        let run = self.mobile_runs.get_mut(&node).expect("native run checked");
        let pending = run
            .native_input
            .resume
            .pending
            .as_mut()
            .expect("pending resume checked");
        pending.ready = true;
        if pending.cancelled {
            run.session_id = pending.previous_session.clone();
            if let Some(session) = &run.session_id {
                self.agent_session_bindings
                    .bind_if_unclaimed(node, AgentProfile::Codex, session);
            }
        }
        // The empty composer is from a newer frame, and no input followed the confirmed Enter/Escape.
        run.native_input.acknowledge_resume();
        true
    }
}
