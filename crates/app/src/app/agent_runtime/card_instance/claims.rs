use super::*;
use crate::settings::agent_hooks::{
    AcceptedAgentHookEvent, AgentHookDrainReport, AgentHookTaskIdentity,
};
use std::collections::HashSet;

impl MultisoulApp {
    /// The production frame loop removes only exact claimed events, including duplicates of an already bound turn.
    pub(crate) fn reconcile_card_instance_hook_report(
        &mut self,
        report: &mut AgentHookDrainReport,
    ) -> bool {
        let mut changed = false;
        let mut claimed = HashSet::new();
        for event in &report.events {
            changed |= self.observe_card_instance_hook(event);
            if self.card_instance_owns_hook_turn(event) {
                claimed.insert(AgentHookTaskIdentity::from(&event.event));
            }
        }
        report.retain_unclaimed_tasks(&claimed);
        changed
    }

    fn card_instance_owns_hook_turn(&self, event: &AcceptedAgentHookEvent) -> bool {
        self.card_instance_tasks.scopes.iter().any(|(id, scope)| {
            let Some(receipt) = self.card_instance_receipt(id) else {
                return false;
            };
            let Some(turn) = receipt.bound_turn else {
                return false;
            };
            receipt.target.card.node_id == event.node_id
                && scope.route_token == self.workspace_route_id
                && event.session_id.as_deref() == Some(turn.session_id.as_str())
                && event.turn_id.as_deref() == Some(turn.turn_id.as_str())
                && event
                    .control_run_id
                    .as_ref()
                    .is_none_or(|run| turn.run_id.as_ref() == Some(run))
                && event.matches_target(&receipt.target)
        })
    }
}
