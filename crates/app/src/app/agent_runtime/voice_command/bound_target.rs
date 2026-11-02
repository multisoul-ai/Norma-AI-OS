use super::*;

/// Carry host-only target identity through synchronous legacy command helpers.
/// The closure cannot await; all Result exits restore the previous binding.
pub(crate) fn with_target<T>(
    app: &mut MultisoulApp,
    binding: Option<&agent::voice_command::VoiceTargetBinding>,
    execute: impl FnOnce(&mut MultisoulApp) -> T,
) -> T {
    let previous = std::mem::replace(&mut app.voice_commit_target, binding.cloned());
    let result = execute(app);
    app.voice_commit_target = previous;
    result
}

impl MultisoulApp {
    pub(crate) fn voice_bound_definition(
        &self,
        address: &str,
    ) -> Option<&storage::AgentDefinition> {
        if let Some(binding) = self
            .voice_commit_target
            .as_ref()
            .filter(|b| b.name.eq_ignore_ascii_case(address))
        {
            return self
                .agent_definitions
                .definitions()
                .iter()
                .find(|definition| definition.id == binding.definition_id);
        }
        self.agent_definitions.definition_by_address(address)
    }
    pub(crate) fn voice_bound_node(&self, definition_id: i64) -> Option<u64> {
        self.voice_commit_target.as_ref().filter(|binding| binding.definition_id == definition_id)
            .and_then(|binding| binding.node_id).filter(|id| self.runtime.document.nodes().iter().any(|node| node.id == *id
                && matches!(&node.kind, CanvasNodeKind::Agent(spec) if spec.definition_id == Some(definition_id))))
    }
}
