use super::terminal_interrupt::InterruptHistory;
use super::*;
use norma::mobile::ApprovalTarget;
use serde_json::{json, Value};

/// One bound native choice. The gateway persists intent before reservation; Provider evidence
/// is read off-thread after the original PTY consumes its own numeric menu shortcut.
pub struct MobileNativeApproval {
    target: ApprovalTarget,
    decision: ApprovalDecision,
    tool_id: String,
    pub(super) history: InterruptHistory,
    prepared: bool,
    active: Arc<AtomicBool>,
    executed: Arc<AtomicBool>,
    process: Option<terminal::ChildProcessObserver>,
    command: String,
    deadline: Instant,
    sent: bool,
    confirmed: bool,
    interrupted: bool,
}

impl Drop for MobileNativeApproval {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
    }
}

impl MobileNativeApproval {
    fn complete(&self) -> bool {
        self.confirmed && (self.decision == ApprovalDecision::Allow || self.interrupted)
    }
    fn check(&self) -> Result<(), String> {
        if !self.active.load(Ordering::Acquire) || self.history.is_closed() {
            return Err("approval_changed".into());
        }
        if Instant::now() >= self.deadline {
            return Err("outcome_unknown".into());
        }
        Ok(())
    }

    pub fn prepare(&mut self) -> Result<(), String> {
        self.check()?;
        if self.prepared {
            return Err("approval_already_prepared".into());
        }
        match &mut self.history {
            InterruptHistory::Claude { reader, cursor } => *cursor = Some(reader.end()?),
            InterruptHistory::Codex {
                reader,
                session,
                position,
            } => {
                let (latest, ended) = reader.codex_interrupt_boundary(
                    session,
                    &self.target.turn_id,
                    None,
                    self.deadline,
                )?;
                if ended.is_some() {
                    return Err("approval_changed".into());
                }
                *position = Some(latest);
            }
        }
        self.prepared = true;
        if self.process.as_mut().is_some_and(|p| {
            if matches!(&self.history, InterruptHistory::Codex { .. }) {
                !p.prepare_with_child(|argv| {
                    argv.first().is_some_and(|program| {
                        Path::new(program)
                            .file_name()
                            .is_some_and(|name| name == "codex")
                    })
                })
            } else {
                !p.prepare()
            }
        }) {
            self.process = None;
        }
        self.check()
    }

    pub fn poll(&mut self) -> Result<bool, String> {
        self.check()?;
        if !self.sent || !self.prepared {
            return Err("approval_not_sent".into());
        }
        if self.decision == ApprovalDecision::Allow {
            self.confirmed |= self.executed.load(Ordering::Acquire)
                || self.process.as_ref().is_some_and(|process| {
                    process.matches_new_child(|argv| match &self.history {
                        InterruptHistory::Claude { reader, .. } => {
                            matches_bash_process(argv, &self.command, &reader.projects_root)
                        }
                        InterruptHistory::Codex { .. } => {
                            matches_codex_process(argv, &self.command)
                        }
                    })
                });
        }
        self.check()?;
        if self.complete() {
            return Ok(true);
        }
        if let InterruptHistory::Codex {
            reader,
            session,
            position,
        } = &self.history
        {
            if self.decision == ApprovalDecision::Allow {
                // A finished history item can race its Hook. Only the exact PostToolUse
                // confirms execution here; completion alone must not guess Allow delivery.
                return Ok(false);
            }
            // Escape in the verified Codex permission dialog cancels the whole turn.
            // Reuse canonical timeline boundaries; synthesized thread status is not evidence.
            let (_, ended) = reader.codex_interrupt_boundary(
                session,
                &self.target.turn_id,
                *position,
                self.deadline,
            )?;
            if let Some(status) = ended {
                if status != "interrupted" {
                    return Err("outcome_unknown".into());
                }
                self.confirmed = true;
                self.interrupted = true;
            }
            self.check()?;
            return Ok(self.complete());
        }
        let InterruptHistory::Claude { reader, cursor } = &mut self.history else {
            unreachable!()
        };
        // Reuse the bounded 100-message/16MiB reader; no full transcript rescans.
        let page = reader.read(cursor.clone(), 100)?;
        *cursor = Some(page.cursor);
        for message in page.messages {
            if message.role != "user" || message.prompt_id.as_deref() != Some(&self.target.turn_id)
            {
                continue;
            }
            self.interrupted |= message.interrupted;
            let Some(blocks) = message.content.as_array() else {
                continue;
            };
            for block in blocks {
                if block["type"] != "tool_result" || block["tool_use_id"] != self.tool_id {
                    continue;
                }
                let denied = block["is_error"] == true && block["content"] == REJECTED;
                self.confirmed = match self.decision {
                    ApprovalDecision::Allow => block["is_error"] != true,
                    ApprovalDecision::Deny => denied,
                };
                if !self.confirmed && (self.decision == ApprovalDecision::Deny || denied) {
                    return Err("outcome_unknown".into());
                }
            }
        }
        self.check()?;
        Ok(self.complete())
    }

