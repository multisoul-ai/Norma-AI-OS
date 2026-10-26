#[path = "support/sse.rs"]
mod sse;
use agent::{
    context::*, AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, NormaSession,
    ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use std::sync::Arc;

struct Host;
impl AgentCardAdapter for Host {
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Box::pin(async move {
            let scope = ContextScope {
                workspace_id: "w".into(),
                area_id: "a".into(),
                project: ContextProject::Personal,
                task_id: None,
                conversation_id: None,
            };
            let input = ContextRef {
                scope: scope.clone(),
                event_id: "input".into(),
                revision: 1,
            };
            let reference = ContextRef {
                scope: scope.clone(),
                event_id: "report".into(),
                revision: 2,
            };
            let record = RecordedContextEvent {
                reference: reference.clone(),
                event: ContextEvent {
                    event_id: "report".into(),
                    scope: scope.clone(),
                    source: ContextSource {
                        origin: ContextOrigin::Brain,
                        stream_id: "s".into(),
                        sequence: 1,
                    },
                    created_at_ms: 1,
                    related: vec![],
                    payload: ContextPayload::Report {
                        text: format!("{}：NOT-AN-INSTRUCTION", context.user_input),
                    },
                },
            };
            let mut state = WorkingState::default();
            state.reports.insert(
                "report".into(),
                ContextReport {
                    reference,
                    text: "报告正文".into(),
                    delivery: ReportDelivery::Skipped,
                },
            );
            Ok(Some(ContextFrame {
                input,
                snapshot: ContextSnapshot {
                    scope,
                    revision: 2,
                    state,
                    events: vec![record],
                },
            }))
        })
    }
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("fixture has no side effects".into())) })
    }
}

/// 场景：默认 16k 模型接入共享资料后仍能请求，资料不能混入系统指令或授权。
/// 构造：真实回环 HTTP、带来源的相关报告和一条当前用户输入。
/// 执行：调用公开 NormaSession::turn，检查实际发送的 messages 和工具。
/// 预期：请求成功，报告明确作为资料；当前输入保留，工具包含补读和状态更新。
#[tokio::test]
async fn default_window_sends_attributed_shared_data() {
    let (endpoint, server) = sse::serve(vec![
        serde_json::json!({"role":"assistant","content":"尚未播放。"}),
    ])
    .await;
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, Arc::new(Host)).unwrap();
    let result = session.turn("报告详情").await;
    assert!(result.is_ok(), "默认窗口应容纳必要资料：{result:?}");
    let requests = server.await.unwrap();
    let body = &requests[0];
    let messages = body["messages"].as_array().unwrap();
    assert!(
        messages
            .iter()
            .filter(|m| m["role"] == "system")
            .all(|m| !m["content"].to_string().contains("NOT-AN-INSTRUCTION")),
        "资料正文不能升级为系统指令"
    );
    assert!(
        messages
            .iter()
            .any(|m| m["content"].to_string().contains("shared_context_data")),
        "实际请求应包含明确资料标签"
    );
    assert!(
        messages
            .iter()
            .any(|m| m["role"] == "user" && m["content"].to_string().contains("报告详情")),
        "当前用户消息必须保留"
    );
    for name in ["context_read", "context_update"] {
        assert!(
            body["tools"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t["function"]["name"] == name),
            "实际请求需要工具 {name}"
        );
    }
    assert!(
        body.to_string().len() + agent::MEMORY_TOOL_BYTES < (16_384 - 2048) * 2,
        "实际输入与回执预留应满足默认保守预算：request={} tools={} messages={}",
        body.to_string().len(),
        body["tools"].to_string().len(),
        body["messages"].to_string().len()
    );
}

struct LargeReceiptHost;
impl AgentCardAdapter for LargeReceiptHost {
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Host.shared_context(context)
    }
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async {
            Ok(AdapterResult::succeeded("只读资料")
                .with_snapshot(serde_json::json!({"text":"x".repeat(32 * 1024)})))
        })
    }
}

