use super::*;
use crate::AgentTaskState;
use crossbeam_channel::Receiver;
use norma::codex::{CodexApproval, CodexApprovalChange, CodexApprovals, CodexConnection};
use serde_json::{json, Value};
use std::time::Duration;
mod audit;
mod control;
mod hooks;
mod interrupt;
pub use hooks::CodexExternalApproval;
#[cfg(test)]
#[path = "../../../tests/unit/mobile_codex_hook_tests.rs"]
mod hook_tests;

#[cfg(test)]
#[path = "../../../tests/unit/mobile_codex_control_tests.rs"]
pub(super) mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/mobile_codex_competition_real.rs"]
mod competition_real;
#[cfg(test)]
#[path = "../../../tests/unit/mobile_codex_approval_real.rs"]
mod real_tests;

#[cfg(test)]
#[path = "../../../tests/unit/mobile_codex_interrupt_tests.rs"]
mod interrupt_tests;
#[cfg(test)]
#[path = "../../../tests/unit/mobile_codex_message_tests.rs"]
mod message_tests;
#[cfg(test)]
#[path = "../../../tests/unit/mobile_session_launch_tests.rs"]
mod session_launch_tests;

pub(super) struct CodexHost {
    pub(super) task_clock: super::timing::TaskClock,
    pub(super) task_summary: super::summary::TaskSummary,
    connection: CodexConnection,
    pub(super) thread_id: String,
    pub(super) turn_id: Option<String>,
    pub(super) initial_turn_id: Option<String>,
    pub(super) latest_turn_id: Option<String>,
    pub(super) retry_task: super::retry::RetryTask,
    approvals: CodexApprovals,
    external_approvals: hooks::ExternalApprovals,
    audit: audit::CodexAudit,
    interrupt: Option<interrupt::InterruptRequest>,
    control: Arc<launch::LaunchControl>,
}

impl Drop for CodexHost {
    fn drop(&mut self) {
        self.control.cancel();
        if self.approvals.cancel_all().is_ok() {
            let _ = self.audit.record(self.approvals.take_changes());
        }
    }
}

impl CodexHost {
    fn native_history_pending(error: &str, thread_id: &str, new_thread: bool) -> bool {
        if error.contains("no rollout found") {
            return true;
        }
        new_thread
            && serde_json::from_str::<Value>(error)
                .ok()
                .is_some_and(|value| {
                    value["message"].as_str().is_some_and(|message| {
                        message.contains("rollout at ")
                            && message.contains(&format!("{thread_id}.jsonl"))
                            && message.ends_with(" is empty")
                    })
                })
    }
    pub(super) fn prepare(launch: MobileAgentLaunch) -> Result<PreparedMobileAgent, PtyError> {
        let task = launch.initial_task.as_deref();
        if task.is_some_and(|text| text.trim().is_empty())
            || (task.is_none() && launch.resume_session_id.is_none())
        {
            return Err(control_error(
                "Codex requires the user's real first task or an existing session",
            ));
        }
        // Bound input before starting a process or a model turn (1 MiB UTF-8 text).
        if task.is_some_and(|text| text.len() > 1024 * 1024) {
            return Err(control_error("initial task too large"));
        }
        let audit = audit::CodexAudit::new(
            launch.node_id,
            launch
                .audit_store
                .clone()
                .ok_or_else(|| control_error("project approval audit store required"))?,
        )
        .map_err(|e| control_error(&e))?;
        let directory = tempfile::Builder::new()
            .prefix("norma-codex-")
            .tempdir_in("/tmp")?;
        // A short, private path also fits macOS sockaddr_un's 104-byte limit.
        let socket_path = directory.path().join("control.sock");
        let endpoint = format!("unix://{}", socket_path.display());
        let run_id = fresh_control_id()?;
        let command = launch
            .hook
            .apply_to_command(
                PtyCommand::new("/bin/zsh")
                    .args([
                        "-lic",
                        &format!(
                            "exec codex app-server --listen {}",
                            crate::session_snapshot::shell_quote(&endpoint)
                        ),
                    ])
                    .cwd(&launch.cwd),
            )
            .env(ENV_CONTROL_RUN_ID, &run_id);
        launch.control.spawn_codex_host(command, directory)?;
        // Any prepare error must release a host even before a connection was established.
        let result = Self::connect_and_start(&launch, &socket_path, task, &run_id, audit);
        let host = match result {
            Ok(host) => host,
            Err(error) => {
                launch.control.cancel();
                return Err(error);
            }
        };
        let native = launch
            .hook
            .apply_to_command(
                PtyCommand::new("/bin/zsh")
                    .args([
                        "-lic",
                        &format!(
                            "exec codex --remote {} resume {}",
                            crate::session_snapshot::shell_quote(&endpoint),
                            crate::session_snapshot::shell_quote(&host.thread_id)
                        ),
                    ])
                    .cwd(&launch.cwd),
            )
            .env(ENV_CONTROL_RUN_ID, &run_id);
        let (sender, receiver) = crossbeam_channel::unbounded();
        let pty = launch
            .control
            .spawn_pty(native, launch.cols, launch.rows, sender)?;
        Ok(PreparedMobileAgent {
            session_id: host.thread_id.clone(),
            launch,
            run_id,
            pty,
            receiver,
            codex: Some(host),
            claude: None,
        })
    }

