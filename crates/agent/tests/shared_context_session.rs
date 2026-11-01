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

/// 场景：桌面项目画布的单次语音输入不能在组装共享资料时耗尽默认预算。
/// 构造：默认 16,384 token 窗口、空项目画布、真实桌面工具集和最小共享状态。
/// 执行：通过与语音回合一致的 turn_streaming(Voice) 入口发送一条简短问句。
/// 预期：只发一个模型请求，保留当前输入、共享资料和桌面工具，不报 available 0。
/// 数值：默认输入预算为 (16,384 - 2,048 回复 token) × 2 保守字节/token。
#[tokio::test]
async fn desktop_voice_command_sends_shared_context_in_default_window() {
    let (endpoint, server) = sse::serve(vec![
        serde_json::json!({"role":"assistant","content":"当前没有待处理事项。"}),
    ])
    .await;
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, Arc::new(Host)).unwrap();
    session.update_desktop_context(Some(agent::DesktopContext::ProjectCanvas {
        workspace_id: "w".into(),
        workspace_name: "工作空间".into(),
        area_id: "a".into(),
        project_name: "项目".into(),
    }));
    let (deltas, _received) = tokio::sync::mpsc::channel(16);
    let result = session
        .turn_streaming(
            "当前项目有什么进展？",
            deltas,
            agent::AgentInputSource::Voice,
        )
        .await;
    assert!(
        result.is_ok(),
        "桌面语音请求应有足够的共享资料预算：{result:?}"
    );
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 1, "单次语音输入只能产生一个模型请求");
    let body = &requests[0];
    let messages = body["messages"].to_string();
    assert!(
        messages.contains("当前项目有什么进展？"),
        "当前语音输入必须保留"
    );
    assert!(messages.contains("shared_context_data"), "必须保留共享资料");
    assert!(
        body["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["function"]["name"] == "workspace_snapshot"),
        "不能靠删除桌面状态工具规避预算错误"
    );
    assert!(
        body.to_string().len() + 512 <= (16_384 - 2048) * 2,
        "实际请求加 512 字节协议余量必须满足已预留回复的输入预算"
    );
}

struct DesktopToolHost;
impl AgentCardAdapter for DesktopToolHost {
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Box::pin(async move {
            let mut frame = Host.shared_context(context).await?.unwrap();
            // 320 × 6 UTF-8 bytes makes the required goal 1,920 bytes, so the
            // fixture covers a few-KiB state like the reported 2,850-byte error.
            frame.snapshot.state.goal = Some(ContextFact {
                reference: frame.input.clone(),
                text: "目标".repeat(320),
            });
            Ok(Some(frame))
        })
    }
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            assert_eq!(
                context.input_source,
                agent::AgentInputSource::Voice,
                "保留语音来源"
            );
            assert!(
                matches!(
                    command,
                    AdapterCommand::WorkspaceSnapshot {
                        scope: agent::WorkspaceSnapshotScope::CurrentProject,
                        area_id: None
                    }
                ),
                "只执行当前项目的只读状态查询"
            );
            Ok(AdapterResult::succeeded("当前项目暂无待办")
                .with_snapshot(serde_json::json!({"proof":"BUDGET_FIX_RECEIPT"})))
        })
    }
}

/// 场景：桌面语音请求携带数 KiB 必需状态时，仍可调用工具并基于真实回执回复。
/// 构造：默认 16k 窗口、1,920 字节目标、一次 workspace_snapshot 调用及小型回执。
/// 执行：调用公开语音流入口，回环服务器先要求工具，再返回最终回答。
/// 预期：工具只执行一次，第二次请求保留目标和回执，不被未发生的下一次工具结果挤出。
#[tokio::test]
async fn desktop_voice_tool_roundtrip_retains_required_state_and_receipt() {
    let (endpoint, server) = sse::serve(vec![
        serde_json::json!({"role":"assistant","tool_calls":[{"index":0,"id":"snapshot-1",
            "type":"function","function":{"name":"workspace_snapshot",
            "arguments":"{\"scope\":\"current_project\"}"}}]}),
        serde_json::json!({"role":"assistant","content":"当前项目暂无待办"}),
    ])
    .await;
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, Arc::new(DesktopToolHost)).unwrap();
    session.update_desktop_context(Some(agent::DesktopContext::ProjectCanvas {
        workspace_id: "w".into(),
        workspace_name: "工作空间".into(),
        area_id: "a".into(),
        project_name: "项目".into(),
    }));
    let (deltas, _received) = tokio::sync::mpsc::channel(16);
    let result = session
        .turn_streaming(
            "当前项目有什么进展？",
            deltas,
            agent::AgentInputSource::Voice,
        )
        .await
        .expect("桌面语音工具回合不能因预留预算而失败");
    assert_eq!(result.tool_calls, 1, "只能执行一次状态查询");
    assert_eq!(
        result.final_text, "当前项目暂无待办",
        "返回模型基于回执生成的回答"
    );
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2, "一个工具回合应包含初始请求和回执续轮");
    let continuation = requests[1]["messages"].to_string();
    assert!(continuation.contains("目标目标"), "续轮必须保留必需目标");
    assert!(
        continuation.contains("BUDGET_FIX_RECEIPT"),
        "续轮必须保留实际工具回执"
    );
    assert!(
        !requests[0]["messages"]
            .to_string()
            .contains("BUDGET_FIX_RECEIPT"),
        "首轮不能提前存在尚未执行的工具回执"
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
