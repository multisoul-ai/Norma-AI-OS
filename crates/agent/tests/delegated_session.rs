#[path = "support/sse.rs"]
mod sse;
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    NormaDelegation, NormaSession, ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

type Calls = Arc<Mutex<Vec<(AdapterCommand, ToolContext)>>>;
struct Host(Calls);
impl AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0
                .lock()
                .expect("执行记录")
                .push((command, context.clone()));
            Ok(AdapterResult::succeeded("已创建 Terminal #41").with_node_id(41))
        })
    }
}

/// AC05/AC08/AC10：实际模型可见完整对话，但工具授权只采用真实用户原文和Live专属来源。
///
/// 数据构造：用户“创建一个终端”，助手上下文含删除建议；SSE返回create_terminal后给出已核验回复。
/// 执行过程：调用NormaSession委派入口，真实HTTP往返及共享工具运行时到达Adapter。
/// 预期结果：模型收到双向上下文；Adapter只收到用户授权原文、稳定外部请求关联和node回执。
#[tokio::test]
async fn delegation_keeps_model_context_separate_from_tool_authorization() {
    let (endpoint, server) = sse::serve(vec![
        json!({"role":"assistant","tool_calls":[{"index":0,"id":"call-fixture","type":"function","function":{"name":"create_terminal","arguments":"{}"}}]}),
        json!({"role":"assistant","content":"已创建 Terminal #41"}),
    ]).await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "live-backend-fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("isolated-project", &config, Arc::new(Host(calls.clone())))
        .expect("后台会话");
    let result = session
        .turn_delegated(NormaDelegation {
            request_id: "live-session:d1".into(),
            revision: 3,
            user_input: "创建一个终端".into(),
            conversation: "用户：创建一个终端\n助手：还可以删除卡片8".into(),
            cancellation: CancellationToken::new(),
        })
        .await
        .expect("委派回合应成功");
    assert_eq!(result.final_text, "已创建 Terminal #41", "必须保留实际结果");
    assert_eq!(result.tool_calls, 1, "应只有一次模型工具调用");
    let requests = server.await.expect("SSE服务完成");
    assert!(
        requests[0]["messages"]
            .to_string()
            .contains("还可以删除卡片8"),
        "模型必须看见助手发言以理解上下文"
    );
    let calls = calls.lock().expect("读取Adapter记录");
    assert_eq!(calls.len(), 1, "实际Adapter只执行一次");
    assert_eq!(
        calls[0].0,
        AdapterCommand::CreateTerminal,
        "采用共享typed工具路径"
    );
    let context = &calls[0].1;
    assert_eq!(
        context.input_source,
        AgentInputSource::GptLiveVoice,
        "不能伪装豆包来源"
    );
    assert_eq!(
        context.user_input, "创建一个终端",
        "授权依据不得混入助手发言"
    );
    assert_eq!(
        context.operation_id.as_deref(),
        Some("live-session:d1:gpt-live:r3:step-1"),
        "操作身份应依赖稳定请求内序号，不能依赖Rig生成的随机调用编号"
    );
    assert!(
        !context.cancellation.is_cancelled(),
        "已执行的工具上下文此时仍有效"
    );
}

/// AC07：用户明确要求两项相同工具操作时，稳定序号不能将第二项误去重。
///
/// 数据构造：创建两个终端，模型一次返回两个create_terminal调用，原始调用ID不同。
/// 执行过程：真实SSE进入串行工具运行时，记录两项Adapter请求的稳定操作身份。
/// 预期结果：两项均到达Adapter，step-1/step-2不同；不宣称已验证真实桌面卡片。
#[tokio::test]
async fn identical_tools_in_one_delegation_have_distinct_stable_steps() {
    let (endpoint, server) = sse::serve(vec![
        json!({"role":"assistant","tool_calls":[
            {"index":0,"id":"call-first","type":"function","function":{"name":"create_terminal","arguments":"{}"}},
            {"index":1,"id":"call-second","type":"function","function":{"name":"create_terminal","arguments":"{}"}}
        ]}),
        json!({"role":"assistant","content":"已处理两项创建"}),
    ]).await;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("isolated-project", &config, Arc::new(Host(calls.clone())))
        .expect("后台会话");
    let result = session
        .turn_delegated(NormaDelegation {
            request_id: "live-session:d2".into(),
            revision: 1,
            user_input: "创建两个终端".into(),
            conversation: String::new(),
            cancellation: CancellationToken::new(),
        })
        .await
        .expect("两项工具执行");
    server.await.expect("两轮SSE请求结束");
    assert_eq!(result.tool_calls, 2, "明确的两次调用必须都执行");
    let calls = calls.lock().expect("实际Adapter请求");
    assert_eq!(calls.len(), 2, "第二项不能被缓存误吞");
    assert_eq!(
        calls[0].1.operation_id.as_deref(),
        Some("live-session:d2:gpt-live:r1:step-1"),
        "首项稳定ID"
    );
    assert_eq!(
        calls[1].1.operation_id.as_deref(),
        Some("live-session:d2:gpt-live:r1:step-2"),
        "第二项使用独立稳定ID"
    );
}

