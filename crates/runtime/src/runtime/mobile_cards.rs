use super::Runtime;
use crate::agent_hooks::{AgentHookEnv, AgentHookRuntimeConfig};
use crate::nodes::term::TermLive;
use crate::LiveNode;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, NodeId};
use norma::agent_hook::{
    fresh_control_id, ApprovalDecision, ApprovalStatus, HookApproval, ENV_CONTROL_RUN_ID,
};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use terminal::PtyError;
use terminal::{PtyCommand, PtyHandle, TerminalSession, TerminalStatus};

#[cfg(test)]
#[path = "../../tests/unit/mobile_approval_snapshot_tests.rs"]
mod approval_snapshot_tests;
mod approvals;
mod archive;
pub(crate) use archive::ArchiveReads;
pub use archive::MobileArchiveRead;
mod claude;
mod terminal_interrupt;
pub use terminal_interrupt::MobileTerminalInterrupt;
#[cfg(test)]
#[path = "../../tests/unit/mobile_claude_interrupt_tests.rs"]
mod claude_interrupt_tests;
mod codex;
pub use codex::CodexExternalApproval;
mod delete;
#[cfg(test)]
#[path = "../../tests/unit/mobile_card_delete_tests.rs"]
mod delete_tests;
mod history;
mod interrupt;
mod launch;
#[cfg(test)]
#[path = "../../tests/unit/mobile_message_tests.rs"]
mod message_tests;
mod messages;
mod native;
mod native_launch;
mod native_resume;
mod retry;
pub use native_launch::NativeAgentLaunch;
mod native_approval;
mod native_approval_action;
mod progress;
mod summary;
#[cfg(test)]
#[path = "../../tests/unit/mobile_task_progress_tests.rs"]
mod task_progress_tests;
#[cfg(test)]
#[path = "../../tests/unit/mobile_task_summary_tests.rs"]
mod task_summary_tests;
mod terminal_control;
pub use terminal_control::TerminalInputLease;
#[cfg(test)]
#[path = "../../tests/unit/mobile_terminal_tests.rs"]
mod terminal_tests;
mod terminal_view;
mod timing;
pub use native_approval_action::MobileNativeApproval;
#[cfg(test)]
#[path = "../../tests/unit/mobile_codex_readiness_tests.rs"]
mod codex_readiness_tests;
#[cfg(test)]
#[path = "../../tests/unit/mobile_native_approval_tests.rs"]
mod native_approval_tests;
#[cfg(test)]
#[path = "../../tests/unit/mobile_native_card_tests.rs"]
mod native_card_tests;
#[cfg(test)]
#[path = "../../tests/unit/mobile_native_codex_approval_tests.rs"]
mod native_codex_approval_tests;
#[cfg(test)]
#[path = "../../tests/unit/mobile_cursor_card_tests.rs"]
mod native_cursor_card_tests;
#[cfg(test)]
#[path = "../../tests/unit/mobile_native_interrupt_tests.rs"]
mod native_interrupt_tests;
mod reopen;
pub use history::MobileClaudeHistoryRead;

// Independent of Runtime/terminal counters: restoring a snapshot must not reuse a pending launch.
static NEXT_MOBILE_LAUNCH: AtomicU64 = AtomicU64::new(1);

pub(crate) struct MobileRun {
    task_steps: progress::TaskSteps,
    task_summary: summary::TaskSummary,
    task_clock: timing::TaskClock,
    native: bool,
    pub(super) native_approvals: native_approval::NativeApprovals,
    generation: u64,
    run_id: Option<String>,
    session_id: Option<String>,
    prompt_id: Option<String>,
    latest_prompt_id: Option<String>,
    retry_task: retry::RetryTask,
    claude: Option<claude::ClaudeApprovals>,
    interrupt: Option<terminal_interrupt::InterruptRequest>,
    control: Arc<launch::LaunchControl>,
    codex: Option<codex::CodexHost>,
    transcript_path: Option<PathBuf>,
    pub(super) native_input: messages::NativeInput,
    pub(super) message: Option<messages::Submission>,
}

impl Drop for MobileRun {
    fn drop(&mut self) {
        self.control.cancel();
        if let Some(claude) = &mut self.claude {
            claude.cancel();
        }
    }
}

