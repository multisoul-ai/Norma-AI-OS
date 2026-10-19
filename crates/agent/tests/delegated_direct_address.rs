#[path = "support/sse.rs"]
mod sse;
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    AgentRosterEntry, AgentTaskState, AgentTrackingMode, CurrentCanvasContext, NormaDelegation,
    NormaSession, ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Host(Mutex<Vec<(AdapterCommand, ToolContext)>>);
impl AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0
                .lock()
                .expect("实际调用")
                .push((command, context.clone()));
            Ok(AdapterResult::accepted("已接收").with_task_id(71))
        })
    }
}

fn session(host: Arc<dyn AgentCardAdapter>, endpoint: String, active: bool) -> NormaSession {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("isolated-project", &config, host).expect("会话");
    session.update_environment(
        CurrentCanvasContext {
            project_name: "fixture".into(),
            workspace_root: "/isolated".into(),
            total_cards: if active { 2 } else { 0 },
            cards: vec![],
            conversation_tails: vec![],
        },
        ["Codex", "Claude"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| AgentRosterEntry {
                definition_id: index as i64 + 1,
                name: name.into(),
                speech_alias: (index == 0).then(|| "小码".into()),
                profile: if index == 0 {
                    norma::protocol::AgentProfile::Codex
                } else {
                    norma::protocol::AgentProfile::Claude
                },
                enabled: true,
                active_node_id: active.then_some(index as u64 + 11),
                tracking: AgentTrackingMode::Managed,
                busy: false,
                task_state: AgentTaskState::Ready,
            })
            .collect(),
    );
    session
}

fn request(input: &str, cancellation: CancellationToken) -> NormaDelegation {
    NormaDelegation {
        request_id: "live:direct".into(),
        revision: 1,
        user_input: input.into(),
        conversation: String::new(),
        cancellation,
    }
}

/// 场景：GPT的姓名、别名、新任务及续接表达必须交由模型，不能被本地句式路由接管。
/// 构造：真实名册及有卡/无卡状态；HTTP模型只返回讨论文本，不调用工具。
/// 执行：公开委派入口，核对实际HTTP请求与宿主副作用。
/// 预期：每条原话均到达模型；零工具、零宿主调用，不被本地强制派工、回复或拒绝。
#[tokio::test]
async fn delegated_addressed_speech_always_reaches_the_model() {
    for (input, active) in [
        ("Codex，修复登录", true),
        ("小码，修复登录", true),
        ("给 小码 一个新任务：修复登录", true),
        ("Codex，继续刚才那个", false),
    ] {
        let (endpoint, server) = sse::serve_openai(vec![
            json!({"role":"assistant","content":"这一轮先讨论具体需求"}),
        ])
        .await;
        let host = Arc::new(Host::default());
        let result = session(host.clone(), endpoint, active)
            .turn_delegated(request(input, CancellationToken::new()))
            .await
            .expect("应由实际模型处理");
        assert!(
            host.0.lock().expect("实际调用").is_empty(),
            "本地不能自行派发：{input}"
        );
        let requests = server.await.expect("模型请求必须完成");
        assert_eq!(
            requests.len(),
            1,
            "原话必须经模型而非本地姓名规则处理：{input}"
        );
        assert!(
            requests[0]["messages"].to_string().contains(input),
            "模型必须看到完整原话：{input}"
        );
        assert_eq!(
            result.final_text, "这一轮先讨论具体需求",
            "保留模型真实回复"
        );
        assert_eq!(result.tool_calls, 0, "模型没有选择工具");
    }
}

/// 场景：GPT模型可根据姓名或别名语义选择回复或新任务，本地只执行合法参数。
/// 构造：两条自然语音和模型选定的reply_agent/assign_agent_task；真实HTTP往返。
/// 执行：公开委派入口，核对宿主命令、原话、来源和稳定step身份。
/// 预期：只执行模型指定的工具一次，原话不改写，不再生成direct寻址身份。
#[tokio::test]
async fn delegated_model_selects_addressed_tools_and_preserves_original_input() {
    for (input, tool, args, expected) in [
        (
            "小码，之前那个问题呢？",
            "reply_agent",
            json!({"agent":"Codex","message":"之前的问题进展如何？"}),
            AdapterCommand::ReplyAgent {
                agent: "Codex".into(),
                message: "之前的问题进展如何？".into(),
            },
        ),
        (
            "给小码来个新任务，看看如何优化登录",
            "assign_agent_task",
            json!({"agent":"Codex","task":"分析如何优化登录"}),
            AdapterCommand::AssignAgentTask {
                agent: "Codex".into(),
                task: "分析如何优化登录".into(),
            },
        ),
    ] {
        let (endpoint, server)=sse::serve_openai(vec![
            json!({"role":"assistant","tool_calls":[{"index":0,"id":"selected-agent","type":"function","function":{"name":tool,"arguments":args.to_string()}}]}),
            json!({"role":"assistant","content":"已接收"}),
        ]).await;
        let host = Arc::new(Host::default());
        let result = session(host.clone(), endpoint, true)
            .turn_delegated(request(input, CancellationToken::new()))
            .await
            .expect("模型工具可执行");
        assert_eq!(
            server.await.expect("HTTP往返").len(),
            2,
            "工具与最终回复各一轮"
        );
        assert_eq!(result.tool_calls, 1, "工具调用必须来自模型");
        let calls = host.0.lock().expect("宿主命令");
        assert_eq!(calls.len(), 1, "模型指定工具只执行一次");
        assert_eq!(calls[0].0, expected, "目标和正文完全来自typed模型工具");
        assert_eq!(calls[0].1.user_input, input, "保留真实原话");
        assert_eq!(
            calls[0].1.input_source,
            AgentInputSource::GptLiveVoice,
            "不能借用旧来源"
        );
        assert_eq!(
            calls[0].1.operation_id.as_deref(),
            Some("live:direct:gpt-live:r1:step-1"),
            "应使用模型工具step身份"
        );
    }
}