    pub fn result(&self) -> Result<Value, String> {
        self.check()?;
        if !self.complete() {
            return Err("outcome_unknown".into());
        }
        Ok(
            json!({"node_id":self.target.node_id,"run_id":self.target.run_id,
            "session_id":self.target.session_id,"turn_id":self.target.turn_id,
            "request_id":self.target.request_id,"decision":self.decision,"status":"submitted"}),
        )
    }
}

const REJECTED: &str = "The user doesn't want to proceed with this tool use. The tool use was rejected (eg. if it was a file edit, the new_string was NOT written to the file). STOP what you are doing and wait for the user to tell you how to proceed.";

// ponytail: observed Codex 0.153.4 macOS shell only; unknown wrappers retain exact Hook evidence.
pub(super) fn matches_codex_process(argv: &[String], command: &str) -> bool {
    !command.is_empty() && argv == ["/bin/zsh", "-c", command]
}

// ponytail: observed Claude 2.1.202 macOS zsh wrapper only. Unknown wrappers wait for
// the exact transcript/PostToolUse evidence; no substring or shell execution here.
pub(super) fn matches_bash_process(argv: &[String], command: &str, projects_root: &Path) -> bool {
    let [shell, option, script] = argv else {
        return false;
    };
    if shell != "/bin/zsh" || option != "-c" || command.is_empty() {
        return false;
    }
    let Some(config) = projects_root.parent().and_then(Path::to_str) else {
        return false;
    };
    let Some(rest) = script.strip_prefix(&format!("source {config}/shell-snapshots/snapshot-zsh-"))
    else {
        return false;
    };
    let Some((snapshot, rest)) = rest.split_once(".sh 2>/dev/null || true && setopt NO_EXTENDED_GLOB NO_BARE_GLOB_QUAL 2>/dev/null || true && eval ") else { return false; };
    if snapshot.is_empty()
        || !snapshot
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return false;
    }
    // Claude uses double-quoted apostrophes, unlike our general launch quoting helper.
    let quoted = format!("'{}'", command.replace('\'', "'\"'\"'"));
    let Some(cwd) = rest.strip_prefix(&format!("{quoted} < /dev/null && pwd -P >| /tmp/claude-"))
    else {
        return false;
    };
    cwd.strip_suffix("-cwd")
        .is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_hexdigit()))
}