/// Produced on the UI thread, then moved to a background executor to spawn.
pub struct MobileAgentLaunch {
    node_id: NodeId,
    title: String,
    generation: u64,
    terminal_id: u64,
    cwd: PathBuf,
    config: AgentHookRuntimeConfig,
    hook: AgentHookEnv,
    hook_binary: PathBuf,
    resume_session_id: Option<String>,
    cols: u16,
    rows: u16,
    initial_task: Option<String>,
    audit_store: Option<storage::ApprovalAuditStore>,
    control: Arc<launch::LaunchControl>,
}

/// Owns the existing PTY handle. Dropping an unclaimed result kills its process tree.
pub struct PreparedMobileAgent {
    launch: MobileAgentLaunch,
    session_id: String,
    run_id: String,
    pty: launch::ControlledPty,
    receiver: crossbeam_channel::Receiver<terminal::PtyEvent>,
    codex: Option<codex::CodexHost>,
    claude: Option<claude::ClaudeApprovals>,
}

#[cfg(test)]
#[path = "../../tests/unit/mobile_agent_launch_tests.rs"]
mod launch_tests;

#[cfg(test)]
#[path = "../../tests/unit/mobile_claude_audit_tests.rs"]
mod claude_audit_tests;

impl Runtime {
    pub fn mobile_agent_is_preparing(&self, node_id: NodeId) -> bool {
        self.mobile_runs.get(&node_id).is_some_and(|run| {
            run.run_id.is_none()
                || run
                    .native_input
                    .resume
                    .pending
                    .as_ref()
                    .is_some_and(|p| !p.ready)
                || run.message.is_some()
                || (run.native && !self.live.contains_key(&node_id))
        })
    }

    /// On-demand display projection; never reads transcripts, drains PTYs, or starts an executor.
    pub fn mobile_card_snapshots(&self) -> Vec<norma::mobile::CardSnapshot> {
        let mut cards: Vec<_> = self
            .document
            .nodes()
            .iter()
            .filter_map(|node| {
                let CanvasNodeKind::Agent(spec) = &node.kind else {
                    return None;
                };
                use crate::AgentTaskState;
                let preparing = self.mobile_agent_is_preparing(node.id);
                let interrupting = self.mobile_runs.get(&node.id).is_some_and(|run| {
                    run.interrupt.is_some()
                        || run
                            .run_id
                            .as_deref()
                            .is_some_and(|id| self.mobile_codex_interrupt_pending(node.id, id))
                });
                let state = match self.agent_task_state(node.id) {
                    _ if interrupting => "interrupting",
                    _ if preparing => "preparing",
                    Some(AgentTaskState::Ready) => "ready",
                    Some(AgentTaskState::Thinking) => "thinking",
                    Some(AgentTaskState::RunningTool { .. }) => "running_tool",
                    Some(AgentTaskState::WaitingApproval { .. }) => "waiting_approval",
                    Some(AgentTaskState::Done) => "done",
                    Some(AgentTaskState::Failed { .. }) => "failed",
                    Some(AgentTaskState::Ended) => "ended",
                    None if self.native_prompt_ready(node.id) => "ready",
                    None if self.node_run_state(node.id).is_none() => "idle",
                    None => "unknown",
                };
                Some(norma::mobile::CardSnapshot {
                    node_id: node.id,
                    profile: spec.profile.display_title().into(),
                    title: node.kind.display_title().chars().take(256).collect(),
                    mobile_control: matches!(
                        spec.profile,
                        AgentProfile::Claude | AgentProfile::Codex | AgentProfile::Cursor
                    ) || self
                        .mobile_runs
                        .get(&node.id)
                        .is_some_and(MobileRun::is_native),
                    can_interrupt: self.mobile_runs.get(&node.id).filter(|run| run.native).map(
                        |run| {
                            spec.profile == AgentProfile::Codex
                                || (spec.profile == AgentProfile::Claude
                                    && run.transcript_path.is_some())
                        },
                    ),
                    task_timing: self.mobile_runs.get(&node.id).and_then(|run| {
                        run.codex
                            .as_ref()
                            .map(|host| &host.task_clock)
                            .unwrap_or(&run.task_clock)
                            .snapshot()
                    }),
                    task_progress: self.mobile_task_progress(node.id),
                    task_summary: self.mobile_runs.get(&node.id).and_then(|run| {
                        run.codex
                            .as_ref()
                            .map(|host| &host.task_summary)
                            .unwrap_or(&run.task_summary)
                            .snapshot()
                    }),
                    state: state.into(),
                    run_id: self.mobile_agent_run_id(node.id).map(str::to_owned),
                    session_id: self.agent_session_id(node.id),
                    // Completed/failed cards retain the confirmed identity after active input clears.
                    turn_id: self.mobile_latest_turn_key(node.id).map(|key| key.turn_id),
                    task_title: self
                        .agent_task_title(node.id)
                        .map(|s| s.chars().take(1024).collect()),
                })
            })
            .collect();
        cards.sort_by_key(|card| card.node_id);
        cards
    }

