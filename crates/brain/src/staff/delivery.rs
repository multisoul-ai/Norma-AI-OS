use super::{TaskEventResult, TaskStore};
use crate::DeliveryPhase;
use norma::protocol::{TaskMessage, WorkerState};
use std::io;

impl TaskStore {
    pub(super) fn record_delivery(
        &self,
        workspace: &str,
        node: u64,
        dispatch: &str,
        phase: DeliveryPhase,
    ) -> io::Result<TaskEventResult> {
        let tasks = self.list(workspace)?;
        let Some(task) = tasks.iter().find(|task| {
            task.workers
                .iter()
                .any(|worker| worker.card_id == Some(node))
        }) else {
            return Ok(TaskEventResult::Unowned);
        };
        let Some(worker) = task
            .workers
            .iter()
            .find(|worker| worker.card_id == Some(node) && worker.dispatch_id == dispatch)
        else {
            return Ok(TaskEventResult::Ignored);
        };
        if worker.outcome.is_some() {
            return Ok(TaskEventResult::Ignored);
        }
        let changed = match phase {
            DeliveryPhase::Accepted => {
                !worker.prompt_delivered || worker.state == WorkerState::NeedsAttention
            }
            DeliveryPhase::NeedsAttention => worker.state != WorkerState::NeedsAttention,
            DeliveryPhase::Redeliver => {
                worker.state != WorkerState::Active || worker.prompt_delivered
            }
        };
        if !changed {
            return Ok(TaskEventResult::Ignored);
        }
        self.update(workspace, &task.id, |task| {
            let Some(worker) = task.workers.iter_mut().find(|worker| {
                worker.card_id == Some(node)
                    && worker.dispatch_id == dispatch
                    && worker.outcome.is_none()
            }) else {
                return Ok(());
            };
            match phase {
                DeliveryPhase::Accepted => {
                    worker.prompt_delivered = true;
                    if worker.state == WorkerState::NeedsAttention {
                        worker.state = WorkerState::Active;
                    }
                }
                DeliveryPhase::NeedsAttention => {
                    worker.state = WorkerState::NeedsAttention;
                    worker.prompt_delivered = false;
                    let recorded = task.messages.iter().any(|message| {
                        matches!(
                            message,
                            TaskMessage::WorkerNeedsAttention { dispatch_id, .. }
                                if dispatch_id == dispatch
                        )
                    });
                    if !recorded {
                        task.messages.push(TaskMessage::WorkerNeedsAttention {
                            node_id: node,
                            dispatch_id: dispatch.to_owned(),
                        });
                    }
                }
                DeliveryPhase::Redeliver => {
                    worker.state = WorkerState::Active;
                    worker.prompt_delivered = false;
                }
            }
            Ok(())
        })?;
        Ok(TaskEventResult::Recorded {
            task_id: task.id.clone(),
            wake: false,
        })
    }
}
