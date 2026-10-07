use super::*;

pub(crate) fn start_doubao_adapter(
    project_id: String,
    generation: u64,
    paths: storage::DatabasePaths,
    root: Option<std::path::PathBuf>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Arc<dyn agent::AgentCardAdapter> {
    let (sender, requests) = async_channel::bounded(16);
    start_doubao_adapter_pump(project_id, generation, requests, window, cx);
    super::super::memory::adapter(Arc::new(ChannelAgentCardAdapter { sender }), paths, root)
}

fn start_doubao_adapter_pump(
    project_id: String,
    generation: u64,
    requests: async_channel::Receiver<NormaAdapterRequest>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) {
    cx.spawn_in(window, async move |this, cx| {
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
                    execute_norma_adapter_command_with_source(
                        app,
                        Some(window),
                        cx,
                        request.command.clone(),
                        request.context.input_source,
                    )
                })
                .unwrap_or_else(|_| {
                    Err(AdapterError::Failed("豆包工具无法访问 App 上下文".into()))
                });
            let _ = request.result_tx.send(result);
        }
    })
    .detach();
}
