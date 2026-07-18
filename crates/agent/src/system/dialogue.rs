use super::dialogue_policy::DialogueRoutingDecision;
use super::AgentSystem;
use crate::journal::CardTaskJournal;
use crate::lifecycle::TaskCancelOutcome;
use crate::provider::{
    assistant_message, system_message, tool_message, user_message, AgentProvider,
};
use crate::tools::{parse_dialogue_tool_call, DialogueToolAction};
use crate::{
    dialogue_tool_specs, AgentError, AgentEvent, AgentMessage, CardToolHost, ProviderRequest,
    ProviderToolCall, CARD_TASK_TOOL, MAX_DIALOGUE_HISTORY_TURNS,
};
use std::collections::HashMap;

impl<D, C, H, J> AgentSystem<D, C, H, J>
where
    D: AgentProvider,
    C: AgentProvider,
    H: CardToolHost,
    J: CardTaskJournal,
{
    pub(super) fn handle_user_turn(&mut self, text: String) -> Result<(), AgentError> {
        if text.trim().is_empty() {
            return Err(AgentError::InvalidCommand(
                "user turn must not be empty".to_string(),
            ));
        }

        while self.dialogue_turns.len() >= MAX_DIALOGUE_HISTORY_TURNS {
            self.dialogue_turns.pop_front();
        }
        let trace_id = crate::model::new_trace_id();
        let routing_decision = super::dialogue_policy::route_user_turn(&text, &self.agent_roster);
        let delegation_targets = routing_decision
            .delegation_targets()
            .map(|targets| targets.to_vec());
        let live_event_sink = (routing_decision == DialogueRoutingDecision::ModelOnly)
            .then(|| self.live_event_sink.clone())
            .flatten();
        let mut turn = vec![user_message(&text)];
        let request = ProviderRequest {
            messages: self.dialogue_request_messages(&turn),
            tools: dialogue_tool_specs(),
        };
        crate::diagnostics::dialogue_request(&self.project_id, &trace_id, &request);
        let mut streamed = Vec::new();
        let mut response = self.dialogue_provider.stream_with_cancellation(
            &request,
            self.live_dialogue_cancellation.as_deref(),
            &mut |event| {
                if let crate::ProviderStreamEvent::TextDelta(text) = event {
                    if !text.is_empty() {
                        if let Some(sink) = &live_event_sink {
                            sink(AgentEvent::DialogueTextDelta { text: text.clone() });
                        }
                        streamed.push(text);
                    }
                }
            },
        )?;
        crate::diagnostics::dialogue_response(&self.project_id, &trace_id, &response);
        if routing_decision.requires_card_task_fallback()
            && !response.tool_calls.iter().any(|call| {
                call.name == CARD_TASK_TOOL
                    && matches!(
                        parse_dialogue_tool_call(&call.arguments_json),
                        Ok(DialogueToolAction::Start { .. })
                    )
            })
        {
            response
                .tool_calls
                .retain(|call| call.name != CARD_TASK_TOOL);
            response
                .tool_calls
                .push(self.explicit_card_task_fallback(&text));
            response.text.clear();
        }
        let actions = response
            .tool_calls
            .iter()
            .filter(|call| call.name == CARD_TASK_TOOL)
            .map(|call| {
                parse_dialogue_tool_call(&call.arguments_json)
                    .map(|action| (call.id.clone(), action))
                    .map_err(|error| AgentError::InvalidCommand(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let has_task_start = actions
            .iter()
            .any(|(_, action)| matches!(action, DialogueToolAction::Start { .. }));
        if !has_task_start {
            for text in streamed {
                self.emit_event(AgentEvent::DialogueTextDelta { text });
            }
        }
        turn.push(assistant_message(&response.text, &response.tool_calls));
        for call in response
            .tool_calls
            .iter()
            .filter(|call| call.name != CARD_TASK_TOOL)
        {
            turn.push(tool_message(
                call.id.clone(),
                format!(
                    r#"{{"status":"ignored","reason":"unsupported_tool","tool":{}}}"#,
                    serde_json::json!(call.name)
                ),
            ));
        }
        let mut turn_tool_receipts = HashMap::<String, String>::new();
        for (call_id, action) in actions {
            if let Some(receipt) = turn_tool_receipts.get(&call_id) {
                turn.push(tool_message(call_id, receipt.clone()));
                continue;
            }
            let receipt = match action {
                DialogueToolAction::Start { intent } => {
                    let assignments = delegation_targets.as_deref().and_then(|targets| {
                        super::dialogue_policy::delegation_assignments(
                            &intent,
                            targets,
                            &self.agent_roster,
                        )
                    });
                    let task_id = self.enqueue_task(trace_id.clone(), intent, assignments);
                    format!(r#"{{"status":"queued","task_id":{task_id}}}"#)
                }
                DialogueToolAction::Cancel { task_id } => match self.cancel_task(task_id) {
                    Ok(TaskCancelOutcome::CancelRequested) => {
                        format!(r#"{{"status":"cancel_requested","task_id":{task_id}}}"#)
                    }
                    Ok(TaskCancelOutcome::Cancelled) => {
                        format!(r#"{{"status":"cancelled","task_id":{task_id}}}"#)
                    }
                    Err(error) => format!(
                        r#"{{"status":"error","task_id":{task_id},"error":{}}}"#,
                        serde_json::json!(error.to_string())
                    ),
                },
                DialogueToolAction::Status { task_id } => {
                    let reported_task_id = task_id
                        .or_else(|| self.records.keys().max().copied())
                        .map_or_else(|| "null".to_string(), |id| id.to_string());
                    self.emit_status(task_id);
                    format!(r#"{{"status":"reported","task_id":{reported_task_id}}}"#)
                }
            };
            turn_tool_receipts.insert(call_id.clone(), receipt.clone());
            turn.push(tool_message(call_id, receipt));
        }
        self.dialogue_turns.push_back(turn);
        Ok(())
    }

    fn explicit_card_task_fallback(&self, intent: &str) -> ProviderToolCall {
        ProviderToolCall {
            id: format!("dialogue-policy-card-task-{}", self.next_task_id),
            name: CARD_TASK_TOOL.to_string(),
            arguments_json: serde_json::json!({
                "action": "start",
                "intent": intent,
            })
            .to_string(),
        }
    }

    fn dialogue_request_messages(&self, current_turn: &[AgentMessage]) -> Vec<AgentMessage> {
        let history_message_count = self.dialogue_turns.iter().map(Vec::len).sum::<usize>();
        let mut messages = Vec::with_capacity(1 + history_message_count + current_turn.len());
        messages.push(system_message(crate::dialogue_system_prompt_with_context(
            &self.agent_roster_prompt,
            &self.current_canvas_context_prompt,
        )));
        for turn in &self.dialogue_turns {
            messages.extend(turn.iter().cloned());
        }
        messages.extend(current_turn.iter().cloned());
        messages
    }
}
