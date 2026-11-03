use super::single_voice::{feedback, PendingVoiceCommand};
use super::*;
use crate::voice_command::{
    clarification_choices, target_only_answer, validate_clarified_proposals, VoiceFeedback,
    VoiceProposal,
};

pub(super) enum VoiceResume {
    Done(NormaTurnResult),
    Replan {
        draft: PendingVoiceCommand,
        model_input: String,
    },
}
impl NormaSession {
    pub(super) async fn resume_voice_draft(
        &mut self,
        draft: PendingVoiceCommand,
        valid: bool,
        context: &ToolContext,
        deltas: Option<&tokio::sync::mpsc::Sender<String>>,
    ) -> Result<VoiceResume, NormaSessionError> {
        if !valid {
            return Ok(VoiceResume::Done(feedback(VoiceFeedback::ScopeChanged, 0)));
        }
        let Some(target) = target_only_answer(&context.user_input, &self.roster) else {
            self.pending_voice = Some(draft);
            return Ok(VoiceResume::Done(feedback(
                VoiceFeedback::ClarifyTarget {
                    candidates: clarification_choices(&self.roster),
                },
                0,
            )));
        };
        if draft
            .proposals
            .iter()
            .all(|p| matches!(p.tool.as_str(), "assign_agent_task" | "reply_agent"))
        {
            return self
                .submit_voice_replan(draft, context, Vec::new(), deltas)
                .await
                .map(VoiceResume::Done);
        }
        // Reuse this new user turn's existing planner, with separate data fields.
        // No ASR correction model or additional serial intent call is introduced.
        let model_input=serde_json::json!({"voice_request":{
            "original_input":draft.input,"clarification_input":context.user_input,"selected_target":target,
            "instruction":"Plan the original request for the target explicitly selected in this clarification. Do not execute any proposed write. Preserve the original constraints."
        }}).to_string();
        Ok(VoiceResume::Replan { draft, model_input })
    }

    pub(super) async fn submit_voice_replan(
        &self,
        draft: PendingVoiceCommand,
        context: &ToolContext,
        proposals: Vec<VoiceProposal>,
        deltas: Option<&tokio::sync::mpsc::Sender<String>>,
    ) -> Result<NormaTurnResult, NormaSessionError> {
        let proposals = if proposals.is_empty() {
            draft.proposals
        } else {
            proposals
        };
        let commits = match validate_clarified_proposals(
            &draft.input,
            &context.user_input,
            &self.roster,
            proposals,
        ) {
            Ok(commits) => commits,
            Err(reason) => return Ok(feedback(reason, 0)),
        };
        self.commit_voice(commits, context, &draft.turn_id, deltas)
            .await
    }
}
