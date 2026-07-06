use crate::{
    compile_card_task_steps, execute_card_task_step, CardCommandRunner, CardTask, CardTaskEvent,
    CardTaskExecutionContext, CardTaskId, CardTaskRequest, CardTaskState,
};
use std::collections::VecDeque;
use voice::CompiledVoicePlan;

pub trait CardTaskPlanner {
    fn plan(
        &mut self,
        request: &CardTaskRequest,
    ) -> Result<CompiledVoicePlan, CardTaskPlannerError>;
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CardTaskPlannerError {
    #[error("planner provider error: {message}")]
    Provider { message: String },
    #[error("planner json error: {message}")]
    Json { message: String },
    #[error("planner compile error: {message}")]
    Compile { message: String },
    #[error("planner rejected execution and kept transcript only: {reason}")]
    TranscriptOnly { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CardTaskCancelError {
    #[error("no active card task")]
    NoActiveCardTask,
}

#[derive(Debug)]
struct ActiveCardTask {
    task: CardTask,
    next_step_index: usize,
    execution: CardTaskExecutionContext,
}

#[derive(Debug, Default)]
pub struct CardTaskQueue {
    next_task_id: u64,
    pending: VecDeque<CardTask>,
    active: Option<ActiveCardTask>,
    completed: Vec<CardTask>,
    events: Vec<CardTaskEvent>,
}

impl CardTaskQueue {
    pub fn enqueue(&mut self, request: CardTaskRequest) -> CardTaskId {
        let task_id = CardTaskId::new(self.next_task_id);
        self.next_task_id += 1;

        let task = CardTask::new(task_id, request);
        self.events.push(CardTaskEvent::Queued {
            task_id,
            summary: task.summary.clone(),
        });
        self.pending.push_back(task);
        task_id
    }

    pub fn drain_events(&mut self) -> Vec<CardTaskEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn pending_tasks(&self) -> &VecDeque<CardTask> {
        &self.pending
    }

    pub fn active_task(&self) -> Option<&CardTask> {
        self.active.as_ref().map(|active| &active.task)
    }

    pub fn completed_tasks(&self) -> &[CardTask] {
        &self.completed
    }

    pub fn cancel_latest(&mut self) -> Result<CardTaskEvent, CardTaskCancelError> {
        if let Some(mut task) = self.pending.pop_back() {
            let event = cancel_task(&mut task, "cancelled queued task");
            self.completed.push(task);
            return Ok(event);
        }

        if let Some(active) = self.active.as_mut() {
            active.task.cancel_requested = true;
            return Ok(CardTaskEvent::Cancelled {
                task_id: active.task.task_id,
                message: "cancelled active task".to_string(),
            });
        }

        Err(CardTaskCancelError::NoActiveCardTask)
    }

    pub fn run_next(
        &mut self,
        planner: &mut dyn CardTaskPlanner,
        runner: &mut dyn CardCommandRunner,
    ) -> Vec<CardTaskEvent> {
        let mut events = Vec::new();

        if self.active.is_none() {
            let Some(mut task) = self.pending.pop_front() else {
                return events;
            };

            task.state = CardTaskState::Planning;
            events.push(CardTaskEvent::Planning {
                task_id: task.task_id,
            });

            match planner.plan(&task.request) {
                Ok(compiled) => {
                    task.steps = compile_card_task_steps(&compiled);
                    events.push(CardTaskEvent::Planned {
                        task_id: task.task_id,
                        step_count: task.steps.len(),
                    });
                    self.active = Some(ActiveCardTask {
                        task,
                        next_step_index: 0,
                        execution: CardTaskExecutionContext::default(),
                    });
                }
                Err(error) => {
                    let message = error.to_string();
                    task.state = CardTaskState::Failed {
                        message: message.clone(),
                    };
                    events.push(CardTaskEvent::Failed {
                        task_id: task.task_id,
                        message,
                    });
                    self.completed.push(task);
                    return events;
                }
            }
        }

        let Some(mut active) = self.active.take() else {
            return events;
        };

        if active.task.cancel_requested {
            let event = cancel_task(&mut active.task, "cancelled active task");
            events.push(event);
            self.completed.push(active.task);
            return events;
        }

        if active.next_step_index >= active.task.steps.len() {
            let task_id = active.task.task_id;
            let summary = active.task.summary.clone();
            active.task.state = CardTaskState::Succeeded;
            events.push(CardTaskEvent::Succeeded { task_id, summary });
            self.completed.push(active.task);
            return events;
        }

        let step_index = active.next_step_index;
        active.task.state = CardTaskState::Executing { step_index };
        events.push(CardTaskEvent::Executing {
            task_id: active.task.task_id,
            step_index,
        });

        let step = active.task.steps[step_index].clone();
        let total_steps = active.task.steps.len();
        match execute_card_task_step(
            step_index,
            &step,
            total_steps,
            &mut active.execution,
            runner,
        ) {
            Ok(_) => {
                events.push(CardTaskEvent::StepSucceeded {
                    task_id: active.task.task_id,
                    step_index,
                });
                active.next_step_index += 1;
                if active.next_step_index >= total_steps {
                    let task_id = active.task.task_id;
                    let summary = active.task.summary.clone();
                    active.task.state = CardTaskState::Succeeded;
                    events.push(CardTaskEvent::Succeeded { task_id, summary });
                    self.completed.push(active.task);
                } else {
                    active.task.state = CardTaskState::Executing {
                        step_index: active.next_step_index,
                    };
                    self.active = Some(active);
                }
            }
            Err(error) => {
                let message = error.to_string();
                events.push(CardTaskEvent::StepFailed {
                    task_id: active.task.task_id,
                    step_index,
                    message: message.clone(),
                });
                active.task.state = CardTaskState::Failed {
                    message: message.clone(),
                };
                events.push(CardTaskEvent::Failed {
                    task_id: active.task.task_id,
                    message,
                });
                self.completed.push(active.task);
            }
        }

        events
    }
}

fn cancel_task(task: &mut CardTask, message: &str) -> CardTaskEvent {
    task.state = CardTaskState::Cancelled {
        message: message.to_string(),
    };
    CardTaskEvent::Cancelled {
        task_id: task.task_id,
        message: message.to_string(),
    }
}
