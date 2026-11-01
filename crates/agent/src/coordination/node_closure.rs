use super::{AgentDispatchReceipt, AgentTaskCoordinator};
use std::collections::VecDeque;

impl AgentTaskCoordinator {
    /// Close only work captured for this node; another instance keeps its queue.
    pub fn finish_node_with_state(
        &mut self,
        definition_id: i64,
        node_id: u64,
        state: &'static str,
    ) -> (usize, Option<AgentDispatchReceipt>) {
        let Some(index) = self
            .agents
            .iter()
            .position(|agent| agent.roster.definition_id == definition_id)
        else {
            return (0, None);
        };
        let (closed, remaining): (VecDeque<_>, VecDeque<_>) =
            std::mem::take(&mut self.agents[index].queue)
                .into_iter()
                .partition(|task| task.target_node_id == node_id);
        self.agents[index].queue = remaining;
        let mut count = closed.len();
        for task in closed {
            self.retain_terminal_task(index, task, state);
        }
        let active_matches = self.agents[index]
            .active
            .as_ref()
            .is_some_and(|task| task.target_node_id == node_id);
        let next = if active_matches {
            count += 1;
            self.finish_current_with_state(definition_id, state)
        } else {
            None
        };
        if self.agents[index].active.is_none() && self.agents[index].queue.is_empty() {
            self.agents[index].roster.busy = false;
        }
        (count, next)
    }
}
