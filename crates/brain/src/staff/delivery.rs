use super::worker_life::{self, Input};
use super::{locate_worker, TaskEventResult, TaskStore, WorkerLocate};
use crate::DeliveryPhase;
use norma::protocol::{TaskMessage, WorkerFailure, WorkerOutcome, WorkerPhase};
use std::io;

fn unconfirmed(phase: WorkerPhase) -> bool {
    matches!(phase, WorkerPhase::Failed(WorkerFailure::Unconfirmed))
}

impl TaskStore {
    pub(super) fn record_delivery(
        &self,
        workspace: &str,
        node: u64,
        dispatch: &str,
        phase: DeliveryPhase,
    ) -> io::Result<TaskEventResult> {
        let tasks = self.list(workspace)?;
        let task_id = match locate_worker(&tasks, node, Some(dispatch)) {
            WorkerLocate::Found(id) => id,
            WorkerLocate::Unowned => return Ok(TaskEventResult::Unowned),
            WorkerLocate::Ignored => return Ok(TaskEventResult::Ignored),
        };
        let Some(worker) = tasks.iter().find_map(|task| {
            task.workers
                .iter()
                .find(|worker| worker.card_id == Some(node) && worker.dispatch_id == dispatch)
        }) else {
            return Ok(TaskEventResult::Unowned);
        };
        if worker.outcome().is_some() {
            return Ok(TaskEventResult::Ignored);
        }
        let changed = match phase {
            DeliveryPhase::Accepted => !worker.delivered() || unconfirmed(worker.phase),
            DeliveryPhase::NeedsAttention => !unconfirmed(worker.phase),
            DeliveryPhase::Redeliver => worker.delivered() || unconfirmed(worker.phase),
        };
        if !changed {
            return Ok(TaskEventResult::Ignored);
        }
        self.update(workspace, &task_id, |task| {
            let Some(worker) = task.workers.iter_mut().find(|worker| {
                worker.card_id == Some(node)
                    && worker.dispatch_id == dispatch
                    && worker.outcome().is_none()
            }) else {
                return Ok(());
            };
            match phase {
                DeliveryPhase::Accepted => worker.phase = WorkerPhase::Running,
                DeliveryPhase::NeedsAttention => {
                    worker.phase = WorkerPhase::Failed(WorkerFailure::Unconfirmed);
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
                    worker.phase = WorkerPhase::Spawning;
                }
            }
            Ok(())
        })?;
        Ok(TaskEventResult::Recorded {
            task_id,
            wake: false,
        })
    }

    /// 跑起来之前的撤回记成中断，并留一条系统消息。不唤醒幕僚。
    pub(super) fn record_withdraw(
        &self,
        workspace: &str,
        node: u64,
        dispatch: &str,
    ) -> io::Result<TaskEventResult> {
        let tasks = self.list(workspace)?;
        let task_id = match locate_worker(&tasks, node, Some(dispatch)) {
            WorkerLocate::Found(id) => id,
            WorkerLocate::Unowned => return Ok(TaskEventResult::Unowned),
            WorkerLocate::Ignored => return Ok(TaskEventResult::Ignored),
        };
        let Some(worker) = tasks.iter().find_map(|task| {
            task.workers
                .iter()
                .find(|worker| worker.card_id == Some(node) && worker.dispatch_id == dispatch)
        }) else {
            return Ok(TaskEventResult::Unowned);
        };
        let (next, _) = worker_life::step(worker.phase, Input::Withdraw { close_card: false });
        if next == worker.phase {
            return Ok(TaskEventResult::Ignored);
        }
        let mut accepted = false;
        self.update(workspace, &task_id, |task| {
            accepted = task.finish_worker(
                node,
                dispatch,
                WorkerOutcome::Interrupted,
                "staff.worker_withdrawn",
            );
            Ok(())
        })?;
        Ok(if accepted {
            TaskEventResult::Recorded {
                task_id,
                wake: false,
            }
        } else {
            TaskEventResult::Ignored
        })
    }
}
