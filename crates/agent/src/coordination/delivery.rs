use super::{AgentDispatchReceipt, AgentDispatchState, AgentTaskCoordinator};

impl AgentTaskCoordinator {
    /// Record actual CLI delivery; queued tasks remain only for ordered result tracking.
    pub fn mark_task_submitted(&mut self, task_id: u64) {
        for agent in &mut self.agents {
            if let Some(task) = agent
                .active
                .iter_mut()
                .chain(agent.queue.iter_mut())
                .find(|task| task.task_id == task_id)
            {
                task.submitted = true;
                return;
            }
        }
    }

    /// Continue transporting messages after Enter, without waiting for a task's completion.
    pub fn next_unsubmitted(&self, definition_id: i64) -> Option<AgentDispatchReceipt> {
        let agent = self
            .agents
            .iter()
            .find(|agent| agent.roster.definition_id == definition_id)?;
        let task = agent
            .active
            .iter()
            .chain(agent.queue.iter())
            .find(|task| !task.submitted)?;
        Some(AgentDispatchReceipt {
            task_id: task.task_id,
            task_ref: task.task_ref.clone(),
            root_task_ref: task.root_task_ref.clone(),
            parent_task_ref: task.parent_task_ref.clone(),
            source_definition_id: task.source_definition_id,
            source_node_id: task.source_node_id,
            definition_id,
            name: agent.roster.name.clone(),
            node_id: task.target_node_id,
            task: task.task.clone(),
            state: AgentDispatchState::SubmittedManaged,
        })
    }
}
