use super::*;

impl Runtime {
    /// Bind the active turn, not mutable display text or its thinking/tool/approval phase.
    /// Authorization and queued Hook checks precede this nonblocking Runtime seam.
    pub fn validate_mobile_interrupt(
        &mut self,
        expected: &norma::mobile::CardSnapshot,
    ) -> Result<(), &'static str> {
        self.drain_mobile_codex();
        let current = self
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == expected.node_id)
            .ok_or("not_found")?;
        if !current.mobile_control {
            return Err("mobile_control_required");
        }
        if current.can_interrupt == Some(false) {
            return Err("desktop_control_required");
        }
        if current.profile != expected.profile
            || current.run_id != expected.run_id
            || current.session_id != expected.session_id
            || current.turn_id != expected.turn_id
        {
            return Err("card_changed");
        }
        if current.run_id.is_none()
            || current.session_id.is_none()
            || current.turn_id.is_none()
            || !matches!(
                current.state.as_str(),
                "thinking" | "running_tool" | "waiting_approval"
            )
        {
            return Err("state_not_ready");
        }
        self.validate_terminal_stop_input(expected.node_id)
    }

    pub(super) fn validate_terminal_stop_input(&self, node: NodeId) -> Result<(), &'static str> {
        if self.mobile_terminal_is_controlled(node) {
            return Err("terminal_controlled");
        }
        if matches!(self.document.node(node).map(|node| &node.kind), Some(CanvasNodeKind::Agent(spec)) if spec.profile == AgentProfile::Codex)
            && self
                .mobile_runs
                .get(&node)
                .is_some_and(|run| run.native && run.native_input.dirty)
        {
            // Ctrl-C can clear the native composer. Recheck after background preparation too.
            return Err("desktop_input_pending");
        }
        Ok(())
    }
}