impl Runtime {
    pub fn reserve_native_approval(
        &mut self,
        expected: &ApprovalTarget,
        decision: ApprovalDecision,
    ) -> Result<MobileNativeApproval, &'static str> {
        if self.mobile_terminal_is_controlled(expected.node_id) {
            return Err("terminal_controlled");
        }
        let current = self
            .mobile_approval_snapshots(expected.node_id)?
            .into_iter()
            .find(|s| s.target == *expected)
            .ok_or("approval_changed")?;
        if !current.decisions.contains(&decision) {
            return Err("decision_unavailable");
        }
        let history = match expected.profile.as_str() {
            "Claude" => InterruptHistory::Claude {
                reader: self
                    .mobile_claude_history(expected.node_id, &expected.run_id)
                    .map_err(|_| "approval_unavailable")?,
                cursor: None,
            },
            "Codex" => InterruptHistory::Codex {
                reader: self
                    .mobile_archive_read()
                    .map_err(|_| "approval_unavailable")?,
                session: expected.session_id.clone(),
                position: None,
            },
            _ => return Err("approval_unavailable"),
        };
        // Captures only the original PTY's stored birth identity here. Kernel reads happen
        // in prepare/poll on the existing background path, never in the foreground drain.
        let process = if decision == ApprovalDecision::Allow && current.tool == "Bash" {
            self.live
                .get(&expected.node_id)
                .and_then(|live| live.child_process_observer())
        } else {
            None
        };
        let p = self
            .mobile_runs
            .get_mut(&expected.node_id)
            .filter(|r| r.native)
            .and_then(|r| r.native_approvals.permission.as_mut())
            .ok_or("approval_changed")?;
        if p.claimed || p.settled || !p.active.load(Ordering::Acquire) {
            return Err("approval_changed");
        }
        p.claimed = true;
        Ok(MobileNativeApproval {
            target: expected.clone(),
            decision,
            tool_id: p.tool_id.clone(),
            history,
            prepared: false,
            active: p.active.clone(),
            executed: p.executed.clone(),
            process,
            command: p.input["command"].as_str().unwrap_or_default().to_owned(),
            // 20s Provider confirmation + persistence/receipt reserve within the existing 30s command bound.
            deadline: Instant::now() + std::time::Duration::from_secs(20),
            sent: false,
            confirmed: false,
            interrupted: false,
        })
    }

    fn matches_native_approval(&self, request: &MobileNativeApproval) -> bool {
        self.mobile_runs
            .get(&request.target.node_id)
            .is_some_and(|r| {
                r.native
                    && r.run_id.as_deref() == Some(&request.target.run_id)
                    && r.session_id.as_deref() == Some(&request.target.session_id)
                    && r.latest_prompt_id.as_deref() == Some(&request.target.turn_id)
                    && r.native_approvals.permission.as_ref().is_some_and(|p| {
                        p.id == request.target.request_id && Arc::ptr_eq(&p.active, &request.active)
                    })
            })
    }

    pub fn send_native_approval(
        &mut self,
        request: &mut MobileNativeApproval,
    ) -> Result<(), String> {
        if self.mobile_terminal_is_controlled(request.target.node_id) {
            return Err("terminal_controlled".into());
        }
        request.check()?;
        if request.sent || !request.prepared || !self.matches_native_approval(request) {
            return Err("approval_changed".into());
        }
        let run = &self.mobile_runs[&request.target.node_id];
        let p = run
            .native_approvals
            .permission
            .as_ref()
            .ok_or("approval_changed")?;
        if p.settled
            || run.message.is_some()
            || run.interrupt.is_some()
            || run.prompt_id.as_deref() != Some(&request.target.turn_id)
        {
            return Err("approval_changed".into());
        }
        let (allow, deny) = self
            .native_dialog_keys(request.target.node_id, &p.tool, &p.input)
            .ok_or("approval_changed")?;
        self.mobile_runs
            .get_mut(&request.target.node_id)
            .expect("bound run")
            .native_input
            .advance();
        request.sent = true;
        // The verified menu shortcut selects the named action regardless of the previous cursor.
        // It is not desktop draft input and must not enter the normal text-submission tracker.
        self.live
            .get_mut(&request.target.node_id)
            .ok_or("approval_changed")?
            .input(crate::NodeInput::Key(terminal::KeyInput {
                key: if request.decision == ApprovalDecision::Allow {
                    allow
                } else {
                    deny
                },
                ctrl: false,
                alt: false,
                shift: false,
            }))
            .map_err(|e| e.to_string())
    }

    /// Called after the App durably records the verified native outcome.
    pub fn finish_native_approval(&mut self, request: &MobileNativeApproval) -> bool {
        if request.check().is_err()
            || !request.sent
            || !request.complete()
            || !self.matches_native_approval(request)
        {
            return false;
        }
        let run = self
            .mobile_runs
            .get_mut(&request.target.node_id)
            .expect("matched run");
        run.native_approvals.invalidate();
        if request.decision == ApprovalDecision::Deny {
            // Both native rejection keys end this turn without a Stop Hook. Keep its PTY alive.
            run.prompt_id = None;
            self.events.update_agent_task_state(
                request.target.node_id,
                crate::AgentTaskState::Ended,
                "mobile/nativePermissionRejected".into(),
            );
        } else if matches!(
            self.agent_task_state(request.target.node_id),
            Some(crate::AgentTaskState::WaitingApproval { .. })
        ) {
            self.events.update_agent_task_state(
                request.target.node_id,
                crate::AgentTaskState::Thinking,
                "mobile/nativePermissionAccepted".into(),
            );
        }
        true
    }
}