/// AC06/AC08/AC11：畸形身份、空原文和已取消的委派不能启动模型或项目工具。
///
/// 数据构造：本地未监听端口，仅构造客户端；分别提交路径式ID、空原文和已取消token。
/// 执行过程：调用公开委派入口，检查错误种类与Adapter副作用为空。
/// 预期结果：请求在网络调用之前被拒绝，取消不会变成Connection或其他Provider错误。
#[tokio::test]
async fn invalid_and_cancelled_delegations_do_not_start_a_provider_turn() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:9/v1",
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("isolated-project", &config, Arc::new(Host(calls.clone())))
        .expect("构造本地客户端不应联网");
    let request = |id: &str, input: &str, cancellation| NormaDelegation {
        request_id: id.into(),
        revision: 1,
        user_input: input.into(),
        conversation: String::new(),
        cancellation,
    };
    let malformed = session
        .turn_delegated(request("../invalid", "创建终端", CancellationToken::new()))
        .await;
    assert!(
        matches!(malformed, Err(agent::NormaSessionError::InvalidDelegation)),
        "身份不符合宿主请求格式"
    );
    let empty = session
        .turn_delegated(request("live:empty", "  ", CancellationToken::new()))
        .await;
    assert!(
        matches!(empty, Err(agent::NormaSessionError::EmptyInput)),
        "不得拿空原文启动后台"
    );
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let cancelled = session
        .turn_delegated(request("live:cancelled", "创建终端", cancellation))
        .await;
    assert!(
        matches!(cancelled, Err(agent::NormaSessionError::Cancelled)),
        "已取消请求不得去连接Provider"
    );
    assert!(
        calls.lock().expect("实际副作用").is_empty(),
        "所有无效请求均不得到达Adapter"
    );
}

/// AC07：上游重放同一调用ID或用该ID换命令，均不能再次执行副作用。
/// 数据构造：三轮真实SSE，call-once创建终端后重放；另一组用同ID改为浏览器。
/// 执行过程：每组经公开委派入口、Rig Hook和共享工具缓存，核对最终Adapter命令。
/// 预期结果：只创建一个终端；合法不同ID的相同工具仍由前一测试证明可执行两次。
#[tokio::test]
async fn provider_call_id_replay_and_conflict_do_not_repeat_side_effects() {
    for (tool, args) in [
        ("create_terminal", "{}"),
        ("create_browser", r#"{"url":"https://example.com"}"#),
    ] {
        let call = |tool: &str, args: &str| {
            json!({"role":"assistant","tool_calls":[{
            "index":0,"id":"call-once","type":"function",
            "function":{"name":tool,"arguments":args}}]})
        };
        let (endpoint, server) = sse::serve(vec![
            call("create_terminal", "{}"),
            call(tool, args),
            json!({"role":"assistant","content":"已核对执行回执"}),
        ])
        .await;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let config = ProviderConfig::new(
            ProviderProtocol::OpenAiCompatible,
            endpoint,
            "fixture",
            Some("fixture".into()),
        );
        let mut session =
            NormaSession::new("isolated-project", &config, Arc::new(Host(calls.clone())))
                .expect("会话");
        session
            .turn_delegated(NormaDelegation {
                request_id: "live-session:replay".into(),
                revision: 1,
                user_input: "创建一个终端并打开浏览器 https://example.com".into(),
                conversation: String::new(),
                cancellation: CancellationToken::new(),
            })
            .await
            .expect("重放/冲突可作为工具回执向模型解释");
        let requests = server.await.expect("三轮SSE完成");
        let calls = calls.lock().expect("实际Adapter命令");
        assert_eq!(calls.len(), 1, "同一调用ID重放或改为{tool}都只能执行一次");
        assert_eq!(
            calls[0].0,
            AdapterCommand::CreateTerminal,
            "原操作不能被替换"
        );
        if tool == "create_browser" {
            assert!(
                requests[2]["messages"]
                    .to_string()
                    .contains("操作ID与原始请求不匹配"),
                "调用ID冲突必须回传明确拒绝，不能假报已执行浏览器"
            );
        }
    }
}

/// AC13：原生Ollama不提供调用ID，新增Live身份映射不能阻断既有Legacy工具。
/// 数据构造：原生NDJSON的工具响应和最终回复；分别使用普通turn和Live委派入口。
/// 执行过程：经真实HTTP与Rig Ollama parser执行创建终端，核对Adapter命令与原始来源。
/// 预期结果：两条路径都能执行一次；空provider ID回退到Rig内部身份，不拒绝合法工具。
#[tokio::test]
async fn ollama_without_provider_call_ids_keeps_tools_available() {
    for live in [false, true] {
        let (endpoint, server) = sse::serve_ollama(vec![
            json!({"role":"assistant","content":"","tool_calls":[{
                "function":{"name":"create_terminal","arguments":{}}}]}),
            json!({"role":"assistant","content":"已创建终端"}),
        ])
        .await;
        let calls = Arc::new(Mutex::new(Vec::new()));
        let config = ProviderConfig::new(ProviderProtocol::Ollama, endpoint, "fixture", None);
        let mut session =
            NormaSession::new("isolated-project", &config, Arc::new(Host(calls.clone())))
                .expect("Ollama会话");
        let result = if live {
            session
                .turn_delegated(NormaDelegation {
                    request_id: "live-session:ollama".into(),
                    revision: 1,
                    user_input: "创建一个终端".into(),
                    conversation: String::new(),
                    cancellation: CancellationToken::new(),
                })
                .await
        } else {
            session.turn("创建一个终端").await
        };
        result.expect("Ollama工具不得因缺provider ID被阻断");
        server.await.expect("Ollama请求完成");
        let calls = calls.lock().expect("实际Adapter调用");
        assert_eq!(calls.len(), 1, "live={live}均只执行一次");
        assert_eq!(
            calls[0].0,
            AdapterCommand::CreateTerminal,
            "合法工具必须到达Adapter"
        );
        assert_eq!(
            calls[0].1.input_source,
            if live {
                AgentInputSource::GptLiveVoice
            } else {
                AgentInputSource::Other
            },
            "来源必须保持原路径"
        );
    }
}
