use super::*;
use std::sync::Mutex;

#[derive(Default)]
pub(super) struct LaunchControl {
    cancelled: AtomicBool,
    processes: Mutex<Vec<Arc<Mutex<PtyHandle>>>>,
}

/// Reuses the terminal backend; cancellation and an unclaimed prepared result both kill it.
pub(super) struct ControlledPty(Arc<Mutex<PtyHandle>>);

impl terminal::PtyBackend for ControlledPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.0.lock().expect("controlled PTY poisoned").write(bytes)
    }
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError> {
        self.0
            .lock()
            .expect("controlled PTY poisoned")
            .resize(cols, rows)
    }
    fn kill(&mut self) {
        self.0.lock().expect("controlled PTY poisoned").kill();
    }
    fn child_pid(&self) -> Option<u32> {
        terminal::PtyBackend::child_pid(&*self.0.lock().expect("controlled PTY poisoned"))
    }
    fn child_process_observer(&self) -> Option<terminal::ChildProcessObserver> {
        terminal::PtyBackend::child_process_observer(
            &*self.0.lock().expect("controlled PTY poisoned"),
        )
    }
}

impl Drop for ControlledPty {
    fn drop(&mut self) {
        self.0.lock().expect("controlled PTY poisoned").kill();
    }
}

impl LaunchControl {
    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Background only; cancellation and spawn share ownership so quit cannot miss a late fork.
    pub(super) fn spawn_pty(
        &self,
        command: PtyCommand,
        cols: u16,
        rows: u16,
        output: crossbeam_channel::Sender<terminal::PtyEvent>,
    ) -> Result<ControlledPty, PtyError> {
        let mut processes = self.processes.lock().expect("process ownership poisoned");
        // At most one server and one native terminal per controlled run.
        if self.is_cancelled() || processes.len() >= 2 {
            return Err(control_error("launch cancelled or processes already owned"));
        }
        let pty = Arc::new(Mutex::new(PtyHandle::spawn_command(
            command, cols, rows, output,
        )?));
        processes.push(pty.clone());
        Ok(ControlledPty(pty))
    }

