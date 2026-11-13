use super::*;
use crate::event::{BrainStatusKind, Trigger, TriggerKind};
use crate::staff::TaskEventResult;

impl Dispatcher {
    pub(super) fn record_task(
        &mut self,
        scope: &BrainScope,
        event: &BrainEventKind,
        recorded: Option<TaskEventResult>,
    ) -> bool {
        if let BrainEventKind::TaskRequest { task_id } = event {
            self.wake_task(scope, task_id, TriggerKind::UserRequest);
            return false;
        }
        match recorded
            .map(Ok)
            .unwrap_or_else(|| self.tasks.record_event(&scope.workspace_id, event))
        {
            Ok(TaskEventResult::Unowned) => true,
            Ok(TaskEventResult::Ignored) => false,
            Ok(TaskEventResult::Recorded { task_id, wake }) => {
                if let Ok(task) = self.tasks.get(&scope.workspace_id, &task_id) {
                    let _ = self.status.send(BrainStatus {
                        scope: scope.clone(),
                        kind: BrainStatusKind::TaskChanged(task),
                    });
                }
                if wake {
                    self.wake_task(scope, &task_id, TriggerKind::AgentFinished);
                }
                // Router/triage 仍接收一次事件并负责唯一的工作空间播报。
                true
            }
            Err(error) => {
                let _ = self.status.send(BrainStatus {
                    scope: scope.clone(),
                    kind: BrainStatusKind::TaskError {
                        reason: error.to_string(),
                    },
                });
                self.retry_events.push(BrainEvent {
                    scope: scope.clone(),
                    kind: event.clone(),
                });
                false
            }
        }
    }

    fn wake_task(&self, scope: &BrainScope, id: &str, trigger: TriggerKind) {
        self.dispatch(vec![ScopedAction {
            scope: scope.clone(),
            action: Action {
                lane: Lane::Task(id.into()),
                text: String::new(),
                approval_id: None,
                trigger: Trigger::new(trigger, id),
            },
        }]);
    }
}
