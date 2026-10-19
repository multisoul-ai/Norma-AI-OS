#[cfg(test)]
#[path = "../../../tests/unit/gpt_live_adapter_tests.rs"]
mod tests;
use super::*;

#[derive(Clone)]
pub(crate) struct LiveAdapterScope {
    pub project: String,
    pub generation: u64,
    pub timeline: Arc<Mutex<live_voice::LiveContext>>,
    pub snapshot: live_voice::DelegationSnapshot,
}

pub(crate) fn start_gpt_live_adapter(
    scope: LiveAdapterScope,
    paths: storage::DatabasePaths,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Arc<dyn agent::AgentCardAdapter> {
    let root = (!scope.project.is_empty()).then(|| std::path::PathBuf::from(&scope.project));
    let (sender, requests) = async_channel::bounded::<NormaAdapterRequest>(16);
    cx.spawn_in(window, async move |this, cx| {
        while let Ok(request) = requests.recv().await {
            let result = this
                .update_in(cx, |app, window, cx| {
                    execute_live_command(
                        app,
                        window,
                        cx,
                        &scope,
                        &request.context,
                        request.command.clone(),
                    )
                })
                .unwrap_or_else(|_| {
                    Err(AdapterError::Unknown(
                        "GPT-Live无法访问项目窗口，结果待核对".into(),
                    ))
                });
            let _ = request.result_tx.send(result);
        }
    })
    .detach();
    super::super::memory::adapter(Arc::new(ChannelAgentCardAdapter { sender }), paths, root)
}

fn execute_live_command(
    app: &mut MultisoulApp,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
    scope: &LiveAdapterScope,
    context: &agent::ToolContext,
    command: AdapterCommand,
) -> Result<AdapterResult, AdapterError> {
    if context.cancellation.is_cancelled() {
        return Err(AdapterError::Cancelled);
    }
    let project = app
        .runtime
        .workspace_root()
        .map(|root| root.display().to_string());
    if scope.project.is_empty()
        || project.as_deref() != Some(scope.project.as_str())
        || context.project_id != scope.project
        || !app.gpt_live.matches_generation(scope.generation)
        || !app.gpt_live.input_complete()
        || context.input_source != AgentInputSource::GptLiveVoice
        || context.user_input != scope.snapshot.user_input
        || !scope
            .timeline
            .lock()
            .expect("Live authorization lock")
            .is_current(&scope.snapshot)
    {
        return Err(AdapterError::Rejected(
            "GPT-Live项目、通话或用户请求已变化，未执行旧操作".into(),
        ));
    }
    let mut result = execute_norma_adapter_command_with_source(
        app,
        Some(window),
        cx,
        command,
        AgentInputSource::GptLiveVoice,
    )?;
    if result.status == "accepted" {
        if let Some(task) = app.agent_task_coordinator.as_ref().and_then(|coordinator| {
            coordinator.tasks().into_iter().find(|task| {
                Some(task.task_id) == result.task_id && Some(task.target_node_id) == result.node_id
            })
        }) {
            result.tasks.push(serde_json::json!({"task_id":task.task_id,"task_ref":task.task_ref,"node_id":task.target_node_id,"state":task.state,"agent":task.agent}));
        }
    }
    Ok(result)
}
