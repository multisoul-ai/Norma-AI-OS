use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    NormaToolRuntime, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Host(Mutex<Vec<AdapterCommand>>);

impl AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0.lock().expect("宿主实际命令").push(command);
            Ok(AdapterResult::succeeded("实际工具回执"))
        })
    }
}

fn context(input: &str) -> ToolContext {
    ToolContext {
        project_id: "isolated-project".into(),
        turn_id: "live:model:r1".into(),
        user_input: input.into(),
        input_source: AgentInputSource::GptLiveVoice,
        contextual_delete_targets: Vec::new(),
        tool_call_id: "fixture-call".into(),
        operation_id: None,
        cancellation: Default::default(),
    }
}

/// 场景：GPT已选择合法工具，本地不能用原话的关键词、标点或句式再次推断意图。
/// 构造：创建、派工、回复、取消、记忆和删除等工具，配含问号/否定/如何的自然表达。
/// 执行：逐项经过公开NormaToolRuntime真实参数解析，核对宿主实际执行次数。
/// 预期：全部合法调用到达宿主；删除仍只执行模型参数中选定的唯一目标。
#[tokio::test]
async fn gpt_live_model_selected_tools_are_not_filtered_by_user_wording() {
    let cases = [
        (
            "嗯，你能建个Codex分析如何优化登录吗？",
            "create_agent",
            json!({"profile":"codex","task":"分析如何优化登录"}),
        ),
        (
            "Codex能不能先看看这个问题？",
            "assign_agent_task",
            json!({"agent":"Codex","task":"分析登录问题"}),
        ),
        (
            "Simon，之前那个问题呢？",
            "reply_agent",
            json!({"agent":"Simon","message":"之前的问题进展如何？"}),
        ),
        ("那个任务先缓缓", "cancel_agent_task", json!({"task_id":7})),
        (
            "给Simon的消息先撤回来吧",
            "cancel_agent_reply",
            json!({"agent":"Simon"}),
        ),
        ("给我来个终端吧", "create_terminal", json!({})),
        (
            "能开个网页吗？",
            "create_browser",
            json!({"url":"https://example.com"}),
        ),
        (
            "这个文件怎么预览？",
            "create_preview",
            json!({"path":"/isolated/readme.md"}),
        ),
        ("让我看看第九张", "focus_card", json!({"node_id":9})),
        (
            "检查一下是否有改动",
            "send_terminal_input",
            json!({"node_id":9,"text":"git status","submit":true}),
        ),
        (
            "以后就中文聊，不用自动播报",
            "memory_write",
            json!({"scope":"personal","id":"preferences","markdown":"中文交流，不自动播报"}),
        ),
        (
            "这张已经没用了，收起来吧",
            "delete_canvas_card",
            json!({"node_id":9}),
        ),
        (
            "第二张可以撤掉了吗？",
            "delete_canvas_card",
            json!({"position":2}),
        ),
        (
            "Simon那张可以收起来吗？",
            "delete_canvas_card",
            json!({"agent":"Simon"}),
        ),
    ];
    let host = Arc::new(Host::default());
    let runtime = NormaToolRuntime::new(host.clone());
    for (input, tool, arguments) in &cases {
        let result = runtime
            .execute(tool, arguments.clone(), &context(input))
            .await;
        assert_eq!(
            result
                .unwrap_or_else(|error| panic!(
                    "模型选定的{tool}不能因原话被拦截：{input}，{error}"
                ))
                .status,
            "succeeded",
            "必须采用实际宿主回执：{tool}"
        );
    }
    let calls = host.0.lock().expect("实际调用");
    assert_eq!(
        calls.len(),
        cases.len(),
        "每项合法调用都必须到达宿主且不重复"
    );
    assert_eq!(
        &calls[calls.len() - 3..],
        &[
            AdapterCommand::DeleteCanvasCard { node_id: 9 },
            AdapterCommand::DeleteCanvasCardAtPosition { position: 2 },
            AdapterCommand::DeleteAgentCard {
                agent: "Simon".into()
            },
        ],
        "删除目标必须完全来自已解析的唯一工具参数，不能通过原话另选目标"
    );
}

/// 场景：移除语言门禁后，结构错误及取消仍须阻止工具执行。
/// 构造：未知profile、缺目标、双目标、负数目标，以及合法但已取消的创建。
/// 执行：公开运行时依次提交，检查具体错误类别及宿主记录。
/// 预期：全部非法参数返回InvalidArguments，取消返回Cancelled，副作用为零。
#[tokio::test]
async fn gpt_live_keeps_typed_argument_and_cancellation_checks() {
    let host = Arc::new(Host::default());
    let runtime = NormaToolRuntime::new(host.clone());
    for (tool, arguments) in [
        ("create_agent", json!({"profile":"unknown"})),
        ("delete_canvas_card", json!({})),
        ("delete_canvas_card", json!({"node_id":9,"position":2})),
        ("delete_canvas_card", json!({"node_id":-1})),
    ] {
        assert!(
            matches!(
                runtime.execute(tool, arguments, &context("可以吗？")).await,
                Err(AdapterError::InvalidArguments(_))
            ),
            "移除词表不能跳过{tool}的真实参数校验"
        );
    }
    let cancelled = context("来个终端吧");
    cancelled.cancellation.cancel();
    assert_eq!(
        runtime
            .execute("create_terminal", json!({}), &cancelled)
            .await,
        Err(AdapterError::Cancelled),
        "取消令牌必须在宿主副作用前生效"
    );
    assert!(
        host.0.lock().expect("实际命令").is_empty(),
        "无效或已取消请求不得产生副作用"
    );
}

/// 场景：GPT移除删除原话判断，不得放宽豆包之外其他来源的既有删除规则。
/// 构造：同一合法node_id=9，原话没有删除动词或编号；分别使用各原有来源。
/// 执行：公开运行时逐项执行，观察具体拒绝类别和实际到达宿主的命令。
/// 预期：Voice/Other/MobileExecution继续拒绝；Doubao原有豁免与GPT独立放行均保留。
#[tokio::test]
async fn gpt_live_removal_preserves_legacy_delete_policies() {
    let host = Arc::new(Host::default());
    let runtime = NormaToolRuntime::new(host.clone());
    for source in [
        AgentInputSource::Voice,
        AgentInputSource::Other,
        AgentInputSource::MobileExecution,
    ] {
        let mut input = context("这张没用了，收起来吧");
        input.input_source = source;
        assert!(
            matches!(
                runtime
                    .execute("delete_canvas_card", json!({"node_id":9}), &input)
                    .await,
                Err(AdapterError::Rejected(_))
            ),
            "原有{source:?}删除意图校验必须保持"
        );
    }
    assert!(
        host.0.lock().expect("旧来源副作用").is_empty(),
        "拒绝不能产生副作用"
    );
    for source in [
        AgentInputSource::DoubaoVoice,
        AgentInputSource::GptLiveVoice,
    ] {
        let mut input = context("这张没用了，收起来吧");
        input.input_source = source;
        runtime
            .execute("delete_canvas_card", json!({"node_id":9}), &input)
            .await
            .expect("采用模型选定的目标");
    }
    assert_eq!(
        *host.0.lock().expect("模型工具调用"),
        vec![AdapterCommand::DeleteCanvasCard { node_id: 9 }; 2],
        "两个独立来源各执行一次相同目标"
    );
}
