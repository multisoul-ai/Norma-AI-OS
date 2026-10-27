use super::*;

impl NormaSession {
    pub(super) fn shared_context_data(
        &mut self,
        context: &ToolContext,
        frame: Option<crate::context::ContextFrame>,
    ) -> Result<Option<String>, NormaSessionError> {
        let Some(frame) = frame else {
            return Ok(None);
        };
        let consumer = if context.input_source == crate::AgentInputSource::GptLiveVoice {
            crate::context::ContextConsumer::LivePlanner
        } else {
            crate::context::ContextConsumer::Legacy
        };
        let budget = self
            .history_char_budget
            .saturating_sub(crate::context::SHARED_CONTEXT_RULES.len());
        let measure = |data: &str| {
            serde_json::to_string(data)
                .expect("context serialization")
                .len()
        };
        let data = crate::context::render_context_frame_measured(
            &frame,
            consumer,
            &context.turn_id,
            &context.user_input,
            budget,
            measure,
        )
        .map_err(|e| NormaSessionError::Runtime(e.to_string()))?;
        let bytes = measure(&data);
        self.history_char_budget = budget.saturating_sub(bytes);
        self.prune_protocol_history();
        Ok(Some(data))
    }
}
