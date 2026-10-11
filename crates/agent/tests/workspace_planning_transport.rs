#[path = "support/workspace_planning_provider.rs"]
mod fixture;
use agent::{generate_workspace_task_plan, WorkspacePlanningError};
use norma::workspace_dispatch::WorkspaceCardRef;
use tokio_util::sync::CancellationToken;

fn targets() -> Vec<WorkspaceCardRef> {
    (1..=2)
        .map(|node_id| WorkspaceCardRef {
            workspace_id: "workspace-a".into(),
            area_id: format!("area-{node_id}"),
            node_id,
            card_instance_id: format!("card-{node_id}"),
        })
        .collect()
}
const COMPLETE: &str = r#"{"assignments":[{"target_index":0,"text":"修复登录"},{"target_index":1,"text":"检查界面"}]}"#;

/// 场景：正式Provider只收到一次无工具请求，完整双目标计划才返回可交付批次。
/// 数据构造：回环OpenAI/Ollama分别返回完整2项JSON；代码持有两张卡片和原文，context仅提供事实。
/// 执行过程：调用正式ModelFactory/单次completion，读取实际HTTP请求，核对schema/targets/原文。
/// 预期结果：两种协议均返回完整原卡片集合；tools为空、请求数=1，无source/执行fingerprint可由模型填写。
#[tokio::test]
async fn whole_planner_requests_once_without_execution_tools() {
    for native in [false, true] {
        let provider = fixture::provider(native, COMPLETE, "stop", 200);
        let targets = targets();
        let plan = generate_workspace_task_plan(
            &provider.config,
            "让 Alan 修复登录，并让 Simon 检查界面",
            "当前项目展开仅提供上下文",
            &targets,
            &CancellationToken::new(),
        )
        .await
        .expect("完整规划");
        provider.server.join().expect("Provider结束");
        let requests = provider.requests.lock().expect("实际请求");
        assert_eq!(requests.len(), 1, "一条用户操作恰好一次完整模型请求");
        let request = &requests[0];
        assert!(
            request["tools"].is_null() || request["tools"].as_array().is_some_and(Vec::is_empty),
            "whole planner不得注册任何执行工具"
        );
        let user = request["messages"]
            .as_array()
            .expect("消息")
            .iter()
            .find(|message| message["role"] == "user")
            .expect("原文输入");
        let input: serde_json::Value =
            serde_json::from_str(user["content"].as_str().expect("JSON输入")).expect("结构化输入");
        assert_eq!(
            input["original_text"], "让 Alan 修复登录，并让 Simon 检查界面",
            "原文必须完整进入一次planning"
        );
        assert_eq!(
            input["targets"].as_array().expect("目标").len(),
            2,
            "两目标全集一起规划"
        );
        assert!(
            input.get("operation_id").is_none(),
            "来源操作不进入模型可伪造schema"
        );
        if native {
            assert_eq!(
                request["format"]["additionalProperties"], false,
                "原生schema拒绝额外可信字段"
            );
            assert_eq!(
                request["format"]["properties"]["assignments"]["minItems"], 2,
                "schema要求覆盖2目标"
            );
            assert_eq!(
                request["format"]["properties"]["assignments"]["maxItems"], 2,
                "schema禁止追加第3目标"
            );
        }
        assert_eq!(plan.tasks().len(), 2, "完整模型成功后返回一个整组");
        assert_eq!(plan.tasks()[0].card(), &targets[0], "目标1仍是原持久卡片");
        assert_eq!(plan.tasks()[1].card(), &targets[1], "目标2仍是原持久卡片");
    }
}

/// 场景：即使JSON看似完整，Provider截断终止也不能返回可执行计划。
/// 数据构造：OpenAI finish_reason=length、Ollama done_reason=length都带完整2项JSON；各一次请求。
/// 执行过程：正式completion检查协议终止证据，再尝试完整响应解析。
/// 预期结果：两种协议均报IncompleteResponse，不能因为JSON闭合就把截断生成当成功。
#[tokio::test]
async fn provider_length_finish_rejects_even_closed_json() {
    for native in [false, true] {
        let provider = fixture::provider(native, COMPLETE, "length", 200);
        let result = generate_workspace_task_plan(
            &provider.config,
            "让 Alan 和 Simon 检查",
            "",
            &targets(),
            &CancellationToken::new(),
        )
        .await;
        provider.server.join().expect("Provider结束");
        assert_eq!(
            result.err(),
            Some(WorkspacePlanningError::IncompleteResponse),
            "Provider没有完整成功，整组不可提交"
        );
        assert_eq!(
            provider.requests.lock().expect("请求").len(),
            1,
            "失败不自动再规划或拆成单项请求"
        );
    }
}

