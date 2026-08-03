use super::{
    mark_task_terminal_maps, AgentCoordinatedTask, AgentCoordinationError, AgentDispatchReceipt,
    AgentDispatchState, AgentQueueSnapshot, AgentQueueTask, AgentRosterEntry,
    AgentTaskCancellation, AgentTaskCoordinator,
};
use norma::protocol::AgentTaskResult;
use std::collections::HashMap;

impl AgentTaskCoordinator {
    pub fn sync_roster(
        &mut self,
        roster: Vec<AgentRosterEntry>,
    ) -> Result<(), AgentCoordinationError> {
        let runtime_instance_id = self.runtime_instance_id.clone();
        let next_task_id = self.next_task_id;
        let completed = std::mem::take(&mut self.completed);
        let unfinished_tasks = std::mem::take(&mut self.unfinished_tasks);
        let unfinished_descendants = std::mem::take(&mut self.unfinished_descendants);
        let mut previous = self
            .agents
            .drain(..)
            .map(|agent| {
                (
                    agent.roster.definition_id,
                    (agent.active, agent.queue, agent.roster.busy),
                )
            })
            .collect::<HashMap<_, _>>();
        let mut replacement = Self::new_with_runtime_id(runtime_instance_id, roster)?;
        replacement.next_task_id = next_task_id;
        replacement.completed = completed;
        replacement.unfinished_tasks = unfinished_tasks;
        replacement.unfinished_descendants = unfinished_descendants;
        for agent in &mut replacement.agents {
            if let Some((active, queue, was_busy)) = previous.remove(&agent.roster.definition_id) {
                agent.active = active;
                agent.queue = queue;
                agent.roster.busy |= was_busy;
            }
        }
        *self = replacement;
        Ok(())
    }

    pub fn finish_current(&mut self, definition_id: i64) -> Option<AgentDispatchReceipt> {
        self.finish_current_with_state(definition_id, "succeeded")
    }

    pub fn finish_current_with_state(
        &mut self,
        definition_id: i64,
        terminal_state: &'static str,
    ) -> Option<AgentDispatchReceipt> {
        self.finish_current_with_result(definition_id, terminal_state, None)
    }

    pub fn finish_current_with_result(
        &mut self,
        definition_id: i64,
        terminal_state: &'static str,
        result: Option<AgentTaskResult>,
    ) -> Option<AgentDispatchReceipt> {
        let agent = self
            .agents
            .iter_mut()
            .find(|agent| agent.roster.definition_id == definition_id)?;
        let node_id = agent.roster.active_node_id?;
        if let Some(completed) = agent.active.take() {
            mark_task_terminal_maps(
                &mut self.unfinished_tasks,
                &mut self.unfinished_descendants,
                &completed,
            );
            self.completed.push_back(AgentCoordinatedTask {
                task_id: completed.task_id,
                task_ref: completed.task_ref,
                root_task_ref: completed.root_task_ref,
                parent_task_ref: completed.parent_task_ref,
                source_definition_id: completed.source_definition_id,
                source_node_id: completed.source_node_id,
                definition_id,
                target_node_id: completed.target_node_id,
                agent: agent.roster.name.clone(),
                task: completed.task,
                state: terminal_state,
                tracking: agent.roster.tracking,
                result,
            });
            trim_completed(&mut self.completed);
        }
        if let Some(next) = agent.queue.pop_front() {
            agent.roster.busy = true;
            agent.active = Some(next.clone());
            Some(AgentDispatchReceipt {
                task_id: next.task_id,
                task_ref: next.task_ref.clone(),
                root_task_ref: next.root_task_ref.clone(),
                parent_task_ref: next.parent_task_ref.clone(),
                source_definition_id: next.source_definition_id,
                source_node_id: next.source_node_id,
                definition_id,
                name: agent.roster.name.clone(),
                node_id,
                task: next.task,
                state: AgentDispatchState::SubmittedManaged,
            })
        } else {
            agent.roster.busy = false;
            None
        }
    }

