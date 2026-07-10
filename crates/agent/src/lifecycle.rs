use crate::journal::CardTaskJournal;
use crate::{AgentError, AgentEvent, AgentProvider, AgentSystem, TaskId, TaskState};

use super::CardToolHost;

impl<D, C, H, J> AgentSystem<D, C, H, J>
where
    D: AgentProvider,
    C: AgentProvider,
    H: CardToolHost,
    J: CardTaskJournal,
{
    pub(super) fn cancel_latest(&mut self) -> Result<(), AgentError> {
        if let Some(task_id) = self.active.as_ref().map(|task| task.task_id) {
            self.cancel_task(task_id)
        } else {
            self.pending
                .back()
                .map(|task| task.task_id)
                .ok_or_else(|| AgentError::InvalidCommand("no active card task".to_string()))
                .and_then(|task_id| self.cancel_task(task_id))
        }
    }

    pub(super) fn cancel_task(&mut self, task_id: TaskId) -> Result<(), AgentError> {
        if let Some(active) = self.active.as_mut().filter(|task| task.task_id == task_id) {
            active.cancel_requested = true;
            self.emit_event(AgentEvent::TaskCancelRequested { task_id });
            return Ok(());
        }
        let Some(index) = self.pending.iter().position(|task| task.task_id == task_id) else {
            return Err(AgentError::InvalidCommand(format!(
                "unknown task {task_id}"
            )));
        };
        self.pending.remove(index);
        self.update_task_state(task_id, TaskState::Cancelled, None, None);
        self.emit_event(AgentEvent::TaskCancelled {
            task_id,
            partial_effects: false,
            completed_tool_calls: 0,
        });
        Ok(())
    }

    pub(super) fn finish_cancelled(&mut self) {
        let Some(active) = self.active.take() else {
            return;
        };
        let partial_effects = active.completed_tool_calls > 0;
        let state = if partial_effects {
            TaskState::CancelledWithPartialEffects
        } else {
            TaskState::Cancelled
        };
        self.update_task_state(active.task_id, state, None, None);
        self.emit_event(AgentEvent::TaskCancelled {
            task_id: active.task_id,
            partial_effects,
            completed_tool_calls: active.completed_tool_calls,
        });
    }

    pub(super) fn finish_failed(&mut self, reason: String) {
        let Some(active) = self.active.take() else {
            return;
        };
        let task_id = active.task_id;
        self.update_task_state(
            task_id,
            TaskState::Failed {
                reason: reason.clone(),
            },
            None,
            Some(reason.clone()),
        );
        self.emit_event(AgentEvent::TaskFailed { task_id, reason });
    }

    pub(super) fn interrupt_for_project_close(&mut self) {
        if let Some(active) = self.active.take() {
            self.update_task_state(active.task_id, TaskState::Interrupted, None, None);
            self.emit_event(AgentEvent::TaskInterrupted {
                task_id: active.task_id,
            });
        }
        while let Some(task) = self.pending.pop_front() {
            self.update_task_state(task.task_id, TaskState::Interrupted, None, None);
            self.emit_event(AgentEvent::TaskInterrupted {
                task_id: task.task_id,
            });
        }
    }

    pub(super) fn emit_status(&mut self, task_id: Option<TaskId>) {
        let records = self
            .records
            .values()
            .filter(|record| task_id.is_none() || Some(record.task_id) == task_id)
            .cloned()
            .collect::<Vec<_>>();
        for record in records {
            self.emit_event(AgentEvent::TaskStatus {
                task_id: record.task_id,
                state: record.state,
                completed_tool_calls: record.completed_tool_calls,
            });
        }
    }
}
