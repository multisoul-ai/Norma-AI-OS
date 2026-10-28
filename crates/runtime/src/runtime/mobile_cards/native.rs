//! Remote input observes the existing terminal; it never replaces a native Agent session.
use super::*;

impl MobileRun {
    pub(in crate::runtime) fn follow_question_session(
        &mut self,
        session: Option<String>,
    ) -> Result<(), &'static str> {
        let run = fresh_control_id().map_err(|_| "question_identity_unavailable")?;
        self.reset_native_session(run);
        self.session_id = session;
        Ok(())
    }

    pub(in crate::runtime) fn native_input_sequence(&self) -> Option<u64> {
        self.native.then_some(self.native_input.sequence)
    }

    pub(in crate::runtime) fn acknowledge_question_input(&mut self, sequence: u64) {
        if self.native && self.native_input.sequence == sequence {
            self.native_input.acknowledge_resume();
        }
    }

    pub(super) fn is_native(&self) -> bool {
        self.native
    }
}

impl Runtime {
    pub(super) fn native_prompt_ready(&self, node: NodeId) -> bool {
        if self.native_question_host_pending(node)
            || !self
                .mobile_runs
                .get(&node)
                .is_some_and(MobileRun::is_native)
            || !matches!(self.document.node(node).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if spec.profile == AgentProfile::Codex)
            || !self.live.get(&node).is_some_and(|live| {
                live.terminal_status() == Some(TerminalStatus::Running)
                    && live.terminal_bracketed_paste_enabled()
            })
        {
            return false;
        }
        // ponytail: Codex 0.153 starts its thread lazily; its loading screen also has an empty prompt.
        // Require a loaded model header or footer; /resume replays history without the startup header.
        let Some(crate::NodeView::Term { snapshot, .. }) = self.node_view(node) else {
            return false;
        };
        // Startup notices can push the loaded model/directory out of a small card's viewport.
        // Read the bounded terminal tail, independent of the desktop's scroll position.
        // Same 120×512 cell ceiling as mobile_terminal_page; no file/session scans.
        if snapshot.cols > 512 || snapshot.lines > 120 {
            return false;
        }
        let total = snapshot.history_size + snapshot.lines;
        let Some(rows) =
            self.node_terminal_rows(node, total.saturating_sub(120), total.saturating_sub(1))
        else {
            return false;
        };
        let lines: Vec<String> = rows
            .iter()
            .map(|row| row.iter().map(|cell| cell.c).collect())
            .collect();
        // Inspect the live bottom screen, regardless of the desktop's scrollback position.
        // A previous input prompt in history does not make a current menu ready for text.
        if !lines
            .iter()
            .rev()
            .take(snapshot.lines)
            .any(|line| line.trim_start().starts_with("› Ask Codex to do anything"))
        {
            return false;
        }
        // /resume emits no SessionStart until the next prompt. Its live footer still identifies
        // a loaded model and reasoning level; never treat a prompt in scrollback as readiness.
        if lines.iter().rev().take(snapshot.lines).any(|line| {
            let Some((model, directory)) = line.trim().split_once(" · ") else {
                return false;
            };
            let mut fields = model.split_whitespace();
            let Some(name) = fields.next() else {
                return false;
            };
            let Some(effort) = fields.next() else {
                return false;
            };
            let name = name.to_ascii_lowercase();
            (name.starts_with("gpt-") || name.starts_with("o3") || name.starts_with("o4"))
                && matches!(
                    effort,
                    "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "default"
                )
                // Codex also displays its optional fast service tier after the reasoning level.
                // Accept only this known marker; unrelated menu/footer text remains unready.
                && matches!(fields.next(), None | Some("fast"))
                && fields.next().is_none()
                && !directory.is_empty()
        }) {
            return true;
        }
        // A screen clear can retain old loading lines in scrollback. The latest model wins.
        // Codex's compact layout can omit directory entirely; it is not a readiness signal.
        lines
            .iter()
            .rev()
            .find_map(|line| line.split_once("model:"))
            .is_some_and(|(_, value)| {
                !value.trim().is_empty() && !value.trim_start().starts_with("loading")
            })
    }