    fn connect_and_start(
        launch: &MobileAgentLaunch,
        path: &Path,
        task: Option<&str>,
        run_id: &str,
        audit: audit::CodexAudit,
    ) -> Result<Self, PtyError> {
        // 15s covers login-shell and CLI cold startup; cancellation is checked every 50ms.
        let deadline = Instant::now() + Duration::from_secs(15);
        let connection = loop {
            if launch.control.is_cancelled() {
                return Err(control_error("Codex launch cancelled or exited"));
            }
            if path.exists() {
                break CodexConnection::connect(path).map_err(|error| control_error(&error))?;
            }
            if Instant::now() >= deadline {
                return Err(control_error("Codex host startup timed out"));
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let mut params = json!({"cwd":launch.cwd,"sandbox":"read-only","approvalPolicy":"untrusted","approvalsReviewer":"user"});
        let method = if let Some(id) = &launch.resume_session_id {
            params["threadId"] = json!(id);
            "thread/resume"
        } else {
            "thread/start"
        };
        let thread = Self::rpc(&connection, method, params)?;
        let thread_id: String = thread["thread"]["id"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or_else(|| control_error("Codex returned no thread ID"))?
            .into();
        if launch
            .resume_session_id
            .as_ref()
            .is_some_and(|id| id != &thread_id)
        {
            return Err(control_error("Codex resumed the wrong thread"));
        }
        // Commit before turn/start: even a first task interrupted before adoption remains locatable.
        launch.record_session(&thread_id, run_id)?;
        let turn_id = if let Some(task) = task {
            let turn = Self::rpc(
                &connection,
                "turn/start",
                json!({"threadId":thread_id,"input":[{"type":"text","text":task}]}),
            )?;
            Some(
                turn["turn"]["id"]
                    .as_str()
                    .ok_or_else(|| control_error("Codex returned no turn ID"))?
                    .into(),
            )
        } else {
            None
        };
        let mut retry_task = super::retry::RetryTask::default();
        retry_task.record(turn_id.as_deref(), task);
        let host = Self {
            retry_task,
            task_clock: super::timing::TaskClock::default(),
            task_summary: super::summary::TaskSummary::default(),
            connection,
            approvals: CodexApprovals::new(run_id.into(), thread_id.clone(), turn_id.clone()),
            external_approvals: hooks::ExternalApprovals::default(),
            audit,
            interrupt: None,
            thread_id,
            initial_turn_id: turn_id.clone(),
            latest_turn_id: turn_id.clone(),
            turn_id,
            control: launch.control.clone(),
        };
        // An empty thread cannot be resumed by the native TUI. Only the actual task creates history.
        // Probe through a second connection: success proves another client can attach this rollout.
        let observer = CodexConnection::connect(path).map_err(|error| control_error(&error))?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let reply = observer
                .request("thread/resume", json!({"threadId":host.thread_id}))
                .map_err(|error| control_error(&error))?;
            match reply.recv_timeout(Duration::from_secs(5)) {
                Ok(Ok(value)) if value["thread"]["id"] == host.thread_id => break,
                Ok(Err(error))
                    if Self::native_history_pending(
                        &error,
                        &host.thread_id,
                        launch.resume_session_id.is_none(),
                    ) && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Ok(Err(error)) => return Err(control_error(&error)),
                _ => {
                    return Err(control_error(
                        "Codex native attachment could not be verified",
                    ))
                }
            }
        }
        Ok(host)
    }

    fn rpc(connection: &CodexConnection, method: &str, params: Value) -> Result<Value, PtyError> {
        connection
            .request(method, params)
            .map_err(|error| control_error(&error))?
            .recv_timeout(Duration::from_secs(30))
            .map_err(|_| control_error("Codex RPC response timed out"))?
            .map_err(|error| control_error(&error))
    }

    fn drain(&mut self, run_id: &str) -> (Vec<(AgentTaskState, String)>, bool) {
        let mut updates = Vec::new();
        let mut display_changed = false;
        if !self.connection.is_connected() || self.control.is_cancelled() {
            updates.push((
                AgentTaskState::Failed {
                    message: "Codex control connection closed".into(),
                },
                "connection/closed".into(),
            ));
            return (updates, display_changed);
        }
        for _ in 0..128 {
            let Ok(event) = self.connection.events().try_recv() else {
                break;
            };
            if let Err(error) = self
                .approvals
                .observe(&event, Instant::now())
                .and_then(|()| self.observe_interrupt(&event))
                .and_then(|()| {
                    self.external_approvals.observe(
                        &event,
                        &self.thread_id,
                        self.turn_id.as_deref(),
                    )
                })
            {
                self.control.cancel();
                updates.push((
                    AgentTaskState::Failed { message: error },
                    "approval/unavailable".into(),
                ));
                break;
            }
            let method = event["method"].as_str().unwrap_or("");
            let params = &event["params"];
            if params["threadId"] != self.thread_id {
                continue;
            }
            if matches!(method, "item/started" | "item/completed")
                && params["item"]["type"] == "userMessage"
            {
                let content = params["item"]["content"].as_array();
                let text = content
                    .filter(|items| items.len() == 1 && items[0]["type"] == "text")
                    .and_then(|items| items[0]["text"].as_str());
                self.retry_task.record(params["turnId"].as_str(), text);
            }
            display_changed |= self.task_summary.observe_codex(method, params);
            let state = match method {
                "turn/started" => {
                    self.turn_id = params["turn"]["id"].as_str().map(str::to_owned);
                    self.latest_turn_id = self.turn_id.clone();
                    self.task_clock.start(self.turn_id.as_deref());
                    Some(AgentTaskState::Thinking)
                }
                "turn/completed"
                    if self.turn_id.is_some()
                        && params["turn"]["id"].as_str() == self.turn_id.as_deref() =>
                {
                    let outcome = match params["turn"]["status"].as_str() {
                        Some("completed") => Some(storage::MobileTurnState::Completed),
                        Some("failed") => Some(storage::MobileTurnState::Failed),
                        Some("interrupted") => Some(storage::MobileTurnState::Interrupted),
                        _ => None,
                    };
                    if let Some(outcome) = outcome {
                        if let Err(message) = self.audit.record_turn(
                            run_id,
                            &self.thread_id,
                            self.turn_id.as_deref().expect("matched turn"),
                            outcome,
                            self.task_summary.snapshot(),
                        ) {
                            self.control.cancel();
                            updates.push((
                                AgentTaskState::Failed { message },
                                "turn/auditFailed".into(),
                            ));
                            break;
                        }
                    }
                    self.turn_id = None;
                    Some(match params["turn"]["status"].as_str() {
                        Some("completed") => AgentTaskState::Done,
                        Some("interrupted") => AgentTaskState::Ended,
                        _ => AgentTaskState::Failed {
                            message: params["turn"]["error"]["message"]
                                .as_str()
                                .unwrap_or("Codex turn failed")
                                .into(),
                        },
                    })
                }
                "item/commandExecution/requestApproval" | "item/fileChange/requestApproval"
                    if self.interrupt.is_none()
                        && self.turn_id.is_some()
                        && params["turnId"].as_str() == self.turn_id.as_deref() =>
                {
                    Some(AgentTaskState::WaitingApproval {
                        tool_name: Some(method.into()),
                    })
                }
                "item/started"
                    if self.turn_id.is_some()
                        && params["turnId"].as_str() == self.turn_id.as_deref() =>
                {
                    match params["item"]["type"].as_str() {
                        Some("commandExecution" | "fileChange") => {
                            Some(AgentTaskState::RunningTool {
                                tool_name: params["item"]["type"].as_str().map(str::to_owned),
                            })
                        }
                        Some("agentMessage") => Some(AgentTaskState::Thinking),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(state) = state {
                if matches!(
                    state,
                    AgentTaskState::Done | AgentTaskState::Ended | AgentTaskState::Failed { .. }
                ) {
                    self.task_clock.finish();
                }
                updates.push((state, method.into()));
            }
        }
        (updates, display_changed)
    }
}
