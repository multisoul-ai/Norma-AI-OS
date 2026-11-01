use super::*;
use crate::voice_command::{validate_voice_proposals, VoiceCommit, VoiceFeedback, VoiceProposal};

pub(super) struct PendingVoiceCommand {
    pub input: String,
    pub proposals: Vec<VoiceProposal>,
    pub turn_id: String,
    roster: Vec<AgentRosterEntry>,
    created: std::time::Instant,
    cancellation: CancellationToken,
}
impl PendingVoiceCommand {
    pub fn is_current(&self, roster: &[AgentRosterEntry]) -> bool {
        self.created.elapsed() < std::time::Duration::from_secs(120)
            && !self.cancellation.is_cancelled()
            && self.roster == roster
    }
}
pub(super) fn feedback(reason: VoiceFeedback, tool_calls: usize) -> NormaTurnResult {
    NormaTurnResult {
        final_text: String::new(),
        tool_calls,
        voice_feedback: Some(reason),
    }
}

impl NormaSession {
    pub(super) async fn submit_voice_proposals(
        &mut self,
        proposals: Vec<VoiceProposal>,
        context: &ToolContext,
        deltas: Option<&tokio::sync::mpsc::Sender<String>>,
    ) -> Result<NormaTurnResult, NormaSessionError> {
        let commits =
            match validate_voice_proposals(&context.user_input, &self.roster, proposals.clone()) {
                Ok(commits) => commits,
                Err(reason) => {
                    if matches!(reason, VoiceFeedback::ClarifyTarget { .. }) {
                        let cancellation = CancellationToken::new();
                        let mut current = self
                            .pending_voice_cancellation
                            .lock()
                            .expect("voice draft cancellation lock");
                        if context.cancellation.is_cancelled() {
                            return Ok(feedback(VoiceFeedback::ScopeChanged, 0));
                        }
                        current.cancel();
                        *current = cancellation.clone();
                        drop(current);
                        self.pending_voice = Some(PendingVoiceCommand {
                            input: context.user_input.clone(),
                            proposals,
                            turn_id: context.turn_id.clone(),
                            roster: self.roster.clone(),
                            created: std::time::Instant::now(),
                            cancellation,
                        });
                    }
                    return Ok(feedback(reason, 0));
                }
            };
        self.commit_voice(commits, context, &context.turn_id, deltas)
            .await
    }

    pub(super) async fn commit_voice(
        &self,
        commits: Vec<VoiceCommit>,
        context: &ToolContext,
        logical_turn: &str,
        deltas: Option<&tokio::sync::mpsc::Sender<String>>,
    ) -> Result<NormaTurnResult, NormaSessionError> {
        let mut replies = Vec::new();
        for (index, commit) in commits.iter().enumerate() {
            let mut call_context = context.clone();
            call_context.operation_id = Some(format!("{logical_turn}:voice:{index}"));
            call_context.tool_call_id = call_context.operation_id.clone().unwrap_or_default();
            let result = match self.tools.submit_voice(commit, &call_context).await {
                Ok(result) => result,
                Err(crate::AdapterError::VoiceCommand(reason)) if index == 0 => {
                    return Ok(feedback(reason, 0))
                }
                Err(error) => {
                    return Ok(feedback(
                        VoiceFeedback::Submission {
                            completed: index,
                            total: commits.len(),
                            uncertain: !matches!(
                                error,
                                crate::AdapterError::VoiceCommand(_)
                                    | crate::AdapterError::Rejected(_)
                                    | crate::AdapterError::Cancelled
                            ),
                        },
                        index,
                    ))
                }
            };
            let accepted = matches!(result.status.as_str(), "accepted" | "succeeded")
                && result
                    .reply_status
                    .as_deref()
                    .is_none_or(|status| matches!(status, "sent" | "queued" | "cancelled"));
            if !accepted {
                return Ok(feedback(
                    VoiceFeedback::Submission {
                        completed: index,
                        total: commits.len(),
                        uncertain: result.status == "unknown",
                    },
                    index,
                ));
            }
            replies.push(result.message);
        }
        let final_text = replies.join("\n");
        if final_text.trim().is_empty() {
            return Ok(feedback(
                VoiceFeedback::Submission {
                    completed: commits.len(),
                    total: commits.len(),
                    uncertain: false,
                },
                commits.len(),
            ));
        }
        if let Some(sender) = deltas {
            let _ = sender.send(final_text.clone()).await;
        }
        Ok(NormaTurnResult {
            final_text,
            tool_calls: commits.len(),
            voice_feedback: None,
        })
    }
}

#[cfg(test)]
#[path = "../../tests/unit/voice_draft_cancel_tests.rs"]
mod cancellation_tests;
