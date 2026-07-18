use agent::{
    norma_tool_definitions, AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter,
    NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingAdapter {
    commands: Mutex<Vec<AdapterCommand>>,
}

struct UnknownAdapter {
    calls: Mutex<usize>,
}

impl AgentCardAdapter for UnknownAdapter {
    fn execute<'a>(
        &'a self,
        _command: AdapterCommand,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            let mut calls = self
                .calls
                .lock()
                .expect("unknown adapter call lock should not be poisoned");
            *calls += 1;
            Ok(AdapterResult::unknown("无法确认 Terminal 是否已经创建"))
        })
    }
}

impl AgentCardAdapter for RecordingAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.commands
                .lock()
                .expect("recording adapter command lock should not be poisoned")
                .push(command);
            Ok(AdapterResult::accepted("task accepted").with_task_id(41))
        })
    }
}

/// TOOL-1：Norma 只向模型暴露 SPEC 定义的八个窄工具及其精确必填参数
///
/// 数据构造（含关键数值的推导过程）：
///   Expected tools = 8（查询、创建 Agent、派发、取消、建 Terminal、聚焦、输入、删除）
///   Side-effect tools = 7（除 `find_agent_tasks` 外）
///   `assign_agent_task` required fields = 2（agent + task）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从生产工具注册表读取所有模型可见 definition
///   2. 按名称核对完整集合，避免旧的宽泛工具混入
///   3. 检查 `assign_agent_task` 和 `create_agent` 的 JSON Schema
///
/// 预期结果：
///   - 正断言：八个工具全部存在且名称准确
///   - 正断言：派发工具仅要求 agent 与 task，创建工具仅允许 profile
///   - 负断言：不再暴露 `open_agent`，模型也不能提供 name/activate
#[test]
fn norma_exposes_exactly_eight_narrow_tool_schemas() {
    let definitions = norma_tool_definitions();
    let names = definitions
        .iter()
        .map(|definition| definition.name.as_str())
        .collect::<Vec<_>>();
    let expected = vec![
        "find_agent_tasks",
        "create_agent",
        "assign_agent_task",
        "cancel_agent_task",
        "create_terminal",
        "focus_card",
        "send_terminal_input",
        "delete_canvas_card",
    ];

    assert_eq!(
        names, expected,
        "Norma should expose only the eight SPEC tools in deterministic order"
    );
    assert!(
        !names.contains(&"open_agent"),
        "the removed open_agent tool must not be exposed"
    );

    let assign = definitions
        .iter()
        .find(|definition| definition.name == "assign_agent_task")
        .expect("assign_agent_task definition should exist");
    assert_eq!(
        assign.parameters["required"],
        json!(["agent", "task"]),
        "assign_agent_task should require exactly one target and one task"
    );
    assert_eq!(
        assign.parameters["additionalProperties"],
        json!(false),
        "assign_agent_task should reject model-invented fields"
    );

    let create = definitions
        .iter()
        .find(|definition| definition.name == "create_agent")
        .expect("create_agent definition should exist");
    assert_eq!(
        create.parameters["required"],
        json!(["profile"]),
        "create_agent should require only the built-in profile"
    );
    assert!(
        create.parameters["properties"].get("name").is_none(),
        "the model must not choose an Agent name"
    );
    assert!(
        create.parameters["properties"].get("activate").is_none(),
        "created Agents should activate automatically without a model flag"
    );
}

/// TOOL-2：明确指派给 AgentCard 的任务直接进入 Adapter，不经过确认状态
///
/// 数据构造（含关键数值的推导过程）：
///   Tool calls = 1（`assign_agent_task`）
///   Target Agent = 1（Alan）
///   Expected task id = 41（RecordingAdapter 的确定性回执）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以模型产生的 `agent` 与 `task` 参数执行 typed tool
///   2. ToolRuntime 注入 project/turn/cancellation 上下文
///   3. Adapter 立即记录唯一 `AssignAgentTask` 并返回接收回执
///
/// 预期结果：
///   - 正断言：Adapter 恰好收到一次 Alan 的完整任务
///   - 正断言：结果直接是 accepted 且带 task_id=41
///   - 负断言：结果和命令中均不存在确认步骤
#[tokio::test]
async fn assign_agent_task_executes_immediately_without_confirmation() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-7".to_string(),
        user_input: "Alan，请检查当前项目有哪些技术栈。".to_string(),
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-2".to_string(),
        operation_id: Some("turn-7:call-2".to_string()),
        cancellation: CancellationToken::new(),
    };

    let result = runtime
        .execute(
            "assign_agent_task",
            json!({
                "agent": "Alan",
                "task": "检查当前项目有哪些技术栈。"
            }),
            &context,
        )
        .await
        .expect("a valid direct Agent assignment should execute successfully");

    assert_eq!(
        result.status, "accepted",
        "direct assignment should return the actual Adapter acceptance status"
    );
    assert_eq!(
        result.task_id,
        Some(41),
        "direct assignment should return the queued AgentCard task id"
    );
    let commands = adapter
        .commands
        .lock()
        .expect("recording adapter command lock should not be poisoned");
    assert_eq!(
        commands.len(),
        1,
        "direct assignment should invoke the Adapter exactly once"
    );
    assert_eq!(
        commands[0],
        AdapterCommand::AssignAgentTask {
            agent: "Alan".to_string(),
            task: "检查当前项目有哪些技术栈。".to_string(),
        },
        "the Adapter should receive the exact target and task without a confirmation command"
    );
    assert!(
        !result.message.contains("确认"),
        "the direct execution result must not ask the user for confirmation"
    );
}

