use super::*;
use crate::voice_command::VoiceCommit;

impl NormaToolRuntime {
    /// Carry the validated candidate through the existing adapters and idempotency policy.
    pub async fn submit_voice(
        &self,
        commit: &VoiceCommit,
        context: &ToolContext,
    ) -> Result<AdapterResult, AdapterError> {
        let mut context = context.clone();
        context.voice_commit = Some(Box::new(commit.clone()));
        let proposal = commit.proposal();
        self.execute(&proposal.tool, proposal.arguments.clone(), &context)
            .await
    }
}
