use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct CountingAdapter(Arc<AtomicUsize>);
impl AgentCardAdapter for CountingAdapter {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Ok(AdapterResult::succeeded("accepted")) })
    }
}

/// 场景：即便模型在普通群聊中调用派工工具，仍不触发桌面副作用。
/// 数据构造：一个计数 Adapter；一次讨论来源 assign，一次执行来源 assign。
/// 执行过程：讨论请求 → 检查拒绝及计数为零 → 明确执行 → 检查通过及计数为一。
/// 预期结果：讨论不派工，执行可派工，策略由代码强制而非仅依赖提示词。
#[tokio::test]
async fn discussion_cannot_dispatch_but_explicit_execution_can() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = NormaToolRuntime::new(Arc::new(CountingAdapter(calls.clone())));
    let mut context = ToolContext {
        project_id: "".into(),
        turn_id: "turn".into(),
        user_input: "这个方案怎么样".into(),
        input_source: AgentInputSource::MobileDiscussion,
        contextual_delete_targets: vec![],
        tool_call_id: "call".into(),
        operation_id: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    };
    assert!(
        runtime
            .execute(
                "assign_agent_task",
                serde_json::json!({"agent":"Alan","task":"实现"}),
                &context
            )
            .await
            .is_err(),
        "discussion must reject execution tools"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "discussion must not reach desktop adapter"
    );
    context.input_source = AgentInputSource::MobileExecution;
    assert!(
        runtime
            .execute(
                "assign_agent_task",
                serde_json::json!({"agent":"Alan","task":"实现"}),
                &context
            )
            .await
            .is_ok(),
        "explicit execution must permit assignment"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "only explicit execution may reach adapter"
    );
}

/// 场景：普通讨论可以按明确项目区域读取记忆，不能借此写入或指定文件路径。
/// 数据构造：只读工作树 list；同一区域 write；任意 root_path 参数。
/// 执行过程：调用只读工具 → 调用 write → 调用带路径工具。
/// 预期结果：只读请求到达宿主一次；写入和模型路径都被拒绝。
#[tokio::test]
async fn scoped_memory_is_read_only_and_accepts_only_area_identity() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = NormaToolRuntime::new(Arc::new(CountingAdapter(calls.clone())));
    let context = ToolContext {
        project_id: "".into(),
        turn_id: "turn".into(),
        user_input: "查看API的约定".into(),
        input_source: AgentInputSource::MobileDiscussion,
        contextual_delete_targets: vec![],
        tool_call_id: "memory".into(),
        operation_id: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    };
    assert!(
        runtime
            .execute(
                "workspace_memory",
                serde_json::json!({"area_id":"api","action":"list","scope":"worktree"}),
                &context
            )
            .await
            .is_ok(),
        "discussion may request target-scoped memory"
    );
    assert!(runtime.execute("workspace_memory",serde_json::json!({"area_id":"api","action":"write","scope":"worktree","id":"a","markdown":"x"}),&context).await.is_err(), "workspace memory tool must not write");
    assert!(
        runtime
            .execute(
                "workspace_memory",
                serde_json::json!({"area_id":"api","action":"list","root_path":"/tmp"}),
                &context
            )
            .await
            .is_err(),
        "models cannot choose filesystem paths"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "only read-only identity-bound request may reach host"
    );
}