struct UnknownHost(Mutex<Vec<AdapterCommand>>);
impl AgentCardAdapter for UnknownHost {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0.lock().expect("实际工具调用").push(command);
            Ok(AdapterResult::unknown("首个操作结果待核对"))
        })
    }
}

/// 场景：移除确定性寻址后，多Agent模型工具仍必须在首个unknown时停止。
/// 构造：模型同时选择Codex和Claude的reply或assign，首个宿主返回unknown。
/// 执行：真实HTTP委派，读取最终错误与宿主命令列表。
/// 预期：只调用第一名Agent，第二项未执行，返回UnknownSideEffect而非成功。
#[tokio::test]
async fn delegated_model_unknown_stops_other_agents() {
    for (tool, field) in [("reply_agent", "message"), ("assign_agent_task", "task")] {
        let calls:Vec<_>=["Codex","Claude"].into_iter().enumerate().map(|(index,agent)| {
            let args=json!({"agent":agent,field:"修复登录"});
            json!({"index":index,"id":format!("agent-{index}"),"type":"function","function":{"name":tool,"arguments":args.to_string()}})
        }).collect();
        let (endpoint, server) =
            sse::serve_openai(vec![json!({"role":"assistant","tool_calls":calls})]).await;
        let host = Arc::new(UnknownHost(Mutex::new(Vec::new())));
        let result = session(host.clone(), endpoint, true)
            .turn_delegated(request("Codex和Claude，修复登录", CancellationToken::new()))
            .await;
        assert_eq!(
            server.await.expect("模型选择工具").len(),
            1,
            "必须经过实际模型"
        );
        assert!(
            matches!(result, Err(agent::NormaSessionError::UnknownSideEffect)),
            "未知结果必须停止：{result:?}"
        );
        let calls = host.0.lock().expect("实际调用");
        assert_eq!(calls.len(), 1, "第二名Agent不能被继续调用");
        assert_eq!(
            matches!(calls[0], AdapterCommand::AssignAgentTask { .. }),
            tool == "assign_agent_task",
            "覆盖两类工具"
        );
    }
}

struct PendingHost {
    entered: tokio::sync::Notify,
    context: Mutex<Option<ToolContext>>,
}
impl AgentCardAdapter for PendingHost {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            *self.context.lock().expect("正在执行的上下文") = Some(context.clone());
            self.entered.notify_one();
            futures::future::pending().await
        })
    }
}

/// 场景：模型寻址工具等待期间，取消必须及时结束整轮并传递给宿主。
/// 构造：真实HTTP模型调用reply_agent，Adapter以Notify报告进入后永久等待。
/// 执行：观察宿主进入再取消令牌，在1秒测试期限内收取结果。
/// 预期：返回Cancelled，宿主上下文也已取消，没有正常成功结果。
#[tokio::test]
async fn cancelling_pending_model_addressed_tool_ends_the_entire_turn() {
    let (endpoint,server)=sse::serve_openai(vec![json!({"role":"assistant","tool_calls":[{"index":0,"id":"pending-reply","type":"function","function":{"name":"reply_agent","arguments":"{\"agent\":\"Codex\",\"message\":\"修复登录\"}"}}]})]).await;
    let host = Arc::new(PendingHost {
        entered: tokio::sync::Notify::new(),
        context: Mutex::new(None),
    });
    let cancellation = CancellationToken::new();
    let request = request("Codex，修复登录", cancellation.clone());
    let mut session = session(host.clone(), endpoint, true);
    let mut turn = tokio::spawn(async move { session.turn_delegated(request).await });
    tokio::time::timeout(std::time::Duration::from_secs(1), host.entered.notified())
        .await
        .expect("先到达实际宿主");
    cancellation.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), &mut turn).await;
    if result.is_err() {
        turn.abort();
    }
    assert!(
        matches!(result, Ok(Ok(Err(agent::NormaSessionError::Cancelled)))),
        "模型工具等待也必须及时取消：{result:?}"
    );
    assert!(
        host.context
            .lock()
            .expect("上下文")
            .as_ref()
            .expect("已进入")
            .cancellation
            .is_cancelled(),
        "宿主也必须观察到取消"
    );
    assert_eq!(
        server.await.expect("实际模型请求").len(),
        1,
        "本轮经过模型调用"
    );
}
