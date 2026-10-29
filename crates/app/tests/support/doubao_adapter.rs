use crate::app::MultisoulApp;
use agent::{AdapterCommand, AdapterError, AdapterResult, ToolContext};
use gpui::{Entity, VisualTestContext};

/// Exercise the production async pump, including source/target preparation.
pub(crate) fn execute(
    app: &Entity<MultisoulApp>,
    cx: &mut VisualTestContext,
    command: AdapterCommand,
) -> Result<AdapterResult, AdapterError> {
    let (adapter, project) = app.update_in(cx, |app, window, cx| {
        let root = app.runtime.workspace_root().unwrap().to_path_buf();
        let project = root.display().to_string();
        let service = app.shared_context_service().unwrap();
        let adapter = crate::app::agent_runtime::start_doubao_adapter(
            project.clone(),
            app.doubao.generation,
            app.database_paths.clone(),
            Some(root),
            service,
            window,
            cx,
        );
        (adapter, project)
    });
    let runtime = crate::app::shared_tokio_runtime().unwrap();
    let (sent, received) = std::sync::mpsc::channel();
    runtime.spawn(async move {
        let result = adapter
            .execute(
                command,
                &ToolContext {
                    project_id: project,
                    turn_id: "cold-target:turn-1".into(),
                    user_input: "检查登录方案".into(),
                    input_source: agent::AgentInputSource::DoubaoVoice,
                    contextual_delete_targets: vec![],
                    tool_call_id: "dispatch-1".into(),
                    operation_id: None,
                    cancellation: Default::default(),
                },
            )
            .await;
        sent.send(result).unwrap();
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        if let Ok(result) = received.try_recv() {
            return result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "后台状态准备必须在测试期限内完成"
        );
        // Only this test harness waits; GPUI production code remains asynchronous.
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

pub(crate) fn is_cold(app: &Entity<MultisoulApp>, cx: &VisualTestContext) -> bool {
    app.read_with(cx, |app, _| app.shared_context.is_none())
}