    pub fn finish_all_with_state(
        &mut self,
        definition_id: i64,
        terminal_state: &'static str,
    ) -> Option<(bool, usize)> {
        let agent = self
            .agents
            .iter_mut()
            .find(|agent| agent.roster.definition_id == definition_id)?;
        let was_busy = agent.roster.busy;
        let queued_count = agent.queue.len();
        let mut terminal_tasks = agent.queue.drain(..).collect::<Vec<_>>();
        if let Some(active) = agent.active.take() {
            terminal_tasks.push(active);
        }
        agent.roster.busy = false;
        for task in terminal_tasks {
            let completed = coordinated_task(
                &task,
                agent.roster.definition_id,
                &agent.roster.name,
                terminal_state,
                agent.roster.tracking,
            );
            mark_task_terminal_maps(
                &mut self.unfinished_tasks,
                &mut self.unfinished_descendants,
                &task,
            );
            self.completed.push_back(completed);
        }
        trim_completed(&mut self.completed);
        Some((was_busy, queued_count))
    }

    pub fn reject_dispatch(&mut self, definition_id: i64, task_ids: &[u64]) -> bool {
        let Some(index) = self
            .agents
            .iter()
            .position(|agent| agent.roster.definition_id == definition_id)
        else {
            return false;
        };
        let mut rejected = {
            let agent = &mut self.agents[index];
            let mut rejected = Vec::new();
            agent.queue.retain(|queued| {
                let remove = task_ids.contains(&queued.task_id);
                if remove {
                    rejected.push(queued.clone());
                }
                !remove
            });
            if agent
                .active
                .as_ref()
                .is_some_and(|active| task_ids.is_empty() || task_ids.contains(&active.task_id))
            {
                if let Some(active) = agent.active.take() {
                    rejected.push(active);
                }
            }
            agent.roster.busy = agent.active.is_some() || !agent.queue.is_empty();
            rejected
        };
        for task in rejected.drain(..) {
            self.retain_terminal_task(index, task, "failed");
        }
        true
    }

    pub fn fail_active_preserving_busy(&mut self, definition_id: i64, task_id: u64) -> bool {
        let Some(index) = self
            .agents
            .iter()
            .position(|agent| agent.roster.definition_id == definition_id)
        else {
            return false;
        };
        let failed = {
            let agent = &mut self.agents[index];
            if !agent
                .active
                .as_ref()
                .is_some_and(|active| active.task_id == task_id)
            {
                return false;
            }
            agent.roster.busy = true;
            agent.active.take()
        };
        if let Some(failed) = failed {
            self.retain_terminal_task(index, failed, "failed");
        }
        true
    }

    pub(super) fn retain_terminal_task(
        &mut self,
        agent_index: usize,
        task: super::QueuedAgentTask,
        terminal_state: &'static str,
    ) {
        let agent = &self.agents[agent_index];
        let completed = coordinated_task(
            &task,
            agent.roster.definition_id,
            &agent.roster.name,
            terminal_state,
            agent.roster.tracking,
        );
        mark_task_terminal_maps(
            &mut self.unfinished_tasks,
            &mut self.unfinished_descendants,
            &task,
        );
        self.completed.push_back(completed);
        trim_completed(&mut self.completed);
    }

    pub fn queued_task_count(&self, definition_id: i64) -> usize {
        self.agents
            .iter()
            .find(|agent| agent.roster.definition_id == definition_id)
            .map(|agent| agent.queue.len())
            .unwrap_or(0)
    }

    pub fn queue_snapshot(&self, definition_id: i64) -> Option<AgentQueueSnapshot> {
        let agent = self
            .agents
            .iter()
            .find(|agent| agent.roster.definition_id == definition_id)?;
        Some(AgentQueueSnapshot {
            current: agent.active.as_ref().map(queue_task),
            waiting: agent.queue.iter().map(queue_task).collect(),
        })
    }

    pub fn is_busy(&self, definition_id: i64) -> bool {
        self.agents
            .iter()
            .find(|agent| agent.roster.definition_id == definition_id)
            .map(|agent| agent.roster.busy)
            .unwrap_or(false)
    }

    pub fn is_cancel_requested(&self, definition_id: i64) -> bool {
        self.agents
            .iter()
            .find(|agent| agent.roster.definition_id == definition_id)
            .and_then(|agent| agent.active.as_ref())
            .is_some_and(|active| active.cancel_requested)
    }

    pub fn cancel_all(&mut self, definition_id: i64) -> Option<(bool, usize)> {
        let agent = self
            .agents
            .iter_mut()
            .find(|agent| agent.roster.definition_id == definition_id)?;
        let was_busy = agent.roster.busy;
        let cancelled_queued = agent.queue.len();
        let mut cancelled = agent.queue.drain(..).collect::<Vec<_>>();
        if let Some(active) = agent.active.take() {
            cancelled.push(active);
        }
        agent.roster.busy = false;
        for task in cancelled {
            mark_task_terminal_maps(
                &mut self.unfinished_tasks,
                &mut self.unfinished_descendants,
                &task,
            );
        }
        Some((was_busy, cancelled_queued))
    }