    pub(crate) fn track_native_agent(&mut self, node: NodeId) {
        if self.mobile_runs.contains_key(&node)
            || !self.live.contains_key(&node)
            || !matches!(self.document.node(node).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if matches!(spec.profile, AgentProfile::Claude | AgentProfile::Codex | AgentProfile::Cursor))
        {
            return;
        }
        self.reserve_native_run(node);
    }

    pub(in crate::runtime) fn reserve_native_run(&mut self, node: NodeId) {
        let Ok(run_id) = fresh_control_id() else {
            return;
        };
        self.reserve_question_process(node, run_id.clone());
        self.mobile_runs.insert(
            node,
            MobileRun {
                task_clock: timing::TaskClock::default(),
                task_steps: progress::TaskSteps::default(),
                task_summary: summary::TaskSummary::default(),
                native: true,
                native_approvals: native_approval::NativeApprovals::default(),
                generation: 0,
                run_id: Some(run_id),
                session_id: self.agent_session_id(node),
                prompt_id: None,
                latest_prompt_id: None,
                claude: None,
                interrupt: None,
                control: Arc::new(launch::LaunchControl::default()),
                codex: None,
                transcript_path: None,
                native_input: messages::NativeInput::default(),
                message: None,
            },
        );
    }

    pub(super) fn observe_native_hook(
        &mut self,
        event: &norma::agent_hook::AgentHookEvent,
    ) -> bool {
        let Some(CanvasNodeKind::Agent(spec)) =
            self.document.node(event.node_id).map(|node| &node.kind)
        else {
            return false;
        };
        if spec.profile.display_title() != format!("{:?}", event.profile) {
            return false;
        }
        if !self.accept_question_lifecycle(event)
            || !self.question_accepts_task_session(event.node_id, event.session_id.as_deref())
        {
            return false;
        }
        let Some(session) = event.session_id.as_ref().filter(|id| !id.is_empty()) else {
            return true;
        };
        if let Some(run) = self.mobile_runs.get_mut(&event.node_id) {
            if let Some(pending) = &run.native_input.resume.pending {
                // A previous turn can finish late while the picker is open. Only the next
                // lifecycle/submit Hook may confirm the identity; preserve the new mobile receipt.
                if !pending.accepts_hook(event) {
                    return false;
                }
                run.native_input.resume.pending = None;
                run.session_id = None;
            }
        }
        if self
            .mobile_runs
            .get(&event.node_id)
            .is_some_and(|run| run.session_id.as_ref().is_some_and(|id| id != session))
        {
            // A new native /new session invalidates the old receipt; other late hooks cannot rebind it.
            if event.provider_event != "SessionStart" {
                return false;
            }
            let Ok(run_id) = fresh_control_id() else {
                return false;
            };
            if let Some(run) = self.mobile_runs.get_mut(&event.node_id) {
                run.reset_native_session(run_id);
            }
        }
        let Some(run) = self.mobile_runs.get_mut(&event.node_id) else {
            return false;
        };
        // Legacy desktop Hooks without turn IDs retain their existing status flow, but cannot
        // produce a mobile receipt/outcome. Once a real turn is known, require its exact ID.
        if !matches!(
            event.provider_event.as_str(),
            "SessionStart" | "UserPromptSubmit" | "SessionEnd"
        ) && (run.latest_prompt_id.is_some() || event.turn_id.is_some())
            && (run.prompt_id.is_none() || run.prompt_id != event.turn_id)
        {
            return false;
        }
        run.session_id = Some(session.clone());
        if matches!(
            event.profile,
            norma::protocol::AgentProfile::Claude | norma::protocol::AgentProfile::Cursor
        ) {
            let Ok(path) = history::hook_path(run, event) else {
                return false;
            };
            if let Some(path) = path {
                run.transcript_path = Some(path);
            }
        }
        run.native_approvals.observe(event);
        match event.provider_event.as_str() {
            // Startup menus also use Enter. Only clear that confirmed input, never a later draft.
            "SessionStart" => run.native_input.acknowledge_start(),
            "PermissionRequest" => run.native_input.approval_menu_open = true,
            "UserPromptSubmit" => {
                // A repeated old Hook must not acknowledge a later human Enter on this PTY.
                let acknowledge_input = !run.native_input.controlled_draft
                    || (event.turn_id.is_some() && run.latest_prompt_id != event.turn_id);
                run.finish_terminal_interrupt(Err(
                    "native turn replaced before interrupt confirmation".into(),
                ));
                run.prompt_id = event.turn_id.clone();
                run.latest_prompt_id = event.turn_id.clone();
                run.accept_mobile_message(event);
                if acknowledge_input {
                    run.native_input.acknowledge();
                }
            }
            "Stop" | "StopFailure" if event.turn_id == run.prompt_id => {
                let status = if event.provider_event == "Stop" {
                    "completed"
                } else {
                    "failed"
                };
                run.finish_terminal_interrupt(Ok(
                    serde_json::json!({"turnId":event.turn_id,"status":status}),
                ));
                run.prompt_id = None;
            }
            "SessionEnd" => {
                run.finish_terminal_interrupt(Err(
                    "native session ended before interrupt confirmation".into(),
                ));
                // /clear and /resume end a session while retaining this same native CLI.
                // The PTY owner cancels on node/project close, not on a session Hook.
                run.message.take();
                run.prompt_id = None;
            }
            _ => {}
        }
        run.task_clock.observe_hook(event);
        run.task_steps.observe(event);
        run.task_summary.observe_hook(event);
        self.retain_claude_question_blocker(event);
        true
    }

