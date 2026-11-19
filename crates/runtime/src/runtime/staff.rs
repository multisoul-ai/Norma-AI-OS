use super::*;
use crate::NodeRunState;
use std::time::Instant;

impl Runtime {
    /// Reserve a fresh card; persistence must succeed before the caller starts its terminal.
    pub fn create_staff_worker(
        &mut self,
        profile: AgentProfile,
        alias: String,
        bounds: CanvasRect,
        dispatch_id: String,
    ) -> NodeId {
        let node = self.add_agent_node(profile, None, Some(alias), bounds);
        self.staff_dispatches.insert(node, (dispatch_id, None));
        node
    }

    /// Model chosen for a Staff worker; applied when its native terminal is prepared.
    pub fn set_staff_model(&mut self, node: NodeId, model: String) {
        if let Some(entry) = self.staff_dispatches.get_mut(&node) {
            entry.1 = Some(model);
        }
    }

    /// Append `--model`; engines without a verified flag refuse instead of silently ignoring it.
    pub(crate) fn with_staff_model(
        &self,
        node: NodeId,
        profile: canvas::AgentProfile,
        mut command: terminal::PtyCommand,
    ) -> Result<terminal::PtyCommand, terminal::PtyError> {
        let Some(model) = self
            .staff_dispatches
            .get(&node)
            .and_then(|(_, m)| m.as_deref())
        else {
            return Ok(command);
        };
        use canvas::AgentProfile::*;
        if !matches!(profile, Claude | Codex | Cursor | OpenCode | Kimi | Grok) {
            return Err(terminal::PtyError::Spawn(
                "model selection unsupported".into(),
            ));
        }
        let script = command
            .args
            .last_mut()
            .ok_or_else(|| terminal::PtyError::Spawn("empty agent command".into()))?;
        script.push_str(" --model ");
        script.push_str(&crate::session_snapshot::shell_quote(model));
        Ok(command)
    }

    pub fn staff_dispatch_id(&self, node: NodeId) -> Option<&str> {
        self.staff_dispatches.get(&node).map(|(id, _)| id.as_str())
    }

    pub fn accepts_staff_hook(&self, event: &norma::agent_hook::AgentHookEvent) -> bool {
        self.staff_dispatch_id(event.node_id)
            .is_none_or(|dispatch| {
                match event.raw_json.get("_norma_dispatch_id") {
                    Some(id) => id.as_str() == Some(dispatch),
                    // Installed pre-Staff helpers already forward the per-process run ID.
                    // Accept that exact owned process; never infer identity from node/session alone.
                    None => event.control_run_id.as_deref().is_some_and(|run| {
                        self.native_question_process_id(event.node_id) == Some(run)
                    }),
                }
            })
    }

    pub fn last_output_at(&self, node: NodeId) -> Option<Instant> {
        self.live.get(&node).and_then(|live| live.last_output_at())
    }

    pub fn terminal_has_exited(&self, node: NodeId) -> bool {
        matches!(
            self.node_run_state(node),
            Some(
                NodeRunState::Finished { .. } | NodeRunState::Error { .. } | NodeRunState::Stopped
            )
        )
    }
}
