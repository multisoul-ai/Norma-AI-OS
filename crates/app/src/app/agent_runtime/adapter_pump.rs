fn start_adapter_pump(
    requests: async_channel::Receiver<NormaAdapterRequest>,
    runtime_id: u64,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn_in(window, async move |this, cx| {
        while let Ok(request) = requests.recv().await {
            tracing::debug!(
                runtime_id,
                project_id = %request.context.project_id,
                turn_id = %request.context.turn_id,
                tool_call_id = %request.context.tool_call_id,
                "Norma AgentCard Adapter received tool command"
            );
            let command = request.command.clone();
            let input_source = request.context.input_source;
            let scope = this
                .update_in(cx, |app, _, _| {
                    if !is_current_runtime(app, runtime_id) {
                        return Err(AdapterError::Cancelled);
                    }
                    voice_command::validate(app, &request.context, &command)
                })
                .unwrap_or(Err(AdapterError::Cancelled));
            if let Err(error) = scope {
                let _ = request.result_tx.send(Err(error));
                continue;
            }
            if let Err(error) = context_guard::prepare(&this, cx, &command, &request.context).await
            {
                let _ = request.result_tx.send(Err(error));
                continue;
            }
            let result = this
                .update_in(cx, |app, window, cx| {
                    if request.context.cancellation.is_cancelled() {
                        return Err(AdapterError::Cancelled);
                    }
                    if !is_current_runtime(app, runtime_id) {
                        return Err(AdapterError::Failed(
                            "Norma Adapter 属于已失效项目".to_string(),
                        ));
                    }
                    voice_command::validate(app, &request.context, &command)?;
                    let receipt_command = command.clone();
                    let result = voice_command::with_target(app, request.context.voice_commit.as_deref().and_then(|commit| commit.binding.as_ref()), |app| execute_norma_adapter_command_with_source(
                        app,
                        Some(window),
                        cx,
                        command,
                        input_source,
                    ))?;
                    Ok(voice_command::localize_receipt(
                        input_source,
                        &receipt_command,
                        result,
                    ))
                })
                .unwrap_or_else(|_| {
                    Err(AdapterError::Failed(
                        "Norma Adapter 无法访问 App 上下文".to_string(),
                    ))
                });
            let _ = request.result_tx.send(result);
        }
    })
    .detach();
}

#[cfg(any(test, feature = "e2e-test-support"))]
pub(crate) fn execute_norma_adapter_command(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
    command: AdapterCommand,
) -> Result<AdapterResult, AdapterError> {
    execute_norma_adapter_command_with_source(
        app,
        Some(window),
        cx,
        command,
        agent::AgentInputSource::Other,
    )
}

fn map_card_result(
    result: Result<CardCommandResult, String>,
    message: &str,
) -> Result<AdapterResult, AdapterError> {
    match result.map_err(AdapterError::Rejected)? {
        CardCommandResult::Created { node_id }
        | CardCommandResult::Deleted { node_id }
        | CardCommandResult::Focused { node_id }
        | CardCommandResult::Sent { node_id } => {
            Ok(AdapterResult::succeeded(message).with_node_id(node_id))
        }
        other => Err(AdapterError::Failed(format!(
            "Canvas 操作返回了意外回执：{other:?}"
        ))),
    }
}