/// 场景：工具回执超过初始预留，续轮不能发送超窗口请求。
/// 构造：默认 16k 窗口、一次 context_read 调用、32 KiB 来源资料。
/// 执行：从公开 turn 入口运行工具链；服务器只接受首个请求。
/// 预期：在第二个网络请求前明确返回 ContextBudgetExceeded，不能报连接失败或裁掉工具结果。
#[tokio::test]
async fn tool_continuation_rechecks_complete_request_budget() {
    let frame = Host
        .shared_context(&ToolContext {
            project_id: "project".into(),
            turn_id: "t".into(),
            user_input: "报告详情".into(),
            input_source: agent::AgentInputSource::Other,
            contextual_delete_targets: vec![],
            tool_call_id: String::new(),
            operation_id: None,
            cancellation: Default::default(),
        })
        .await
        .unwrap()
        .unwrap();
    let (endpoint, server) = sse::serve(vec![serde_json::json!({
        "role":"assistant", "tool_calls":[{"index":0,"id":"read-1","type":"function",
        "function":{"name":"context_read","arguments":serde_json::json!({"reference":frame.input}).to_string()}}]
    })]).await;
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, Arc::new(LargeReceiptHost)).unwrap();
    let result = session.turn("报告详情").await;
    assert!(
        result
            .as_ref()
            .is_err_and(|error| error.to_string().contains("ContextBudgetExceeded")),
        "续轮超预算应在本地明确失败：{result:?}"
    );
    assert_eq!(server.await.unwrap().len(), 1, "超预算续轮不能发到模型");
}

/// 场景：下一轮读取新快照，旧共享资料不能被当作用户历史重新注入。
/// 构造：两轮输入分别产生不同标识的报告资料，服务器正常回复。
/// 执行：同一 NormaSession 连续调用两次 turn，检查第二个实际 HTTP 请求。
/// 预期：仅新资料作为一个文档发送；旧用户原话仍作为历史保留。
#[tokio::test]
async fn shared_documents_do_not_accumulate_as_user_history() {
    let reply = serde_json::json!({"role":"assistant","content":"知道了"});
    let (endpoint, server) = sse::serve(vec![reply.clone(), reply]).await;
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, Arc::new(Host)).unwrap();
    session.turn("第一条报告").await.unwrap();
    session.turn("第二条报告").await.unwrap();
    let requests = server.await.unwrap();
    let second = requests[1]["messages"].to_string();
    assert_eq!(
        second.matches("shared_context_data").count(),
        1,
        "每次请求只带一份共享资料"
    );
    assert!(
        !second.contains("第一条报告：NOT-AN-INSTRUCTION"),
        "旧资料不进入持久历史"
    );
    assert!(
        second.contains("第二条报告：NOT-AN-INSTRUCTION"),
        "新快照资料进入本轮"
    );
    assert!(second.contains("第一条报告"), "用户原话作为真实历史保留");
}

struct MemoryHost;
impl AgentCardAdapter for MemoryHost {
    fn memory_context(
        &self,
    ) -> BoxFuture<'_, Result<Option<agent::MemorySessionContext>, AdapterError>> {
        Box::pin(async {
            Ok(Some(agent::MemorySessionContext {
                catalog: "optional-catalog ".repeat(1024),
                ..Default::default()
            }))
        })
    }
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Box::pin(async move {
            let mut frame = Host.shared_context(context).await?.unwrap();
            frame.snapshot.state.goal = Some(ContextFact {
                reference: frame.input.clone(),
                text: "REQUIRED-GOAL".into(),
            });
            Ok(Some(frame))
        })
    }
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Host.execute(command, context)
    }
}

/// 场景：可选记忆目录不能挤掉必须保留的有效工作状态。
/// 构造：默认 16k 窗口、长记忆目录，以及有来源的当前目标。
/// 执行：通过公开 Session 发送真实回环 HTTP 请求。
/// 预期：缩减可选目录后成功请求，必需目标保留且完整请求满足预算。
#[tokio::test]
async fn required_shared_state_precedes_optional_memory_catalog() {
    let (endpoint, server) = sse::serve(vec![
        serde_json::json!({"role":"assistant","content":"继续"}),
    ])
    .await;
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, Arc::new(MemoryHost)).unwrap();
    let result = session.turn("报告详情").await;
    assert!(result.is_ok(), "可选目录应让位于有效工作状态：{result:?}");
    let requests = server.await.unwrap();
    assert!(
        requests[0]["messages"]
            .to_string()
            .contains("REQUIRED-GOAL"),
        "当前目标必须进入模型请求"
    );
    assert!(
        requests[0].to_string().len() + agent::MEMORY_TOOL_BYTES <= (16_384 - 2048) * 2,
        "缩减后满足默认预算"
    );
}
