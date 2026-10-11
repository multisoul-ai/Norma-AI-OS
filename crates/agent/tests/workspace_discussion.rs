use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
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

/// 场景：桌面入口中立的工作空间讨论不能获得手机执行身份，也不能调用任务或卡片修改工具。
/// 数据构造：CountingAdapter与非空workspace上下文，来源WorkspaceDiscussion；5种合法修改工具参数及一条已缓存执行回执。
/// 执行过程：逐一拒绝修改；先以Other保存旧执行回执，再用Discussion重复该key；最后读取精确区域记忆。
/// 预期结果：讨论不能返回旧修改回执或触发输入；唯一Other执行及最后只读请求各一次，不伪造手机或路径权限。
#[tokio::test]
async fn entry_neutral_workspace_discussion_has_a_hard_mutation_gate() {
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = NormaToolRuntime::new(Arc::new(CountingAdapter(calls.clone())));
    let context = ToolContext {
        project_id: "workspace-a".into(),
        turn_id: "turn-a".into(),
        user_input: "让 Alan 修复登录".into(),
        input_source: AgentInputSource::WorkspaceDiscussion,
        contextual_delete_targets: vec![],
        tool_call_id: "call-a".into(),
        operation_id: None,
        cancellation: tokio_util::sync::CancellationToken::new(),
    };
    for (name, args) in [
        (
            "assign_agent_task",
            serde_json::json!({"agent":"Alan","task":"修复登录"}),
        ),
        (
            "reply_agent",
            serde_json::json!({"agent":"Alan","message":"继续"}),
        ),
        ("create_terminal", serde_json::json!({})),
        ("focus_card", serde_json::json!({"node_id":1})),
        (
            "memory_write",
            serde_json::json!({"scope":"personal","id":"memo","markdown":"写入记忆"}),
        ),
    ] {
        assert!(
            runtime.execute(name, args, &context).await.is_err(),
            "讨论来源不得修改卡片或任务：{name}"
        );
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "明确原文也不能覆盖代码持有的讨论模式"
    );
    let mut prior = context.clone();
    prior.input_source = AgentInputSource::Other;
    prior.operation_id = Some("prior-execution".into());
    let args = serde_json::json!({"agent":"Alan","task":"修复登录"});
    assert!(
        runtime
            .execute("assign_agent_task", args.clone(), &prior)
            .await
            .is_ok(),
        "已有单项目明确执行回执作为反例数据"
    );
    prior.input_source = AgentInputSource::WorkspaceDiscussion;
    assert!(
        runtime
            .execute("assign_agent_task", args, &prior)
            .await
            .is_err(),
        "缓存的执行回执也不能跳过当前讨论权限门控"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "仅反例构造的Other执行触达Adapter，讨论未重放"
    );
    assert!(
        runtime
            .execute(
                "workspace_memory",
                serde_json::json!({"area_id":"area-a","action":"list","scope":"worktree"}),
                &context
            )
            .await
            .is_ok(),
        "讨论可按授权身份请求只读记忆"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "唯一Other执行和最后只读请求各到达Adapter一次"
    );
}

struct CurrentReadAdapter {
    authorized: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
}

impl AgentCardAdapter for CurrentReadAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let result = if matches!(command, AdapterCommand::Memory(_)) {
            if self.authorized.load(Ordering::SeqCst) {
                Ok(AdapterResult::succeeded("current-read"))
            } else {
                Err(AdapterError::Rejected("read-grant-revoked".into()))
            }
        } else {
            Ok(AdapterResult::accepted("prior-execution"))
        };
        Box::pin(async { result })
    }
}

/// 场景：讨论只读工具不能复用旧修改回执，也不能在授权撤销后复用旧读取结果。
/// 数据构造：宿主可切换只读授权；先缓存Other派工，再以相同operation_id两次讨论读取，第二次撤权。
/// 执行过程：构造旧执行回执 → 允许当前读取 → 撤销授权 → 重复读取。
/// 预期结果：第一次取得当前读取正文而非旧执行状态；第二次返回真实拒绝；三次请求都到宿主复核。
#[tokio::test]
async fn discussion_reads_revalidate_the_host_instead_of_reusing_operation_receipts() {
    let authorized = Arc::new(AtomicBool::new(true));
    let calls = Arc::new(AtomicUsize::new(0));
    let runtime = NormaToolRuntime::new(Arc::new(CurrentReadAdapter {
        authorized: authorized.clone(),
        calls: calls.clone(),
    }));
    let mut context = ToolContext {
        project_id: "workspace-a".into(),
        turn_id: "turn-a".into(),
        user_input: "读取记忆".into(),
        input_source: AgentInputSource::Other,
        contextual_delete_targets: vec![],
        tool_call_id: "call-a".into(),
        operation_id: Some("same-key".into()),
        cancellation: tokio_util::sync::CancellationToken::new(),
    };
    let prior = runtime
        .execute(
            "assign_agent_task",
            serde_json::json!({"agent":"Alan","task":"修复登录"}),
            &context,
        )
        .await
        .expect("构造旧修改回执");
    assert_eq!(prior.message, "prior-execution", "反例必须有旧执行缓存");
    context.input_source = AgentInputSource::WorkspaceDiscussion;
    let args = serde_json::json!({"scope":"personal"});
    let current = runtime
        .execute("memory_list", args.clone(), &context)
        .await
        .expect("当前授权读取");
    assert_eq!(
        current.message, "current-read",
        "只读工具不能带出旧修改回执"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2, "当前读取须经过宿主授权");
    authorized.store(false, Ordering::SeqCst);
    assert_eq!(
        runtime.execute("memory_list", args, &context).await.err(),
        Some(AdapterError::Rejected("read-grant-revoked".into())),
        "撤权后不能复用旧读取成功"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3, "再次读取须复核当前授权");
}
