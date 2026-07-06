use super::Runtime;
use crate::nodes::term::TermLive;
use canvas::{AgentProfile, CanvasRect, NodeId};
use terminal::{PtyCommand, PtyError};

impl Runtime {
    pub fn spawn_agent_with_command(
        &mut self,
        profile: AgentProfile,
        bounds: CanvasRect,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        self.spawn_agent_with_command_and_alias(profile, None, bounds, command, cols, rows)
    }

    pub fn spawn_agent_with_command_and_alias(
        &mut self,
        profile: AgentProfile,
        alias: Option<String>,
        bounds: CanvasRect,
        command: PtyCommand,
        cols: u16,
        rows: u16,
    ) -> Result<NodeId, PtyError> {
        let sid = self.allocate_session_id();
        let live =
            TermLive::spawn_command(sid, command.with_cwd(self.workspace_root()), cols, rows)?;
        let node_id = self.add_agent_node(profile, alias, bounds);
        self.register_agent_live_node(node_id, live);
        Ok(node_id)
    }

    pub fn agent_command(profile: AgentProfile) -> PtyCommand {
        crate::nodes::agent::agent_command(profile, None)
    }

    pub fn set_test_agent_spawn_command(&mut self, command: PtyCommand) {
        self.test_agent_spawn_command = Some(command);
    }
}
