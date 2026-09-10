use super::*;
use crossbeam_channel::{bounded, Receiver, Sender};
use norma::claude::TranscriptCursor;
use serde_json::{json, Value};
use std::time::Duration;

pub(super) struct InterruptRequest {
    cancelled: Arc<AtomicBool>,
    pub(super) deadline: Instant,
    completion: Option<Sender<Result<Value, String>>>,
}

impl InterruptRequest {
    fn finish(mut self, result: Result<Value, String>) {
        if let Some(sender) = self.completion.take() {
            let _ = sender.try_send(result);
        }
    }
}

impl Drop for InterruptRequest {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(sender) = self.completion.take() {
            let _ = sender.try_send(Err(
                "terminal run closed before interrupt was confirmed".into()
            ));
        }
    }
}

/// One reserved run/turn, moved between the UI and its background reader. No client-supplied path.
pub struct MobileTerminalInterrupt {
    node_id: NodeId,
    run_id: String,
    prompt_id: String,
    pub(super) history: InterruptHistory,
    cancelled: Arc<AtomicBool>,
    deadline: Instant,
    prepared: bool,
    confirmed: Option<&'static str>,
    sent: bool,
}

pub(super) enum InterruptHistory {
    Claude {
        reader: MobileClaudeHistoryRead,
        cursor: Option<TranscriptCursor>,
    },
    Codex {
        reader: MobileArchiveRead,
        session: String,
        position: Option<u64>,
    },
}

impl InterruptHistory {
    pub(super) fn is_closed(&self) -> bool {
        match self {
            Self::Claude { reader, .. } => reader.is_closed(),
            Self::Codex { reader, .. } => reader.is_closed(),
        }
    }
}

impl Drop for MobileTerminalInterrupt {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

impl MobileTerminalInterrupt {
    pub fn is_active(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire) && !self.history.is_closed()
    }

    fn check(&self) -> Result<(), String> {
        if !self.is_active() {
            return Err("terminal interrupt request revoked".into());
        }
        if Instant::now() >= self.deadline {
            return Err("terminal interrupt confirmation timed out; outcome unknown".into());
        }
        Ok(())
    }

    /// Blocking IO; capture the Provider position before the UI sends its native stop key.
    pub fn prepare(&mut self) -> Result<(), String> {
        self.check()?;
        if self.prepared {
            return Err("interrupt already prepared".into());
        }
        match &mut self.history {
            InterruptHistory::Claude { reader, cursor } => *cursor = Some(reader.end()?),
            InterruptHistory::Codex {
                reader,
                session,
                position,
            } => {
                let (latest, outcome) = reader.codex_interrupt_boundary(
                    session,
                    &self.prompt_id,
                    None,
                    self.deadline,
                )?;
                if outcome.is_some() {
                    return Err("Codex turn already ended before stop".into());
                }
                *position = Some(latest);
            }
        }
        self.prepared = true;
        self.check()
    }

    /// Blocking bounded IO; half-written new records retain their cursor until complete.
    pub fn poll(&mut self) -> Result<bool, String> {
        self.check()?;
        if !self.sent {
            return Err("stop key not sent".into());
        }
        if !self.prepared {
            return Err("interrupt not prepared".into());
        }
        let outcome = match &mut self.history {
            InterruptHistory::Claude { reader, cursor } => {
                // 100 = existing transcript page maximum; each page also has a 16 MiB scan cap.
                let page = reader.read(cursor.clone(), 100)?;
                *cursor = Some(page.cursor);
                page.messages
                    .iter()
                    .any(|message| {
                        message.interrupted && message.prompt_id.as_deref() == Some(&self.prompt_id)
                    })
                    .then_some("interrupted")
            }
            InterruptHistory::Codex {
                reader,
                session,
                position,
            } => {
                reader
                    .codex_interrupt_boundary(session, &self.prompt_id, *position, self.deadline)?
                    .1
            }
        };
        self.check()?;
        self.confirmed = self.confirmed.or(outcome);
        Ok(self.confirmed.is_some())
    }
}

impl MobileRun {
    pub(super) fn record_claude_turn(
        &mut self,
        node_id: NodeId,
        state: storage::MobileTurnState,
    ) -> Result<(), String> {
        let outcome = storage::MobileTurnOutcome {
            key: storage::MobileTurnKey {
                node_id,
                profile: "Claude".into(),
                run_id: self.run_id.clone().ok_or("missing run")?,
                session_id: self.session_id.clone().ok_or("missing session")?,
                turn_id: self.prompt_id.clone().ok_or("missing prompt")?,
            },
            state,
        };
        self.claude
            .as_mut()
            .ok_or("Claude audit unavailable")?
            .record_turn(outcome)
    }
    pub(super) fn finish_terminal_interrupt(&mut self, result: Result<Value, String>) {
        if let Some(request) = self.interrupt.take() {
            request.finish(result);
        }
    }

    pub(super) fn expire_terminal_interrupt(&mut self) -> bool {
        if let Some(request) = &self.interrupt {
            if request.cancelled.load(Ordering::Acquire) || Instant::now() >= request.deadline {
                self.finish_terminal_interrupt(Err(
                    "terminal interrupt abandoned or timed out; outcome unknown".into(),
                ));
                return true;
            }
        }
        false
    }
}