    pub fn mobile_native_history(
        &mut self,
        node: NodeId,
    ) -> Result<Option<(MobileArchiveRead, storage::RecordedMobileSession)>, String> {
        let Some(saved) = self.mobile_native_session(node)? else {
            return Ok(None);
        };
        Ok(Some((self.mobile_archive_read()?, saved)))
    }

    /// Capture the current native identity for background history reads and durable stop outcomes.
    pub fn mobile_native_session(
        &self,
        node: NodeId,
    ) -> Result<Option<storage::RecordedMobileSession>, String> {
        let Some(run) = self.mobile_runs.get(&node).filter(|run| run.is_native()) else {
            return Ok(None);
        };
        let card = self
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == node)
            .ok_or("card unavailable")?;
        let saved = storage::RecordedMobileSession {
            session: storage::MobileSession {
                key: storage::MobileSessionKey {
                    node_id: node,
                    profile: card.profile,
                    run_id: card.run_id.ok_or("run unavailable")?,
                    session_id: card.session_id.ok_or("session unavailable")?,
                },
                workspace_root: self.workspace_root.clone().ok_or("workspace unavailable")?,
                title: card.title,
            },
            transcript_path: run.transcript_path.clone(),
            sequence: 0,
            recorded_at_unix: 0,
        };
        Ok(Some(saved))
    }
}

impl Runtime {
    /// Capture accepted native Hook identity only; persistence is owned by the App background executor.
    pub fn native_mobile_event(
        &self,
        event: &norma::agent_hook::AgentHookEvent,
    ) -> Option<(
        storage::MobileSession,
        Option<PathBuf>,
        Option<storage::MobileTurnOutcome>,
    )> {
        let run = self
            .mobile_runs
            .get(&event.node_id)
            .filter(|run| run.native && run.session_id == event.session_id)?;
        if !matches!(
            event.provider_event.as_str(),
            "SessionStart" | "UserPromptSubmit" | "Stop" | "StopFailure"
        ) {
            return None;
        }
        let node = self.document.node(event.node_id)?;
        let CanvasNodeKind::Agent(spec) = &node.kind else {
            return None;
        };
        let outcome = if matches!(event.provider_event.as_str(), "Stop" | "StopFailure") {
            let key = self.mobile_latest_turn_key(event.node_id)?;
            if event.turn_id.as_deref() != Some(&key.turn_id) {
                return None;
            }
            Some(storage::MobileTurnOutcome {
                summary: run.task_summary.snapshot(),
                key,
                state: if event.provider_event == "Stop" {
                    storage::MobileTurnState::Completed
                } else {
                    storage::MobileTurnState::Failed
                },
            })
        } else {
            None
        };
        Some((
            storage::MobileSession {
                key: storage::MobileSessionKey {
                    node_id: node.id,
                    profile: spec.profile.display_title().into(),
                    run_id: run.run_id.clone()?,
                    session_id: run.session_id.clone()?,
                },
                workspace_root: self.workspace_root.clone()?,
                title: node.kind.display_title(),
            },
            run.transcript_path.clone(),
            outcome,
        ))
    }
}