/// TOOL-3：同一 operation_id 的 unknown 回执会复用，不自动重放副作用
///
/// 数据构造（含关键数值的推导过程）：
///   Runtime execute calls = 2（模拟同一 Rig tool call 被重复提交）
///   Distinct operation ids = 1（`turn-9:call-1`）
///   Expected Adapter side effects = 2 submissions - 1 deduplicated = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一次创建 Terminal 返回 unknown，Runtime 保存原始回执
///   2. 使用相同 operation_id 再次提交相同工具调用
///   3. Runtime 直接返回首次 unknown，不再次调用 Adapter
///
/// 预期结果：
///   - 正断言：两次调用都得到同一个 unknown 事实
///   - 正断言：Adapter 实际只执行一次
///   - 负断言：unknown 不得被提升为 succeeded 或 accepted
#[tokio::test]
async fn unknown_operation_receipt_is_idempotent_and_never_replayed() {
    let adapter = Arc::new(UnknownAdapter {
        calls: Mutex::new(0),
    });
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-9".to_string(),
        user_input: "创建一个 Terminal".to_string(),
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".to_string(),
        operation_id: Some("turn-9:call-1".to_string()),
        cancellation: CancellationToken::new(),
    };

    let first = runtime
        .execute("create_terminal", json!({}), &context)
        .await
        .expect("the first unknown Adapter receipt should be returned as data");
    let second = runtime
        .execute("create_terminal", json!({}), &context)
        .await
        .expect("a duplicate operation should reuse its first receipt");

    assert_eq!(
        first, second,
        "duplicate operation_id calls should return the exact first receipt"
    );
    assert_eq!(
        first.status, "unknown",
        "an uncertain side effect must stay unknown"
    );
    assert!(
        first.status != "succeeded" && first.status != "accepted",
        "unknown must not be upgraded to a success state"
    );
    assert_eq!(
        *adapter
            .calls
            .lock()
            .expect("unknown adapter call lock should not be poisoned"),
        1,
        "the Adapter side effect must execute only once for one operation_id"
    );
}

/// TOOL-4：删除卡片必须由本轮用户原文中的明确删除意图授权
///
/// 数据构造（含关键数值的推导过程）：
///   Delete attempts = 4（模糊“整理” + 两种否定表达 + 明确“删除”）
///   Authorized attempts = 1
///   Expected Adapter side effects = 2 - 1 rejected = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用“整理一下画布”执行 `delete_canvas_card`
///   2. Runtime 在 Adapter 前拒绝模糊意图
///   3. 用“不要删除卡片 #12”验证否定表达仍被拒绝
///   4. 用“删除卡片 #12”再次执行相同工具
///
/// 预期结果：
///   - 正断言：明确删除请求到达 Adapter 且成功
///   - 负断言：模糊整理请求被拒绝，不产生第一次删除副作用
#[tokio::test]
async fn delete_canvas_card_requires_explicit_user_intent() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let ambiguous = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-10".to_string(),
        user_input: "整理一下画布".to_string(),
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".to_string(),
        operation_id: Some("turn-10:call-1".to_string()),
        cancellation: CancellationToken::new(),
    };

    let rejected = runtime
        .execute("delete_canvas_card", json!({"node_id": 12}), &ambiguous)
        .await;

    assert!(
        matches!(rejected, Err(AdapterError::Rejected(_))),
        "an ambiguous canvas cleanup request must be rejected before the Adapter; actual: {rejected:?}"
    );
    assert_eq!(
        adapter
            .commands
            .lock()
            .expect("recording adapter command lock should not be poisoned")
            .len(),
        0,
        "an ambiguous request must not execute any delete side effect"
    );

    let negated = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-11".to_string(),
        user_input: "不要删除卡片 #12，只整理画布".to_string(),
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".to_string(),
        operation_id: Some("turn-11:call-1".to_string()),
        cancellation: CancellationToken::new(),
    };
    let negated_result = runtime
        .execute("delete_canvas_card", json!({"node_id": 12}), &negated)
        .await;

    assert!(
        matches!(negated_result, Err(AdapterError::Rejected(_))),
        "a negated delete request must be rejected before the Adapter; actual: {negated_result:?}"
    );
    assert_eq!(
        adapter
            .commands
            .lock()
            .expect("recording adapter command lock should not be poisoned")
            .len(),
        0,
        "a negated delete request must not execute a delete side effect"
    );

    let negated_close = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-12".to_string(),
        user_input: "千万别关掉卡片 #12".to_string(),
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".to_string(),
        operation_id: Some("turn-12:call-1".to_string()),
        cancellation: CancellationToken::new(),
    };
    let close_result = runtime
        .execute("delete_canvas_card", json!({"node_id": 12}), &negated_close)
        .await;

    assert!(
        matches!(close_result, Err(AdapterError::Rejected(_))),
        "a negated close request must not authorize destructive deletion; actual: {close_result:?}"
    );
    assert_eq!(
        adapter
            .commands
            .lock()
            .expect("recording adapter command lock should not be poisoned")
            .len(),
        0,
        "a negated close request must not execute a delete side effect"
    );

    let explicit = ToolContext {
        project_id: "project-1".to_string(),
        turn_id: "turn-13".to_string(),
        user_input: "删除卡片 #12".to_string(),
        contextual_delete_targets: Vec::new(),
        tool_call_id: "call-1".to_string(),
        operation_id: Some("turn-13:call-1".to_string()),
        cancellation: CancellationToken::new(),
    };
    let accepted = runtime
        .execute("delete_canvas_card", json!({"node_id": 12}), &explicit)
        .await
        .expect("an explicit delete request should reach the Adapter");

    assert_eq!(
        accepted.status, "accepted",
        "the explicit delete should return the Adapter's real success receipt"
    );
    assert_eq!(
        adapter
            .commands
            .lock()
            .expect("recording adapter command lock should not be poisoned")
            .len(),
        1,
        "exactly the explicitly authorized delete should execute"
    );
}

