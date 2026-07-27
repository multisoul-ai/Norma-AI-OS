use super::{
    AgentAssignment, AgentCoordinatedTask, AgentCoordinationError, AgentDispatchReceipt,
    AgentDispatchSource, AgentDispatchState, AgentQueueTaskSource, AgentTaskCancellation,
    AgentTaskCoordinator, AgentTaskReferenceError, AgentTrackingMode, QueuedAgentTask,
    UnfinishedTaskLineage, MAX_UNFINISHED_DESCENDANTS,
};

#[derive(Clone)]
struct DispatchParent {
    task_ref: String,
    root_task_ref: String,
}

impl AgentTaskCoordinator {
    pub fn dispatch_from(
        &mut self,
        assignments: Vec<AgentAssignment>,
        force_untracked: bool,
        source: AgentDispatchSource,
    ) -> Result<Vec<AgentDispatchReceipt>, AgentCoordinationError> {
        let parent = self.resolve_dispatch_parent(source)?;
        let resolved = assignments
            .iter()
            .map(|assignment| {
                let index = self.preflight(assignment, force_untracked)?;
                self.validate_delegation_target(parent.as_ref(), source, index)?;
                Ok(index)
            })
            .collect::<Result<Vec<_>, AgentCoordinationError>>()?;
        self.validate_descendant_budget(parent.as_ref(), assignments.len())?;

        let mut receipts = Vec::with_capacity(assignments.len());
        for (assignment, index) in assignments.into_iter().zip(resolved) {
            let superseded = {
                let agent = &mut self.agents[index];
                (agent.roster.tracking == AgentTrackingMode::Untracked)
                    .then(|| agent.active.take())
                    .flatten()
            };
            if let Some(superseded) = superseded {
                self.retain_terminal_task(index, superseded, "interrupted");
            }
            let task_id = self.next_task_id;
            self.next_task_id = self.next_task_id.saturating_add(1);
            let task_ref = format!("{}:{task_id}", self.runtime_instance_id);
            let root_task_ref = parent
                .as_ref()
                .map(|parent| parent.root_task_ref.clone())
                .unwrap_or_else(|| task_ref.clone());
            let parent_task_ref = parent.as_ref().map(|parent| parent.task_ref.clone());
            let agent = &mut self.agents[index];
            let node_id = agent
                .roster
                .active_node_id
                .expect("preflight guarantees an active node");
            let coordinated = QueuedAgentTask {
                task_id,
                task_ref: task_ref.clone(),
                root_task_ref: root_task_ref.clone(),
                parent_task_ref: parent_task_ref.clone(),
                source_definition_id: source.definition_id,
                source_node_id: source.node_id,
                source: source.queue_source,
                target_node_id: node_id,
                task: assignment.task.clone(),
                cancel_requested: false,
            };
            let state = match (agent.roster.tracking, agent.roster.busy) {
                (AgentTrackingMode::Managed, true) => {
                    agent.queue.push_back(coordinated.clone());
                    AgentDispatchState::Queued
                }
                (AgentTrackingMode::Managed, false) => {
                    agent.roster.busy = true;
                    agent.active = Some(coordinated.clone());
                    AgentDispatchState::SubmittedManaged
                }
                (AgentTrackingMode::Untracked, _) => {
                    agent.roster.busy = true;
                    agent.active = Some(coordinated.clone());
                    AgentDispatchState::SubmittedUntracked
                }
            };
            self.unfinished_tasks.insert(
                task_ref.clone(),
                UnfinishedTaskLineage {
                    parent_task_ref: parent_task_ref.clone(),
                    source_definition_id: source.definition_id,
                    target_definition_id: agent.roster.definition_id,
                },
            );
            if parent.is_some() {
                *self
                    .unfinished_descendants
                    .entry(root_task_ref.clone())
                    .or_default() += 1;
            }
            receipts.push(AgentDispatchReceipt {
                task_id,
                task_ref,
                root_task_ref,
                parent_task_ref,
                source_definition_id: source.definition_id,
                source_node_id: source.node_id,
                definition_id: agent.roster.definition_id,
                name: agent.roster.name.clone(),
                node_id,
                task: assignment.task,
                state,
            });
        }
        Ok(receipts)
    }

    pub fn register_external_root(
        &mut self,
        definition_id: i64,
        node_id: u64,
        task: impl Into<String>,
    ) -> Result<(AgentDispatchReceipt, bool), AgentCoordinationError> {
        let index = self
            .agents
            .iter()
            .position(|agent| {
                agent.roster.definition_id == definition_id
                    && agent.roster.active_node_id == Some(node_id)
            })
            .ok_or(AgentCoordinationError::SourceAgentUnavailable)?;
        if let Some(active) = self.agents[index].active.as_ref() {
            return Ok((receipt_for_active(&self.agents[index], active), false));
        }
        let task_id = self.next_task_id;
        self.next_task_id = self.next_task_id.saturating_add(1);
        let task_ref = format!("{}:{task_id}", self.runtime_instance_id);
        let task = task.into();
        let agent = &mut self.agents[index];
        let state = match agent.roster.tracking {
            AgentTrackingMode::Managed => AgentDispatchState::SubmittedManaged,
            AgentTrackingMode::Untracked => AgentDispatchState::SubmittedUntracked,
        };
        let root = QueuedAgentTask {
            task_id,
            task_ref: task_ref.clone(),
            root_task_ref: task_ref.clone(),
            parent_task_ref: None,
            source_definition_id: None,
            source_node_id: None,
            source: AgentQueueTaskSource::Manual,
            target_node_id: node_id,
            task: task.clone(),
            cancel_requested: false,
        };
        self.unfinished_tasks.insert(
            task_ref.clone(),
            UnfinishedTaskLineage {
                parent_task_ref: None,
                source_definition_id: None,
                target_definition_id: definition_id,
            },
        );
        agent.roster.busy = true;
        agent.active = Some(root);
        Ok((
            AgentDispatchReceipt {
                task_id,
                task_ref: task_ref.clone(),
                root_task_ref: task_ref,
                parent_task_ref: None,
                source_definition_id: None,
                source_node_id: None,
                definition_id,
                name: agent.roster.name.clone(),
                node_id,
                task,
                state,
            },
            true,
        ))
    }

