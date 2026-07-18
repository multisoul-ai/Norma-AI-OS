use super::{card_task, AgentSystem};
use crate::journal::CardTaskJournal;
use crate::provider::{assistant_message, AgentProvider};
use crate::{AgentEvent, CardToolHost, ProviderRequest, TaskState};

impl<D, C, H, J> AgentSystem<D, C, H, J>
where
    D: AgentProvider,
    C: AgentProvider,
    H: CardToolHost,
    J: CardTaskJournal,
{
    pub(super) fn request_next_card_response(&mut self) {
        if let Some(completion) = self
            .active
            .as_ref()
            .and_then(card_task::delegation_completion)
        {
            self.finish_delegation(completion);
            return;
        }
        let Some(active) = self.active.as_mut() else {
            return;
        };
        active.round_count += 1;
        let request = ProviderRequest {
            messages: active.messages.clone(),
            tools: crate::card_subagent_tool_specs(),
        };
        crate::diagnostics::card_request(
            &self.project_id,
            &active.trace_id,
            active.task_id,
            active.round_count,
            &request,
        );
        let response = match self.card_provider.complete(&request) {
            Ok(response) => response,
            Err(error) => {
                self.finish_failed(error.to_string());
                return;
            }
        };
        crate::diagnostics::card_response(
            &self.project_id,
            &active.trace_id,
            active.task_id,
            active.round_count,
            active.completed_tool_calls,
            &response,
        );
        if response.tool_calls.is_empty() {
            let task_id = active.task_id;
            if active.completed_tool_calls == 0 {
                self.finish_failed("TaskIncomplete: no successful card-tool receipt".to_string());
                return;
            }
            if !active.unresolved_tool_failures.is_empty() {
                let errors = active
                    .unresolved_tool_failures
                    .iter()
                    .map(|failure| failure.error.as_str())
                    .collect::<Vec<_>>()
                    .join("; ");
                self.finish_failed(format!(
                    "TaskIncomplete: unresolved card-tool failures: {errors}"
                ));
                return;
            }
            let summary = card_task::successful_summary(active);
            self.update_task_state(task_id, TaskState::Succeeded, Some(summary.clone()), None);
            self.emit_event(AgentEvent::TaskSucceeded { task_id, summary });
            self.active = None;
            return;
        }
        active
            .messages
            .push(assistant_message(&response.text, &response.tool_calls));
        active.pending_tool_calls.extend(response.tool_calls);
    }

    fn finish_delegation(&mut self, completion: card_task::DelegationCompletion) {
        let Some(active) = self.active.as_ref() else {
            return;
        };
        let task_id = active.task_id;
        let trace_id = active.trace_id.clone();
        let status = match &completion {
            card_task::DelegationCompletion::Succeeded(_) => "dispatched",
            card_task::DelegationCompletion::PartiallyDispatched(_) => "partially_dispatched",
            card_task::DelegationCompletion::Failed(_) => "dispatch_failed",
        };
        tracing::info!(
            project_id = %self.project_id,
            trace_id,
            task_id,
            status,
            "deterministic Agent delegation completed"
        );
        match completion {
            card_task::DelegationCompletion::Succeeded(summary) => {
                self.update_task_state(task_id, TaskState::Succeeded, Some(summary.clone()), None);
                self.emit_event(AgentEvent::TaskSucceeded { task_id, summary });
                self.active = None;
            }
            card_task::DelegationCompletion::PartiallyDispatched(summary) => {
                self.update_task_state(
                    task_id,
                    TaskState::PartiallyDispatched,
                    Some(summary.clone()),
                    None,
                );
                self.emit_event(AgentEvent::TaskPartiallyDispatched { task_id, summary });
                self.active = None;
            }
            card_task::DelegationCompletion::Failed(reason) => self.finish_failed(reason),
        }
    }
}
