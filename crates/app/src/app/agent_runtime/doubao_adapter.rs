use super::*;

pub(crate) fn start_doubao_adapter(
    project_id: String,
    generation: u64,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Arc<dyn agent::AgentCardAdapter> {
    let (sender, requests) = async_channel::bounded(16);
    start_doubao_adapter_pump(project_id, generation, requests, window, cx);
    Arc::new(ChannelAgentCardAdapter { sender })
}

fn start_doubao_adapter_pump(
    project_id: String,
    generation: u64,
    requests: async_channel::Receiver<NormaAdapterRequest>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn_in(window, async move |this, cx| {
        let mut confirmation = voice_confirmation::PendingVoiceConfirmation::default();
        while let Ok(request) = requests.recv().await {
            let result = this
                .update_in(cx, |app, window, cx| {
                    let current_project = app
                        .runtime
                        .workspace_root()
                        .map(|root| root.display().to_string());
                    if current_project.as_deref() != Some(project_id.as_str()) {
                        return Err(AdapterError::Rejected(
                            "当前项目已变更，豆包工具调用已拒绝".into(),
                        ));
                    }
                    if app.doubao.generation != generation {
                        return Err(AdapterError::Rejected(
                            "豆包通话已结束，工具调用已拒绝".into(),
                        ));
                    }
                    if !confirmation.authorize(
                        &request.command,
                        &request.context.user_input,
                        &request.context.turn_id,
                    ) {
                        return Err(AdapterError::Rejected(
                            "用户转写没有明确授权执行该操作".into(),
                        ));
                    }
                    let result = execute_norma_adapter_command_with_source(
                        app,
                        Some(window),
                        cx,
                        request.command.clone(),
                        request.context.input_source,
                    );
                    if let Ok(receipt) = &result {
                        if receipt.status == "accepted" {
                            if let Some(task_id) = receipt.task_id {
                                app.doubao.tracked_tasks.insert(task_id);
                            }
                        }
                    }
                    result
                })
                .unwrap_or_else(|_| {
                    Err(AdapterError::Failed("豆包工具无法访问 App 上下文".into()))
                });
            let _ = request.result_tx.send(result);
        }
    })
    .detach();
}
