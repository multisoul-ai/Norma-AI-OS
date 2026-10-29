//! Native launch preparation does no IO; the App can spawn off the foreground thread.
use super::*;

pub struct PreparedNativeAgent {
    terminal: crate::nodes::agent::PreparedAgentTerminal,
}
impl PreparedNativeAgent {
    pub fn into_live(self) -> Result<TermLive, PtyError> {
        self.terminal.into_live()
    }
}

pub struct NativeAgentLaunch {
    run_id: String,
    terminal_id: u64,
    command: PtyCommand,
    cols: u16,
    rows: u16,
    control: Arc<launch::LaunchControl>,
}
impl NativeAgentLaunch {
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn spawn(self) -> Result<PreparedNativeAgent, PtyError> {
        if self.control.is_cancelled() {
            return Err(control_error("launch cancelled"));
        }
        let terminal = crate::nodes::agent::prepare_agent_command(
            self.terminal_id,
            self.command,
            self.cols,
            self.rows,
            self.control.clone(),
        )?;
        if self.control.is_cancelled() {
            return Err(control_error("launch cancelled"));
        }
        Ok(PreparedNativeAgent { terminal })
    }

    pub(super) fn spawn_with_factory(
        self,
        factory: Option<&crate::PtyFactory>,
    ) -> Result<TermLive, PtyError> {
        if self.control.is_cancelled() {
            return Err(control_error("launch cancelled"));
        }
        let mut live = crate::nodes::agent::spawn_agent_command(
            self.terminal_id,
            self.command,
            self.cols,
            self.rows,
            factory,
            Some(self.control.clone()),
        )?;
        if self.control.is_cancelled() {
            live.kill();
            return Err(control_error("launch cancelled"));
        }
        Ok(live)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/native_launch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/unit/native_question_lifecycle_tests.rs"]
mod question_lifecycle_tests;

impl Runtime {
    /// Same TermLive and renderer as desktop cards. Opening the CLI does not submit a task.
    pub fn spawn_mobile_agent_card(
        &mut self,
        profile: AgentProfile,
        alias: Option<String>,
        bounds: CanvasRect,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        self.spawn_mobile_agent_card_with_identity(profile, None, alias, bounds, cols, rows)
    }

    /// Create a phone-controlled card that retains its project Agent definition identity.
    pub fn spawn_mobile_agent_card_for_definition(
        &mut self,
        profile: AgentProfile,
        definition_id: i64,
        alias: String,
        bounds: CanvasRect,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        if definition_id <= 0 {
            return Err(control_error("agent definition id must be positive"));
        }
        self.spawn_mobile_agent_card_with_identity(
            profile,
            Some(definition_id),
            Some(alias),
            bounds,
            cols,
            rows,
        )
    }

    fn spawn_mobile_agent_card_with_identity(
        &mut self,
        profile: AgentProfile,
        definition_id: Option<i64>,
        alias: Option<String>,
        bounds: CanvasRect,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let node = self.create_mobile_agent_card(profile, definition_id, alias, bounds)?;
        if let Err(error) = self.start_native_agent_terminal(node, cols, rows) {
            self.document.remove_node(node);
            return Err(error);
        }
        Ok(node)
    }

    /// Resume only this card, retaining its saved permission policy and session.
    pub fn start_native_agent_terminal(
        &mut self,
        node: NodeId,
        cols: u16,
        rows: u16,
    ) -> Result<(), PtyError> {
        let launch = self.prepare_native_agent_terminal(node, cols, rows)?;
        let run = launch.run_id.clone();
        let result = launch.spawn_with_factory(self.pty_factory.as_ref());
        self.complete_native_agent_terminal(node, &run, result)
    }
}

impl Runtime {
    pub fn prepare_native_agent_terminal(
        &mut self,
        node: NodeId,
        cols: u16,
        rows: u16,
    ) -> Result<NativeAgentLaunch, PtyError> {
        if self.live.get(&node).is_some_and(|live| {
            !matches!(live.terminal_status(), Some(TerminalStatus::Exited { .. }))
        }) || self.mobile_runs.contains_key(&node)
        {
            return Err(control_error("terminal already exists"));
        }
        let Some(CanvasNodeKind::Agent(spec)) = self.document.node(node).map(|node| &node.kind)
        else {
            return Err(control_error("agent missing"));
        };
        if !matches!(
            spec.profile,
            AgentProfile::Codex | AgentProfile::Claude | AgentProfile::Cursor
        ) {
            return Err(control_error("mobile Provider unavailable"));
        }
        let profile = spec.profile;
        let mobile_control = spec.mobile_control;
        self.reserve_native_run(node);
        let hook = self.agent_hook_env(node, profile);
        let session = self.agent_session_id(node);
        let command = if mobile_control {
            crate::nodes::agent::mobile_agent_command(profile, session.as_deref(), hook.as_ref())?
                .with_cwd(self.workspace_root())
        } else if let Some(session) = session.as_deref() {
            crate::session_snapshot::build_agent_resume_command(
                profile,
                crate::DEFAULT_TERMINAL_SHELL,
                session,
                hook.as_ref(),
                self.workspace_root(),
            )
        } else {
            crate::nodes::agent::agent_command(profile, hook.as_ref())
                .with_cwd(self.workspace_root())
        };
        if let Some(mut exited) = self.live.remove(&node) {
            exited.kill();
        }
        let run = self
            .mobile_runs
            .get(&node)
            .ok_or_else(|| control_error("run identity unavailable"))?;
        let run_id = run
            .run_id
            .clone()
            .ok_or_else(|| control_error("run identity unavailable"))?;
        let control = run.control.clone();
        Ok(NativeAgentLaunch {
            run_id,
            terminal_id: self.allocate_session_id(),
            command,
            cols,
            rows,
            control,
        })
    }

    /// A late spawn cannot replace a deleted, restarted or closed project/card.
    pub fn complete_native_agent_terminal(
        &mut self,
        node: NodeId,
        run_id: &str,
        result: Result<TermLive, PtyError>,
    ) -> Result<(), PtyError> {
        let current = self.mobile_runs.get(&node).is_some_and(|run| {
            run.native && run.run_id.as_deref() == Some(run_id) && !run.control.is_cancelled()
        });
        if !current || self.live.contains_key(&node) {
            if let Ok(mut live) = result {
                live.kill();
            }
            return Err(control_error("launch expired"));
        }
        match result {
            Ok(live) => {
                self.register_agent_live_node(node, live);
                Ok(())
            }
            Err(error) => {
                self.mobile_runs.remove(&node);
                Err(error)
            }
        }
    }
}