impl Runtime {
    /// Reserve one native interrupt. Authorization precedes this seam; preparation does file IO
    /// off-thread, send revalidates the turn, and the receipt waits for Provider evidence.
    pub fn interrupt_mobile_terminal(
        &mut self,
        node_id: NodeId,
        run_id: &str,
        prompt_id: &str,
    ) -> Result<(MobileTerminalInterrupt, Receiver<Result<Value, String>>), String> {
        let history = match self.document.node(node_id).map(|node| &node.kind) {
            Some(CanvasNodeKind::Agent(spec)) if spec.profile == AgentProfile::Claude => {
                InterruptHistory::Claude {
                    reader: self.mobile_claude_history(node_id, run_id)?,
                    cursor: None,
                }
            }
            Some(CanvasNodeKind::Agent(spec)) if spec.profile == AgentProfile::Codex => {
                let session = self
                    .mobile_runs
                    .get(&node_id)
                    .filter(|run| {
                        run.native
                            && run.run_id.as_deref() == Some(run_id)
                            && !run.control.is_cancelled()
                    })
                    .and_then(|run| run.session_id.clone())
                    .ok_or("native Codex session unavailable")?;
                InterruptHistory::Codex {
                    reader: self.mobile_archive_read()?,
                    session,
                    position: None,
                }
            }
            _ => return Err("terminal stop unavailable".into()),
        };
        self.validate_terminal_stop_input(node_id)?;
        let run = self
            .mobile_runs
            .get_mut(&node_id)
            .filter(|run| {
                run.prompt_id.as_deref() == Some(prompt_id) && (run.native || run.claude.is_some())
            })
            .ok_or("terminal turn unavailable")?;
        run.expire_terminal_interrupt();
        if run.message.is_some() {
            return Err("mobile message submission pending".into());
        }
        if run.interrupt.is_some() {
            return Err("interrupt already requested".into());
        }
        if !self
            .live
            .get(&node_id)
            .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
        {
            return Err("Agent terminal unavailable".into());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        // Same 30s bound as controlled startup: unknown completion must not leave a pending request forever.
        let deadline = Instant::now() + Duration::from_secs(30);
        let (sender, receipt) = bounded(1);
        run.interrupt = Some(InterruptRequest {
            cancelled: cancelled.clone(),
            deadline,
            completion: Some(sender),
        });
        if let Some(claude) = &mut run.claude {
            claude.cancel();
        }
        Ok((
            MobileTerminalInterrupt {
                node_id,
                run_id: run_id.into(),
                prompt_id: prompt_id.into(),
                history,
                cancelled,
                deadline,
                prepared: false,
                confirmed: None,
                sent: false,
            },
            receipt,
        ))
    }

    fn matches_terminal_interrupt(&self, request: &MobileTerminalInterrupt) -> bool {
        self.mobile_runs.get(&request.node_id).is_some_and(|run| {
            run.run_id.as_deref() == Some(&request.run_id)
                && run.prompt_id.as_deref() == Some(&request.prompt_id)
                && run
                    .interrupt
                    .as_ref()
                    .is_some_and(|pending| Arc::ptr_eq(&pending.cancelled, &request.cancelled))
        })
    }

    /// UI-thread PTY input only. Call after preparation and processing queued local Hook events.
    pub fn send_mobile_terminal_interrupt(
        &mut self,
        request: &mut MobileTerminalInterrupt,
    ) -> Result<(), String> {
        request.check()?;
        if !request.prepared || !self.matches_terminal_interrupt(request) {
            return Err("terminal interrupt not prepared or turn changed".into());
        }
        if request.sent {
            return Err("stop key already sent".into());
        }
        self.validate_terminal_stop_input(request.node_id)?;
        if self.mobile_runs[&request.node_id].message.is_some() {
            return Err("mobile message submission pending".into());
        }
        self.mobile_runs
            .get_mut(&request.node_id)
            .expect("bound run")
            .native_input
            .advance();
        request.sent = true;
        // This reserved system key is not desktop draft input; preserve any existing draft.
        // Claude 2.1.202 needs Escape. Codex 0.153.4 needs Ctrl-C for a durable turnCompleted;
        // Escape did not provide that completion evidence in the real native probe.
        let codex = matches!(request.history, InterruptHistory::Codex { .. });
        self.live
            .get_mut(&request.node_id)
            .ok_or("Agent terminal unavailable")?
            .input(crate::NodeInput::Key(terminal::KeyInput {
                key: if codex {
                    terminal::Key::Char('c')
                } else {
                    terminal::Key::Escape
                },
                ctrl: codex,
                alt: false,
                shift: false,
            }))
            .map_err(|error| error.to_string())
    }

    /// Only a bound request's private reader evidence can produce interrupted. A read/transport
    /// error reports unknown outcome without clearing the current turn or claiming it has ended.
    pub fn finish_mobile_terminal_interrupt(
        &mut self,
        request: &MobileTerminalInterrupt,
        error: Option<String>,
    ) -> bool {
        if !self.matches_terminal_interrupt(request) {
            return false;
        }
        let run = self.mobile_runs.get_mut(&request.node_id).expect("run");
        if let Some(error) = error {
            run.finish_terminal_interrupt(Err(error));
            return true;
        }
        if request.check().is_err() || request.confirmed.is_none() || !request.sent {
            return false;
        }
        // Native outcomes use the App's existing native-session store before its remote receipt.
        if !run.native {
            if let Err(error) =
                run.record_claude_turn(request.node_id, storage::MobileTurnState::Interrupted)
            {
                run.finish_terminal_interrupt(Err(error));
                return false;
            }
        }
        let status = request.confirmed.expect("confirmed terminal boundary");
        run.finish_terminal_interrupt(Ok(json!({"turnId":request.prompt_id,"status":status})));
        run.prompt_id = None;
        self.events.update_agent_task_state(
            request.node_id,
            match status {
                "completed" => crate::AgentTaskState::Done,
                "failed" => crate::AgentTaskState::Failed {
                    message: "Agent turn failed".into(),
                },
                _ => crate::AgentTaskState::Ended,
            },
            "mobile/terminalTurnEnded".into(),
        );
        true
    }
}
