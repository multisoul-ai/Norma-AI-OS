use super::*;

impl NormaSession {
    pub(super) fn append_conversation_context(
        &mut self,
        preamble: &mut String,
        mut tails: Vec<crate::AgentConversationTail>,
    ) {
        let hints = crate::ConversationTailRenderHints {
            focused_node_id: self
                .canvas
                .cards
                .iter()
                .find(|card| card.focused)
                .map(|c| c.node_id),
            selected_node_ids: self
                .canvas
                .cards
                .iter()
                .filter(|card| card.selected)
                .map(|c| c.node_id)
                .collect(),
            ..Default::default()
        };
        tails.sort_by(|left, right| crate::compare_tails_for_context(left, right, &hints));
        while !tails.is_empty() {
            let addition = format!(
                "\n{}",
                crate::render_conversation_tails(&tails, &hints).text
            );
            // Account for outer JSON escaping without charging another pair of quotes.
            let bytes = serde_json::to_string(&addition)
                .expect("tail serialization")
                .len()
                - 2;
            if bytes <= self.history_char_budget {
                preamble.push_str(&addition);
                self.history_char_budget -= bytes;
                return;
            }
            let mut shortened = false;
            for tail in &mut tails {
                for (message, truncated) in [
                    (
                        &mut tail.last_user_message,
                        &mut tail.user_message_truncated,
                    ),
                    (
                        &mut tail.last_agent_message,
                        &mut tail.agent_message_truncated,
                    ),
                ] {
                    if let Some(text) = message.as_mut().filter(|text| !text.is_empty()) {
                        let keep = text.chars().take(crate::MAX_TAIL_MESSAGE_CHARS).count() / 2;
                        *text = text.chars().take(keep).collect();
                        *truncated = true;
                        shortened = true;
                    }
                }
            }
            if !shortened {
                tails.pop();
            }
        }
    }

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
        // Keep tool-result headroom when possible. Only required material may
        // reclaim it; optional material must not fill the whole input window.
        let mut selection_budget = budget.saturating_sub(crate::MEMORY_TOOL_BYTES);
        let data = loop {
            match crate::context::render_context_frame_measured(
                &frame,
                consumer,
                &context.turn_id,
                &context.user_input,
                selection_budget,
                measure,
            ) {
                Ok(data) => break data,
                Err(crate::context::ContextAssemblyError::BudgetExceeded {
                    required_bytes,
                    ..
                }) if required_bytes > selection_budget && required_bytes <= budget => {
                    selection_budget = required_bytes;
                }
                Err(error) => return Err(NormaSessionError::Runtime(error.to_string())),
            }
        };
        let bytes = measure(&data);
        self.history_char_budget = budget.saturating_sub(bytes);
        self.prune_protocol_history();
        Ok(Some(data))
    }
}