/// 场景：模型遗漏第二目标或Provider错误，均不返回可提交的第一项计划。
/// 数据构造：一次合法HTTP但只含索引0；另一次HTTP400。原始目标全集仍为2。
/// 执行过程：分别走正式Provider生成，检查错误类型及单次请求数量。
/// 预期结果：遗漏返回TargetSetMismatch，Provider错误返回ProviderFailed，没有自动重试或部分计划。
#[tokio::test]
async fn missing_second_target_and_provider_error_do_not_return_a_partial_plan() {
    for (content, status, expected) in [
        (
            r#"{"assignments":[{"target_index":0,"text":"修复登录"}]}"#,
            200,
            WorkspacePlanningError::TargetSetMismatch,
        ),
        ("", 400, WorkspacePlanningError::ProviderFailed),
    ] {
        let provider = fixture::provider(false, content, "stop", status);
        let result = generate_workspace_task_plan(
            &provider.config,
            "让 Alan 和 Simon 检查",
            "",
            &targets(),
            &CancellationToken::new(),
        )
        .await;
        provider.server.join().expect("Provider结束");
        assert_eq!(result.err(), Some(expected), "失败必须阻止整组提交");
        assert_eq!(
            provider.requests.lock().expect("请求").len(),
            1,
            "失败只保留原操作，不拆分或重试"
        );
    }
}

/// 场景：已取消的planning在访问Provider前结束，取消不能变成一次后台派工。
/// 数据构造：指向不会监听的回环端口，合法两目标；token调用前已cancel。
/// 执行过程：正式生成器验证取消状态，不访问Provider。
/// 预期结果：立即返回Cancelled，不返回ProviderFailed或半个计划。
#[tokio::test]
async fn cancellation_before_planning_does_not_contact_provider() {
    let config = agent::ProviderConfig::new(
        agent::ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:1/v1",
        "workspace-fixture",
        None,
    );
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let result = generate_workspace_task_plan(
        &config,
        "让 Alan 和 Simon 检查",
        "",
        &targets(),
        &cancellation,
    )
    .await;
    assert_eq!(
        result.err(),
        Some(WorkspacePlanningError::Cancelled),
        "已取消操作不得访问Provider或返回计划"
    );
}

/// 场景：请求已发送后取消，后台迟到响应不能产出可提交计划。
/// 数据构造：回环Provider已捕获请求但等待测试放行，原文与两目标完整；捕获后cancel。
/// 执行过程：正式planner与取消任务并行，确认实际请求到达再取消，之后放行迟到响应并收口线程。
/// 预期结果：返回Cancelled，恰好1次请求；取消优先且没有返回部分计划。
#[tokio::test]
async fn cancellation_during_provider_wait_discards_the_whole_plan() {
    let provider = fixture::pending_provider();
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    let captured = provider.requests.clone();
    let cancel_when_received = async move {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if !captured.lock().expect("已捕获请求").is_empty() {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("回环请求须先发送");
        cancel.cancel();
    };
    let targets = targets();
    let (result, ()) = tokio::join!(
        generate_workspace_task_plan(
            &provider.config,
            "让 Alan 和 Simon 检查",
            "",
            &targets,
            &cancellation
        ),
        cancel_when_received
    );
    assert_eq!(
        result.err(),
        Some(WorkspacePlanningError::Cancelled),
        "进行中取消不能返回部分计划"
    );
    provider
        .release
        .expect("放行迟到响应")
        .send(())
        .expect("服务器仍在等待");
    provider.server.join().expect("Provider收口");
    assert_eq!(
        provider.requests.lock().expect("请求").len(),
        1,
        "取消不触发Provider重试"
    );
}

/// 场景：模型迟迟不返回时正式20秒预算截止，不能把已接到的部分处理当派工成功。
/// 数据构造：回环Provider捕获1个完整请求后等待测试放行；无任何JSON响应。
/// 执行过程：调用正式生成器直至20秒超时，随后放行连接以收口fixture。
/// 预期结果：返回Timeout而非计划；原请求只发送1次，不自动重试。
#[tokio::test]
async fn timeout_before_complete_response_returns_no_plan() {
    let provider = fixture::pending_provider();
    let result = generate_workspace_task_plan(
        &provider.config,
        "让 Alan 和 Simon 检查",
        "",
        &targets(),
        &CancellationToken::new(),
    )
    .await;
    assert_eq!(
        result.err(),
        Some(WorkspacePlanningError::Timeout),
        "20秒预算截止必须整组失败"
    );
    provider
        .release
        .expect("放行超时响应")
        .send(())
        .expect("服务器等待25秒，20秒预算应先截止");
    provider.server.join().expect("Provider收口");
    assert_eq!(
        provider.requests.lock().expect("请求").len(),
        1,
        "超时不拆分或自动再请求"
    );
}

struct NoMutationAdapter(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl agent::AgentCardAdapter for NoMutationAdapter {
    fn execute<'a>(
        &'a self,
        _: agent::AdapterCommand,
        _: &'a agent::ToolContext,
    ) -> futures::future::BoxFuture<'a, Result<agent::AdapterResult, agent::AdapterError>> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async {
            Err(agent::AdapterError::Rejected(
                "fixture does not mutate".into(),
            ))
        })
    }
}

