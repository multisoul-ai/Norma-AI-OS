//! Guard every provider request, including tool continuations, before network IO.
use super::*;
use rig_core::completion::{CompletionRequest, ToolDefinition};

pub(super) fn input_bytes(window_tokens: usize) -> usize {
    window_tokens.saturating_sub(2048).saturating_mul(2)
}

struct RequestBudget {
    template: CompletionRequest,
    model_name: String,
    max_bytes: usize,
}

pub(super) async fn attach<M: CompletionModel>(
    agent: &mut Agent<M>,
    model_name: String,
    max_bytes: usize,
) -> Result<(), NormaSessionError> {
    let tools: Vec<ToolDefinition> = agent
        .tool_server_handle
        .get_tool_defs(None)
        .await
        .map_err(|error| NormaSessionError::Runtime(error.to_string()))?;
    let template = CompletionRequest {
        model: None,
        preamble: agent.preamble.clone(),
        chat_history: rig_core::OneOrMany::one(Message::user("")),
        documents: agent.static_context.clone(),
        tools,
        temperature: agent.temperature,
        max_tokens: agent.max_tokens,
        tool_choice: agent.tool_choice.clone(),
        additional_params: agent.additional_params.clone(),
        output_schema: None,
    };
    agent.hooks.push(RequestBudget {
        template,
        model_name,
        max_bytes,
    });
    Ok(())
}

impl<M: CompletionModel> AgentHook<M> for RequestBudget {
    async fn on_event(&self, _: &HookContext, event: StepEvent<'_, M>) -> Flow {
        let StepEvent::CompletionCall {
            prompt, history, ..
        } = event
        else {
            return Flow::cont();
        };
        let mut request = self.template.clone();
        request.chat_history = rig_core::OneOrMany::many(
            history
                .iter()
                .cloned()
                .chain(std::iter::once(prompt.clone())),
        )
        .expect("current prompt is always present");
        // OpenAI serializes tool arguments and documents as strings. Count their
        // escaping through Rig's own wire conversion; also retain the core size
        // as a conservative floor for Ollama's structured representation.
        let core_bytes = serde_json::to_vec(&request).map(|bytes| bytes.len());
        let wire =
            openai::completion::CompletionRequest::try_from((self.model_name.clone(), request));
        let Ok(wire) = wire else {
            return Flow::terminate("ContextBudgetExceeded: request cannot be measured");
        };
        let Ok(wire_bytes) = serde_json::to_vec(&wire).map(|bytes| bytes.len()) else {
            return Flow::terminate("ContextBudgetExceeded: request cannot be serialized");
        };
        let Ok(core_bytes) = core_bytes else {
            return Flow::terminate("ContextBudgetExceeded: context cannot be serialized");
        };
        // 512 B covers streaming/provider options and document normalization wrappers.
        let required = core_bytes
            .max(wire_bytes)
            .saturating_add(512)
            .saturating_add(crate::MEMORY_TOOL_BYTES);
        if required > self.max_bytes {
            return Flow::terminate(format!(
                "ContextBudgetExceeded: complete request needs {required} bytes, available {}",
                self.max_bytes
            ));
        }
        Flow::cont()
    }
}