    pub fn runtime_instance_id(&self) -> &str {
        &self.runtime_instance_id
    }

    pub fn task_by_ref(
        &self,
        task_ref: &str,
    ) -> Result<AgentCoordinatedTask, AgentTaskReferenceError> {
        let task_id = self.parse_task_ref(task_ref)?;
        self.tasks()
            .into_iter()
            .find(|task| task.task_id == task_id && task.task_ref == task_ref)
            .ok_or(AgentTaskReferenceError::NotFound)
    }

    pub fn cancel_task_ref(
        &mut self,
        task_ref: &str,
    ) -> Result<AgentTaskCancellation, AgentTaskReferenceError> {
        let task_id = self.parse_task_ref(task_ref)?;
        let cancellation = self.cancel_task(task_id);
        if cancellation == AgentTaskCancellation::NotFound {
            Err(AgentTaskReferenceError::NotFound)
        } else {
            Ok(cancellation)
        }
    }

    fn resolve_dispatch_parent(
        &self,
        source: AgentDispatchSource,
    ) -> Result<Option<DispatchParent>, AgentCoordinationError> {
        match (source.definition_id, source.node_id) {
            (None, None) => Ok(None),
            (Some(definition_id), Some(node_id)) => self
                .agents
                .iter()
                .find(|agent| {
                    agent.roster.definition_id == definition_id
                        && agent.roster.active_node_id == Some(node_id)
                })
                .and_then(|agent| agent.active.as_ref())
                .map(|task| {
                    Some(DispatchParent {
                        task_ref: task.task_ref.clone(),
                        root_task_ref: task.root_task_ref.clone(),
                    })
                })
                .ok_or(AgentCoordinationError::SourceAgentUnavailable),
            _ => Err(AgentCoordinationError::SourceAgentUnavailable),
        }
    }

    fn validate_delegation_target(
        &self,
        parent: Option<&DispatchParent>,
        source: AgentDispatchSource,
        target_index: usize,
    ) -> Result<(), AgentCoordinationError> {
        let target_definition_id = self.agents[target_index].roster.definition_id;
        if source.definition_id == Some(target_definition_id) {
            return Err(AgentCoordinationError::SelfDispatchForbidden {
                definition_id: target_definition_id,
            });
        }
        let Some(parent) = parent else {
            return Ok(());
        };
        let mut cursor = Some(parent.task_ref.as_str());
        while let Some(task_ref) = cursor {
            let Some(lineage) = self.unfinished_tasks.get(task_ref) else {
                break;
            };
            if lineage.source_definition_id == Some(target_definition_id)
                || lineage.target_definition_id == target_definition_id
            {
                return Err(AgentCoordinationError::DelegationCycle {
                    target_definition_id,
                });
            }
            cursor = lineage.parent_task_ref.as_deref();
        }
        Ok(())
    }

    fn validate_descendant_budget(
        &self,
        parent: Option<&DispatchParent>,
        additional_tasks: usize,
    ) -> Result<(), AgentCoordinationError> {
        let Some(parent) = parent else {
            return Ok(());
        };
        let unfinished = self
            .unfinished_descendants
            .get(&parent.root_task_ref)
            .copied()
            .unwrap_or(0);
        if unfinished.saturating_add(additional_tasks) > MAX_UNFINISHED_DESCENDANTS {
            return Err(AgentCoordinationError::DelegationBudgetExceeded {
                root_task_ref: parent.root_task_ref.clone(),
                limit: MAX_UNFINISHED_DESCENDANTS,
            });
        }
        Ok(())
    }

    fn parse_task_ref(&self, task_ref: &str) -> Result<u64, AgentTaskReferenceError> {
        let (runtime_instance_id, task_id) = task_ref
            .rsplit_once(':')
            .ok_or(AgentTaskReferenceError::Invalid)?;
        if runtime_instance_id != self.runtime_instance_id {
            return Err(AgentTaskReferenceError::RuntimeExpired);
        }
        task_id
            .parse::<u64>()
            .map_err(|_| AgentTaskReferenceError::Invalid)
    }
}

fn receipt_for_active(
    agent: &super::CoordinatorAgent,
    active: &QueuedAgentTask,
) -> AgentDispatchReceipt {
    AgentDispatchReceipt {
        task_id: active.task_id,
        task_ref: active.task_ref.clone(),
        root_task_ref: active.root_task_ref.clone(),
        parent_task_ref: active.parent_task_ref.clone(),
        source_definition_id: active.source_definition_id,
        source_node_id: active.source_node_id,
        definition_id: agent.roster.definition_id,
        name: agent.roster.name.clone(),
        node_id: active.target_node_id,
        task: active.task.clone(),
        state: match agent.roster.tracking {
            AgentTrackingMode::Managed => AgentDispatchState::SubmittedManaged,
            AgentTrackingMode::Untracked => AgentDispatchState::SubmittedUntracked,
        },
    }
}