    pub(super) fn end_mobile_run(&mut self, node_id: NodeId) {
        use crate::{AgentTaskState, NodeRunState};
        if self.mobile_runs.remove(&node_id).is_some()
            && !matches!(
                self.agent_task_state(node_id),
                Some(AgentTaskState::Done | AgentTaskState::Failed { .. })
            )
        {
            self.events.update_agent_task_state(
                node_id,
                match self.node_run_state(node_id) {
                    Some(NodeRunState::Error { message }) => AgentTaskState::Failed { message },
                    _ => AgentTaskState::Ended,
                },
                "controlled/sessionClosed".into(),
            );
        }
    }

    pub(super) fn end_all_mobile_runs(&mut self) {
        let ids: Vec<_> = self.mobile_runs.keys().copied().collect();
        for id in ids {
            self.end_mobile_run(id);
        }
    }

    /// Reserve one explicit launch. An idle card never calls this by itself.
    pub fn mobile_agent_launch(
        &mut self,
        node_id: NodeId,
        hook_binary: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<MobileAgentLaunch, PtyError> {
        let Some(CanvasNodeKind::Agent(spec)) = self.document.node(node_id).map(|node| &node.kind)
        else {
            return Err(control_error("agent card missing"));
        };
        if !spec.mobile_control
            || !matches!(spec.profile, AgentProfile::Claude | AgentProfile::Codex)
        {
            return Err(control_error(
                "controlled native launch requires Claude or Codex",
            ));
        }
        if self.mobile_runs.contains_key(&node_id)
            || self
                .live
                .get(&node_id)
                .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
        {
            return Err(control_error("agent already running or preparing"));
        }
        if cols == 0 || rows == 0 {
            return Err(control_error("invalid terminal size"));
        }
        let cwd = self
            .workspace_root
            .clone()
            .ok_or_else(|| control_error("workspace missing"))?;
        let config = self
            .agent_hook_config
            .clone()
            .ok_or_else(|| control_error("Hook ingress unavailable"))?;
        let hook = self
            .agent_hook_env(node_id, spec.profile)
            .ok_or_else(|| control_error("Hook context missing"))?;
        let title = self
            .document
            .node(node_id)
            .expect("validated card")
            .kind
            .display_title()
            .chars()
            .take(256)
            .collect();
        let generation = NEXT_MOBILE_LAUNCH
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map_err(|_| control_error("mobile launch identity exhausted"))?;
        let terminal_id = self.allocate_session_id();
        let control = Arc::new(launch::LaunchControl::default());
        self.mobile_runs.insert(
            node_id,
            MobileRun {
                task_clock: timing::TaskClock::default(),
                task_steps: progress::TaskSteps::default(),
                task_summary: summary::TaskSummary::default(),
                native: false,
                native_approvals: native_approval::NativeApprovals::default(),
                generation,
                run_id: None,
                session_id: None,
                prompt_id: None,
                latest_prompt_id: None,
                retry_task: retry::RetryTask::default(),
                claude: None,
                interrupt: None,
                control: control.clone(),
                codex: None,
                transcript_path: None,
                native_input: messages::NativeInput::default(),
                message: None,
            },
        );
        Ok(MobileAgentLaunch {
            node_id,
            title,
            generation,
            terminal_id,
            cwd,
            config,
            hook,
            hook_binary: hook_binary.into(),
            resume_session_id: self.agent_session_id(node_id),
            cols,
            rows,
            control,
            initial_task: None,
            audit_store: None,
        })
    }

    pub fn cancel_mobile_agent_launch(&mut self, node_id: NodeId, generation: u64) {
        if self
            .mobile_runs
            .get(&node_id)
            .is_some_and(|run| run.generation == generation && run.run_id.is_none())
        {
            self.mobile_runs.remove(&node_id);
        }
    }

    /// Adoption updates only in-memory state; all process creation has already completed.
    pub fn attach_mobile_agent(&mut self, prepared: PreparedMobileAgent) -> Result<(), PtyError> {
        let PreparedMobileAgent {
            launch,
            session_id,
            run_id,
            pty,
            receiver,
            codex,
            claude,
        } = prepared;
        let session = TerminalSession::spawn_with_backend(
            launch.terminal_id,
            launch.cols,
            launch.rows,
            Box::new(pty),
            receiver,
        );
        let node_id = launch.node_id;
        self.attach_mobile_live(launch, session_id, run_id, TermLive::from_session(session)?)?;
        self.mobile_runs
            .get_mut(&node_id)
            .expect("attached run")
            .claude = claude;
        if let Some(host) = codex {
            self.agent_session_bindings.bind_if_unclaimed(
                node_id,
                AgentProfile::Codex,
                &host.thread_id,
            );
            self.mobile_runs
                .get_mut(&node_id)
                .expect("attached run")
                .codex = Some(host);
            self.drain_mobile_codex();
        }
        Ok(())
    }

    fn attach_mobile_live(
        &mut self,
        launch: MobileAgentLaunch,
        session_id: String,
        run_id: String,
        mut live: TermLive,
    ) -> Result<(), PtyError> {
        let valid = self.mobile_runs.get(&launch.node_id).is_some_and(|run| {
            run.generation == launch.generation
                && run.run_id.is_none()
                && !run.control.is_cancelled()
        }) && self.agent_hook_config.as_ref() == Some(&launch.config)
            && self.workspace_root.as_ref() == Some(&launch.cwd)
            && matches!(self.document.node(launch.node_id).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if spec.mobile_control && spec.profile == launch.hook.profile);
        if !valid {
            live.kill();
            self.cancel_mobile_agent_launch(launch.node_id, launch.generation);
            return Err(control_error(
                "launch no longer belongs to this card/workspace",
            ));
        }
        let run = self
            .mobile_runs
            .get_mut(&launch.node_id)
            .expect("validated launch");
        run.run_id = Some(run_id);
        run.session_id = Some(session_id);
        self.register_agent_live_node(launch.node_id, live);
        Ok(())
    }

    pub(super) fn mobile_session_id(&self, node_id: NodeId) -> Option<&str> {
        self.mobile_runs.get(&node_id)?.session_id.as_deref()
    }

    pub fn cancel_mobile_approvals(&mut self, node_id: NodeId) {
        if let Some(run) = self.mobile_runs.get_mut(&node_id) {
            if let Some(claude) = &mut run.claude {
                claude.cancel();
            }
        }
    }

    /// Create metadata only. An explicit dispatch starts the controlled session;
    /// creating a card must never send an initialization prompt to a Provider.
    pub fn create_mobile_agent_card(
        &mut self,
        profile: AgentProfile,
        definition_id: Option<i64>,
        alias: Option<String>,
        bounds: CanvasRect,
    ) -> Result<NodeId, PtyError> {
        if !matches!(
            profile,
            AgentProfile::Codex | AgentProfile::Claude | AgentProfile::Cursor
        ) {
            return Err(PtyError::Spawn(
                "mobile control currently supports Codex, Claude and Cursor only".into(),
            ));
        }
        Ok(self.document.add_node(
            CanvasNodeKind::Agent(AgentSpec {
                profile,
                alias,
                definition_id,
                mobile_control: true,
            }),
            bounds,
        ))
    }
}

fn control_error(message: &str) -> PtyError {
    PtyError::Spawn(message.into())
}
