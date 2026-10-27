use super::*;

pub(super) async fn run_turn<M>(
    model: M,
    tools: NormaToolRuntime,
    request: TurnRequest,
) -> Result<InternalTurnResult, NormaSessionError>
where
    M: CompletionModel + Clone + 'static,
    M::StreamingResponse: WasmCompatSend + WasmCompatSync + GetTokenUsage + Clone + Unpin,
{
    let TurnRequest {
        preamble,
        context_data,
        model_name,
        max_input_bytes,
        input,
        history,
        context,
        cancellation,
        deltas,
        desktop_active,
    } = request;
    let future = async move {
        let rig_context = RigToolContext::new(context);
        let unknown_status = rig_context.clone();
        let mut agent = build_agent(
            model,
            &preamble,
            tools,
            rig_context.clone(),
            desktop_active,
            context_data.as_deref(),
        );
        request_budget::attach(&mut agent, model_name, max_input_bytes).await?;
        let mut extensions = ToolCallExtensions::new();
        extensions.insert(rig_context);
        let mut stream = agent
            .stream_prompt(&input)
            .history(history)
            .tool_extensions(extensions)
            .max_turns(MAX_MODEL_TURNS)
            .tool_concurrency(1)
            .await;
        let mut tool_calls = 0usize;
        let mut pending_raw = String::new();
        let mut pending_visible = String::new();
        let mut pending_deltas = Vec::new();
        while let Some(item) = stream.next().await {
            if unknown_status.unknown_seen() {
                return Err(NormaSessionError::UnknownSideEffect);
            }
            if unknown_status.failure_budget_exhausted() {
                return Err(NormaSessionError::ToolFailureBudgetExceeded);
            }
            let item = item.map_err(|error| NormaSessionError::Runtime(error.to_string()))?;
            match item {
                MultiTurnStreamItem::ToolExecutionStart { .. } => {
                    pending_raw.clear();
                    pending_visible.clear();
                    pending_deltas.clear();
                    tool_calls = tool_calls.saturating_add(1);
                    if tool_calls > MAX_TOOL_CALLS {
                        return Err(NormaSessionError::ToolBudgetExceeded);
                    }
                }
                MultiTurnStreamItem::FinalResponse(response) => {
                    let final_visible = visible_norma_reply(&response.output);
                    if let Some(sender) = &deltas {
                        for delta in
                            finalize_stream_deltas(&final_visible, &pending_visible, pending_deltas)
                        {
                            let _ = sender.send(delta).await;
                        }
                    }
                    return Ok(InternalTurnResult {
                        final_text: response.output,
                        tool_calls,
                        new_messages: response.messages.unwrap_or_default(),
                    });
                }
                MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::Text(text)) => {
                    pending_raw.push_str(&text.text);
                    let next_visible = visible_stream_snapshot(&pending_raw).trim().to_string();
                    if let Some(delta) = next_visible.strip_prefix(&pending_visible) {
                        if !delta.is_empty() {
                            pending_deltas.push(delta.to_string());
                            pending_visible = next_visible;
                        }
                    }
                }
                MultiTurnStreamItem::StreamAssistantItem(StreamedAssistantContent::ToolCall {
                    tool_call,
                    internal_call_id,
                }) => {
                    // Rig's OpenAI hook omits provider IDs; the completed stream item precedes it.
                    unknown_status.record_streamed_call(&tool_call.id, &internal_call_id);
                }
                MultiTurnStreamItem::StreamAssistantItem(_)
                | MultiTurnStreamItem::StreamUserItem(_)
                | MultiTurnStreamItem::CompletionCall(_) => {}
                _ => {}
            }
        }
        if unknown_status.unknown_seen() {
            return Err(NormaSessionError::UnknownSideEffect);
        }
        if unknown_status.failure_budget_exhausted() {
            return Err(NormaSessionError::ToolFailureBudgetExceeded);
        }
        Err(NormaSessionError::MissingFinalResponse)
    };

    tokio::select! {
        _ = cancellation.cancelled() => Err(NormaSessionError::Cancelled),
        timed = tokio::time::timeout(TURN_TIMEOUT, future) => {
            timed.map_err(|_| NormaSessionError::Timeout)?
        }
    }
}