    pub fn cancel_task(&mut self, task_id: u64) -> AgentTaskCancellation {
        for agent in &mut self.agents {
            if let Some(index) = agent
                .queue
                .iter()
                .position(|queued| queued.task_id == task_id)
            {
                let cancelled = agent
                    .queue
                    .remove(index)
                    .expect("queued task index was found immediately above");
                mark_task_terminal_maps(
                    &mut self.unfinished_tasks,
                    &mut self.unfinished_descendants,
                    &cancelled,
                );
                self.completed.push_back(AgentCoordinatedTask {
                    task_id: cancelled.task_id,
                    task_ref: cancelled.task_ref,
                    root_task_ref: cancelled.root_task_ref,
                    parent_task_ref: cancelled.parent_task_ref,
                    source_definition_id: cancelled.source_definition_id,
                    source_node_id: cancelled.source_node_id,
                    definition_id: agent.roster.definition_id,
                    target_node_id: cancelled.target_node_id,
                    agent: agent.roster.name.clone(),
                    task: cancelled.task,
                    state: "cancelled",
                    tracking: agent.roster.tracking,
                    result: None,
                });
                trim_completed(&mut self.completed);
                return AgentTaskCancellation::Cancelled;
            }
            if let Some(active) = agent
                .active
                .as_mut()
                .filter(|active| active.task_id == task_id)
            {
                active.cancel_requested = true;
                return AgentTaskCancellation::CancelRequested;
            }
        }
        AgentTaskCancellation::NotFound
    }

    pub fn cancel_unsubmitted_task(
        &mut self,
        task_id: u64,
    ) -> (AgentTaskCancellation, Option<AgentDispatchReceipt>) {
        let definition_id = self.agents.iter().find_map(|agent| {
            agent
                .active
                .as_ref()
                .filter(|active| active.task_id == task_id)
                .map(|_| agent.roster.definition_id)
        });
        match definition_id {
            Some(definition_id) => (
                AgentTaskCancellation::Cancelled,
                self.finish_current_with_state(definition_id, "cancelled"),
            ),
            None => (AgentTaskCancellation::NotFound, None),
        }
    }

    pub fn tasks(&self) -> Vec<AgentCoordinatedTask> {
        let mut tasks = self.completed.iter().cloned().collect::<Vec<_>>();
        for agent in &self.agents {
            if let Some(active) = &agent.active {
                tasks.push(coordinated_task(
                    active,
                    agent.roster.definition_id,
                    &agent.roster.name,
                    if active.cancel_requested {
                        "cancel_requested"
                    } else {
                        "running"
                    },
                    agent.roster.tracking,
                ));
            }
            tasks.extend(agent.queue.iter().map(|queued| {
                coordinated_task(
                    queued,
                    agent.roster.definition_id,
                    &agent.roster.name,
                    "queued",
                    agent.roster.tracking,
                )
            }));
        }
        tasks.sort_by_key(|task| task.task_id);
        tasks
    }

    pub fn task_id_for(&self, definition_id: i64, task: &str) -> Option<u64> {
        self.tasks()
            .into_iter()
            .rev()
            .find(|candidate| candidate.definition_id == definition_id && candidate.task == task)
            .map(|candidate| candidate.task_id)
    }
}

fn coordinated_task(
    task: &super::QueuedAgentTask,
    definition_id: i64,
    agent: &str,
    state: &'static str,
    tracking: super::AgentTrackingMode,
) -> AgentCoordinatedTask {
    AgentCoordinatedTask {
        task_id: task.task_id,
        task_ref: task.task_ref.clone(),
        root_task_ref: task.root_task_ref.clone(),
        parent_task_ref: task.parent_task_ref.clone(),
        source_definition_id: task.source_definition_id,
        source_node_id: task.source_node_id,
        definition_id,
        target_node_id: task.target_node_id,
        agent: agent.to_string(),
        task: task.task.clone(),
        state,
        tracking,
        result: None,
    }
}

fn queue_task(task: &super::QueuedAgentTask) -> AgentQueueTask {
    AgentQueueTask {
        task_id: task.task_id,
        task_ref: task.task_ref.clone(),
        task: task.task.clone(),
        source: task.source,
    }
}

fn trim_completed(completed: &mut std::collections::VecDeque<AgentCoordinatedTask>) {
    while completed.len() > 100 {
        completed.pop_front();
    }
}