    /// Shared by active sessions and one-shot archive reads. All diagnostics are consumed off-thread.
    pub(super) fn spawn_codex_host(
        self: &Arc<Self>,
        command: PtyCommand,
        directory: tempfile::TempDir,
    ) -> Result<(), PtyError> {
        // 64 * PTY chunk(8192) = at most512KiB queued diagnostics.
        let (sender, receiver) = crossbeam_channel::bounded(64);
        let pty = self.spawn_pty(command, 80, 24, sender)?;
        let control = self.clone();
        let worker = std::thread::Builder::new()
            .name("codex-host-output".into())
            .spawn(move || {
                while !control.is_cancelled() {
                    match receiver.recv_timeout(std::time::Duration::from_millis(10)) {
                        Ok(terminal::PtyEvent::Output(_))
                        | Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                        Ok(terminal::PtyEvent::Exited { .. })
                        | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                }
                control.cancel();
                drop(pty);
                drop(directory);
            });
        if let Err(error) = worker {
            self.cancel();
            return Err(error.into());
        }
        Ok(())
    }

    pub(super) fn cancel(&self) {
        // Keep ownership locked through kill: app quit must not interrupt another thread
        // halfway through the existing SIGSTOP → descendants → SIGKILL cleanup.
        let mut processes = self.processes.lock().expect("process ownership poisoned");
        self.cancelled.store(true, Ordering::Release);
        for pty in processes.drain(..) {
            pty.lock().expect("controlled PTY poisoned").kill();
        }
    }
}

impl MobileAgentLaunch {
    pub fn with_approval_audit(mut self, store: storage::ApprovalAuditStore) -> Self {
        self.audit_store = Some(store);
        self
    }
    pub fn with_initial_task(mut self, text: String) -> Self {
        self.initial_task = Some(text);
        self
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn record_session(&self, session_id: &str, run_id: &str) -> Result<(), PtyError> {
        self.audit_store
            .as_ref()
            .ok_or_else(|| control_error("project approval audit required"))?
            .record_session(&storage::MobileSession {
                key: storage::MobileSessionKey {
                    node_id: self.node_id,
                    profile: self.hook.profile.display_title().into(),
                    run_id: run_id.into(),
                    session_id: session_id.into(),
                },
                workspace_root: self.cwd.clone(),
                title: self.title.clone(),
            })
            .map_err(|error| control_error(&error.to_string()))
    }

    /// File IO, entropy and process launch must run off the GPUI foreground executor.
    pub fn prepare(mut self) -> Result<PreparedMobileAgent, PtyError> {
        if self.control.is_cancelled() {
            return Err(control_error("launch cancelled"));
        }
        if self.hook.profile == AgentProfile::Codex {
            return codex::CodexHost::prepare(self);
        }
        if self.initial_task.is_some() {
            return Err(control_error(
                "Claude receives its first task through the ready terminal",
            ));
        }
        if self.hook_binary == Path::new(norma::agent_hook::AGENT_HOOK_BINARY_NAME) {
            self.hook_binary = norma::agent_hook::resolve_agent_hook_command_path()
                .ok_or_else(|| control_error("permission Hook binary unavailable"))?;
        }
        let claude = claude::ClaudeApprovals::new(
            self.audit_store
                .clone()
                .ok_or_else(|| control_error("project approval audit required"))?,
        )
        .map_err(|e| control_error(&e))?;
        let run_id = fresh_control_id()?;
        let session_id = match &self.resume_session_id {
            Some(id) => id.clone(),
            None => fresh_control_id()?,
        };
        // This is a historical identity, not a restorable conversation until Claude provides it.
        // Disk failure must precede process spawn and any user input.
        self.record_session(&session_id, &run_id)?;
        let (sender, receiver) = crossbeam_channel::unbounded();
        let pty = self.control.spawn_pty(
            self.command(&session_id, &run_id),
            self.cols,
            self.rows,
            sender,
        )?;
        Ok(PreparedMobileAgent {
            launch: self,
            session_id,
            run_id,
            pty,
            receiver,
            codex: None,
            claude: Some(claude),
        })
    }

    pub(super) fn command(&self, session_id: &str, run_id: &str) -> PtyCommand {
        let hook_command = norma::agent_hook::build_hook_command(
            &self.hook_binary,
            norma::agent_hook::HookSource::Claude,
        );
        // CLI settings merge with existing user/project/managed policies; no global writes.
        // 125s = bridge timeout(120s) + local response budget(2s) + process exit margin(3s).
        let mut hooks = serde_json::Map::new();
        for event in
            norma::agent_hook::hook_events_for_source(norma::agent_hook::HookSource::Claude)
        {
            let permission = *event == "PermissionRequest";
            let command = if permission {
                format!("{hook_command} --request-approval")
            } else {
                hook_command.clone()
            };
            hooks.insert(
                (*event).into(),
                serde_json::json!([{"matcher":"*", "hooks":[{
                    "type":"command", "command":command, "timeout": if permission {125} else {5}
                }]}]),
            );
        }
        let settings = serde_json::json!({"hooks": hooks}).to_string();
        let session_flag = if self.resume_session_id.is_some() {
            "--resume"
        } else {
            "--session-id"
        };
        let command = format!(
            "exec claude --permission-mode default {session_flag} {} --settings {}",
            crate::session_snapshot::shell_quote(session_id),
            crate::session_snapshot::shell_quote(&settings)
        );
        self.hook
            .apply_to_command(
                PtyCommand::new("/bin/zsh")
                    .args(["-lic", &command])
                    .cwd(&self.cwd),
            )
            .env(ENV_CONTROL_RUN_ID, run_id)
    }
}
