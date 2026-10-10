use super::*;
use crate::settings::agent_hooks::AcceptedAgentHookEvent;
use norma::agent_hook::{extract_prompt_text, observe_conversation, ConversationObservationKind};
use norma::protocol::{AgentTaskResult, AgentTaskResultAvailability};

impl MultisoulApp {
    /// Called only for Hook events already accepted by the Runtime's provider/run/session guard.
    pub(crate) fn observe_card_instance_hook(&mut self, event: &AcceptedAgentHookEvent) -> bool {
        let candidates: Vec<_> = self
            .card_instance_tasks
            .scopes
            .iter()
            .filter_map(|(id, scope)| {
                let assignment = self.card_instance_tasks.coordinator.assignment(id)?;
                if assignment.target.card.node_id != event.node_id
                    || scope.route_token != self.workspace_route_id
                {
                    return None;
                }
                event
                    .matches_target(&assignment.target)
                    .then(|| assignment.target.clone())
            })
            .collect();
        let Some(target) = candidates.first() else {
            return false;
        };
        if event
            .control_run_id
            .as_ref()
            .is_some_and(|run| target.execution.run_id.as_ref() != Some(run))
        {
            return false;
        }
        let (Some(session), Some(turn_id)) = (event.session_id.clone(), event.turn_id.clone())
        else {
            return false;
        };
        if target.execution.session_id.as_ref() != Some(&session) {
            return false;
        }
        let turn = CardInstanceTurn {
            run_id: target.execution.run_id.clone(),
            session_id: session,
            turn_id,
        };
        if event.provider_event == "UserPromptSubmit" {
            self.card_instance_tasks
                .latest_turns
                .insert(target.card.clone(), turn.clone());
            let Some(prompt) = extract_prompt_text(&event.raw_json) else {
                return false;
            };
            // Claude expands pasted Tab into four spaces. Every other byte must agree with our submitted text.
            let text = self
                .card_instance_tasks
                .scopes
                .keys()
                .filter_map(|id| {
                    let assignment = self.card_instance_tasks.coordinator.assignment(id)?;
                    let receipt = self.card_instance_tasks.coordinator.receipt(id)?;
                    let expected = if event.profile == norma::protocol::AgentProfile::Claude {
                        assignment.text.replace('\t', "    ")
                    } else {
                        assignment.text.clone()
                    };
                    (assignment.target == *target
                        && receipt.stage == CardInstanceStage::Submitted
                        && expected == prompt)
                        .then(|| assignment.text.clone())
                })
                .collect::<Vec<_>>();
            let [text] = text.as_slice() else {
                return false;
            };
            return self
                .card_instance_tasks
                .coordinator
                .observe_start(target, text, turn);
        }
        let stage = match event.provider_event.as_str() {
            "Stop" => CardInstanceStage::Succeeded,
            "StopFailure" => CardInstanceStage::Failed,
            "SessionEnd" => CardInstanceStage::Interrupted,
            _ => return false,
        };
        let observation = observe_conversation(event);
        let result = match observation {
            Some(value)
                if value.kind == ConversationObservationKind::CompleteAgentReply
                    && value.text.is_some() =>
            {
                Some(AgentTaskResult {
                    availability: AgentTaskResultAvailability::Available,
                    text: value.text,
                    truncated: value.truncated,
                })
            }
            _ => Some(AgentTaskResult {
                availability: AgentTaskResultAvailability::Unavailable,
                text: None,
                truncated: false,
            }),
        };
        self.card_instance_tasks
            .coordinator
            .observe_terminal(target, &turn, stage, result)
    }
}
