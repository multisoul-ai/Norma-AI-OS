//! Restore existing terminal sessions and attach the same mobile observation seam.
use super::*;

impl Runtime {
    pub fn restore_live_sessions(&mut self, checkpoints: &[SessionCheckpoint]) -> usize {
        let prepared = self.prepare_question_restore();
        let mut ctx = crate::session_restore::RestoreCtx {
            question_processes: &self.questions.processes,
            pty_factory: self.pty_factory.as_ref(),
            live: &mut self.live,
            terminal_shells: &mut self.terminal_shells,
            agent_session_bindings: &self.agent_session_bindings,
            events: &mut self.events,
            next_session_id: &mut self.next_session_id,
            workspace_root: self.workspace_root.as_deref(),
            agent_hook_config: self.agent_hook_config.as_ref(),
        };
        let restored =
            crate::session_restore::restore_live_sessions(checkpoints, &self.document, &mut ctx);
        self.finish_question_restore(prepared);
        restored
    }

    pub fn save_node_sessions(&self, store: &NodeSessionStore) -> StorageResult<()> {
        crate::session_restore::save_node_sessions(
            store,
            &self.live,
            &self.document,
            self.workspace_root.as_deref(),
            &self.agent_session_bindings,
            &self.agent_launch_cwds,
        )
    }

    pub fn restore_live_sessions_with_node_sessions(
        &mut self,
        checkpoints: &[SessionCheckpoint],
        node_sessions: &[NodeSession],
    ) -> usize {
        self.hydrate_agent_session_bindings(node_sessions);
        let prepared = self.prepare_question_restore();
        let mut ctx = crate::session_restore::RestoreCtx {
            question_processes: &self.questions.processes,
            pty_factory: self.pty_factory.as_ref(),
            live: &mut self.live,
            terminal_shells: &mut self.terminal_shells,
            agent_session_bindings: &self.agent_session_bindings,
            events: &mut self.events,
            next_session_id: &mut self.next_session_id,
            workspace_root: self.workspace_root.as_deref(),
            agent_hook_config: self.agent_hook_config.as_ref(),
        };
        let restored = crate::session_restore::restore_live_sessions_with_node_sessions(
            checkpoints,
            node_sessions,
            &self.document,
            &mut ctx,
        );
        self.finish_question_restore(prepared);
        restored
    }

    pub fn restore_live_session_for_node_with_node_sessions(
        &mut self,
        node_id: NodeId,
        checkpoints: &[SessionCheckpoint],
        node_sessions: &[NodeSession],
        discovered_session_id: Option<&str>,
    ) -> bool {
        if let Some(session_id) = discovered_session_id {
            self.bind_discovered_agent_session(node_id, session_id);
        }
        if matches!(self.document.node(node_id).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if spec.mobile_control)
        {
            return self.start_native_agent_terminal(node_id, 80, 24).is_ok();
        }
        let prepared = self.prepare_question_restore();
        let mut ctx = crate::session_restore::RestoreCtx {
            question_processes: &self.questions.processes,
            pty_factory: self.pty_factory.as_ref(),
            live: &mut self.live,
            terminal_shells: &mut self.terminal_shells,
            agent_session_bindings: &self.agent_session_bindings,
            events: &mut self.events,
            next_session_id: &mut self.next_session_id,
            workspace_root: self.workspace_root.as_deref(),
            agent_hook_config: self.agent_hook_config.as_ref(),
        };
        let restored = crate::session_restore::restore_live_session_for_node_with_node_sessions(
            node_id,
            checkpoints,
            node_sessions,
            discovered_session_id,
            &self.document,
            &mut ctx,
        );
        self.finish_question_restore(prepared);
        restored
    }
}