/// TOOL-5：删除目标必须精确绑定节点编号，指代“当前卡片”时还必须匹配真实选择上下文。
///
/// 数据构造（含关键数值的推导过程）：
///   delete attempts = 3（#123→node 12、无上下文“当前卡片”→12、有上下文“当前卡片”→12）
///   contextual targets = 0 + 0 + 1（最后一次仅授权 node 12）
///   expected side effects = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用用户原文“删除 #123”尝试删除 node 12
///   2. 用“删除当前卡片”但不给 selected/focused 上下文再次尝试
///   3. 将 node 12 放入本轮真实上下文目标后重试
///
/// 预期结果：
///   - 正断言：只有第三次精确上下文授权到达 Adapter
///   - 负断言：数字子串 #123 不授权 #12，空上下文指代也不授权任何删除
#[tokio::test]
async fn delete_canvas_card_binds_exact_node_or_actual_context_target() {
    let adapter = Arc::new(RecordingAdapter::default());
    let runtime = NormaToolRuntime::new(adapter.clone());
    let context = |turn: &str, input: &str, targets: Vec<u64>| ToolContext {
        project_id: "project-1".to_string(),
        turn_id: turn.to_string(),
        user_input: input.to_string(),
        contextual_delete_targets: targets,
        tool_call_id: "call-1".to_string(),
        operation_id: Some(format!("{turn}:call-1")),
        cancellation: CancellationToken::new(),
    };

    let substring = runtime
        .execute(
            "delete_canvas_card",
            json!({"node_id": 12}),
            &context("turn-20", "删除卡片 #123", Vec::new()),
        )
        .await;
    let unbound_context = runtime
        .execute(
            "delete_canvas_card",
            json!({"node_id": 12}),
            &context("turn-21", "删除当前卡片", Vec::new()),
        )
        .await;
    let bound_context = runtime
        .execute(
            "delete_canvas_card",
            json!({"node_id": 12}),
            &context("turn-22", "删除当前卡片", vec![12]),
        )
        .await
        .expect("实际 selected/focused 上下文中的 node 12 应获授权");
    let commands = adapter
        .commands
        .lock()
        .expect("recording adapter command lock should not be poisoned");

    assert!(
        matches!(substring, Err(AdapterError::Rejected(_))),
        "用户写 #123 时不得靠数字子串授权删除 #12；实际：{substring:?}"
    );
    assert!(
        matches!(unbound_context, Err(AdapterError::Rejected(_))),
        "没有真实选择上下文时‘当前卡片’不得授权 node 12；实际：{unbound_context:?}"
    );
    assert_eq!(
        bound_context.status, "accepted",
        "真实上下文明确包含 node 12 时应接受删除"
    );
    assert_eq!(commands.len(), 1, "三次尝试中必须只有一次到达 Adapter");
    assert_eq!(
        commands[0],
        AdapterCommand::DeleteCanvasCard { node_id: 12 },
        "唯一删除副作用必须精确命中上下文中的 node 12"
    );
    assert!(
        !commands.contains(&AdapterCommand::DeleteCanvasCard { node_id: 123 }),
        "不得执行用户未通过工具参数指定的 node 123 删除"
    );
}