/// 场景：工作空间讨论的正式模型请求只暴露记忆工具，不能复用单项目direct-address执行旁路。
/// 数据构造：非空workspace身份，已知Alan卡片，显式命令正文仍由代码标记Discussion；回环SSE回复讨论文本。
/// 执行过程：调用正式NormaSession的中立Discussion回合，捕获真实HTTP工具目录与Adapter调用次数。
/// 预期结果：模型收到原文但未获派工/Reply/Cancel/创建/输入工具；直接点名helper不执行，Adapter副作用0。
#[tokio::test]
async fn workspace_discussion_provider_cannot_receive_execution_tools() {
    let provider = fixture::provider(false, "我们继续讨论登录方案。", "stop", 200);
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut session = agent::NormaSession::new(
        "workspace-a",
        &provider.config,
        std::sync::Arc::new(NoMutationAdapter(calls.clone())),
    )
    .expect("工作空间讨论会话");
    session.update_environment(
        agent::CurrentCanvasContext {
            project_name: "工作空间A".into(),
            workspace_root: String::new(),
            total_cards: 1,
            cards: vec![],
            conversation_tails: vec![],
        },
        vec![agent::AgentRosterEntry {
            definition_id: 1,
            name: "Alan".into(),
            speech_alias: None,
            profile: norma::protocol::AgentProfile::Codex,
            enabled: true,
            active_node_id: Some(1),
            tracking: agent::AgentTrackingMode::Managed,
            busy: false,
            task_state: agent::AgentTaskState::Ready,
        }],
    );
    let (deltas, _receiver) = tokio::sync::mpsc::channel(16);
    let result = session
        .turn_streaming(
            "让 Alan 修复登录",
            deltas,
            agent::AgentInputSource::WorkspaceDiscussion,
        )
        .await;
    provider.server.join().expect("Provider结束");
    assert!(
        result.is_ok(),
        "讨论应走模型回复，不能被旧direct-address尝试执行后报错"
    );
    assert_eq!(
        calls.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "讨论从已知点名到模型完整回复仍保持零Adapter副作用"
    );
    let requests = provider.requests.lock().expect("实际HTTP");
    assert_eq!(requests.len(), 1, "讨论只进入一个模型回合");
    let tools = requests[0]["tools"].as_array().expect("只读工具目录");
    assert!(
        tools
            .iter()
            .any(|tool| tool["function"]["name"] == "workspace_memory"),
        "允许按授权区域读取项目记忆"
    );
    for name in [
        "assign_agent_task",
        "reply_agent",
        "cancel_agent_task",
        "create_agent",
        "create_terminal",
        "send_terminal_input",
        "focus_card",
        "memory_write",
    ] {
        assert!(
            !tools.iter().any(|tool| tool["function"]["name"] == name),
            "讨论模型不得获得修改工具：{name}"
        );
    }
}
