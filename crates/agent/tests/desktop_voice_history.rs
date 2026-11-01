#[path = "support/sse.rs"]
mod sse;

use agent::{
    context::*, AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    DesktopContext, NormaSession, ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use std::sync::Arc;

struct Host;

impl AgentCardAdapter for Host {
    fn shared_context<'a>(
        &'a self,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Box::pin(async {
            let scope = ContextScope {
                workspace_id: "w".into(),
                area_id: "a".into(),
                project: ContextProject::Personal,
                task_id: None,
                conversation_id: None,
            };
            Ok(Some(ContextFrame {
                input: ContextRef {
                    scope: scope.clone(),
                    event_id: "input".into(),
                    revision: 1,
                },
                snapshot: ContextSnapshot {
                    scope,
                    revision: 1,
                    state: WorkingState::default(),
                    events: vec![],
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

fn desktop_session(endpoint: String, host: Arc<dyn AgentCardAdapter>) -> NormaSession {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, host).unwrap();
    session.update_desktop_context(Some(DesktopContext::ProjectCanvas {
        workspace_id: "w".into(),
        workspace_name: "工作空间".into(),
        area_id: "a".into(),
        project_name: "项目".into(),
    }));
    session
}

fn populated_canvas() -> agent::CurrentCanvasContext {
    agent::CurrentCanvasContext {
        project_name: "项目".into(),
        workspace_root: "/fixture/project".into(),
        total_cards: 1,
        cards: vec![agent::CanvasCardContext {
            node_id: 7,
            position: 1,
            kind: agent::CanvasCardKind::Agent,
            title: "Codex".into(),
            selected: true,
            focused: true,
        }],
        conversation_tails: vec![agent::AgentConversationTail {
            definition_id: 1,
            node_id: 7,
            session_id: "fixture-session".into(),
            agent_name: "Codex".into(),
            profile: norma::protocol::AgentProfile::Codex,
            task_state: agent::AgentConversationTaskState::Done,
            last_user_message: Some(format!("USER_TAIL:{}", "文".repeat(1200))),
            last_agent_message: Some(format!("AGENT_TAIL:{}", "答".repeat(1200))),
            updated_at_unix: 1,
            availability: agent::AgentConversationAvailability::Available,
            user_message_truncated: false,
            agent_message_truncated: false,
        }],
    }
}

/// 场景：画布已有智能体及中文会话时，语音指令仍应能发到模型。
/// 构造：默认 16k 窗口、一张聚焦卡片、前后两条各 1,200 字的会话尾部。
/// 执行：通过公开 turn_streaming(Voice) 查询项目，检查实际 HTTP 请求。
/// 预期：保留用户原话、共享状态、卡片身份及可用会话片段，不报 available 0。
/// 数值：两条尾部正文共 2 × 1,200 × 3 = 7,200 UTF-8 字节，尚未计算转义。
#[tokio::test]
async fn desktop_voice_with_populated_canvas_fits_default_window() {
    let (endpoint, server) = sse::serve(vec![
        serde_json::json!({"role":"assistant","content":"已读取当前会话。"}),
    ])
    .await;
    let mut session = desktop_session(endpoint, Arc::new(Host));
    session.update_environment(populated_canvas(), vec![]);
    let (deltas, _received) = tokio::sync::mpsc::channel(16);
    let result = session
        .turn_streaming("当前项目有什么进展？", deltas, AgentInputSource::Voice)
        .await;
    assert!(
        result.is_ok(),
        "有会话的画布不能耗尽必需状态预算：{result:?}"
    );
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 1, "一条问句只发一个模型请求");
    let messages = requests[0]["messages"].to_string();
    for required in [
        "当前项目有什么进展？",
        "shared_context_data",
        "Codex",
        "USER_TAIL:",
        "AGENT_TAIL:",
    ] {
        assert!(messages.contains(required), "实际请求保留 {required}");
    }
    assert!(
        !messages.contains(&"文".repeat(1200)),
        "可选尾部应按实际字节预算缩减，不能占满固定状态空间"
    );
    assert!(
        requests[0].to_string().len() + 512 <= 28_672,
        "请求加协议余量不超过原默认窗口"
    );
}

struct RequiredStateHost;
impl AgentCardAdapter for RequiredStateHost {
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Box::pin(async move {
            let mut frame = Host.shared_context(context).await?.unwrap();
            frame.snapshot.state.goal = Some(ContextFact {
                reference: frame.input.clone(),
                text: format!("REQUIRED_GOAL:{}", "目标".repeat(320)),
            });
            Ok(Some(frame))
        })
    }
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async { Ok(AdapterResult::succeeded("TOOL_RECEIPT")) })
    }
}

/// 场景：多智能体的控制字符、引号和中文不能挤掉必需目标或阻断工具续轮。
/// 构造：六份长尾部、聚焦第一张、1,920 字节目标；模型要求一次只读状态查询。
/// 执行：公开语音入口发起工具回合，检查两次 HTTP 请求和模型返回的最终结果。
/// 预期：完整目标、聚焦会话身份和真实工具回执保留；截断可选正文后仍满足原窗口。
#[tokio::test]
async fn desktop_voice_escaped_tails_yield_to_required_state_and_tool_receipt() {
    let (endpoint, server) = sse::serve(vec![
        serde_json::json!({"role":"assistant","tool_calls":[{
            "index":0,"id":"snapshot-1","type":"function","function":{
            "name":"workspace_snapshot","arguments":"{\"scope\":\"current_project\"}"}}]}),
        serde_json::json!({"role":"assistant","content":"状态查询完成。"}),
    ])
    .await;
    let mut session = desktop_session(endpoint, Arc::new(RequiredStateHost));
    let mut canvas = populated_canvas();
    let template = canvas.conversation_tails[0].clone();
    canvas.conversation_tails = (0..6)
        .map(|index| {
            let mut tail = template.clone();
            tail.node_id += index;
            tail.session_id = format!("fixture-{index}");
            tail.last_user_message = Some(format!(
                "USER_TAIL:{index}:{}",
                "\"\\\u{0001}文".repeat(300)
            ));
            tail.last_agent_message = Some(format!("AGENT_TAIL:{index}:{}", "答".repeat(1200)));
            tail
        })
        .collect();
    session.update_environment(canvas, vec![]);
    let (deltas, _received) = tokio::sync::mpsc::channel(16);
    let result = session
        .turn_streaming("查询当前状态", deltas, AgentInputSource::Voice)
        .await
        .expect("长会话尾部应让位给必需状态并允许小型回执续轮");
    assert_eq!(result.tool_calls, 1, "只执行一次只读查询");
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2, "初始请求和工具续轮各一次");
    for body in &requests {
        let text = body["messages"].to_string();
        assert!(
            text.contains(&format!("REQUIRED_GOAL:{}", "目标".repeat(320))),
            "必须保留完整目标"
        );
        assert!(text.contains("fixture-0"), "优先保留聚焦卡片的会话身份");
        assert!(
            !text.contains(&"答".repeat(1200)),
            "完整可选正文不能挤占必需材料"
        );
        assert!(
            body.to_string().len() + 512 <= 28_672,
            "按实际转义后的请求守住窗口上限"
        );
    }
    assert!(
        requests[1]["messages"].to_string().contains("TOOL_RECEIPT"),
        "保留真实工具回执"
    );
    assert!(
        !requests[0]["messages"].to_string().contains("TOOL_RECEIPT"),
        "首轮不能伪造回执"
    );
}

/// 场景：桌面语音追问“按刚才的方案继续”需要上一轮用户原话和回答。
/// 构造：默认 16,384 token 窗口、最小共享状态，以及仅出现在首轮的两种标识。
/// 执行：同一公开 NormaSession 连续调用两次 turn_streaming(Voice)，检查实际 HTTP。
/// 预期：第二轮保留首轮完整对话，只含一份当前共享资料；首轮不能预先含助手回答。
#[tokio::test]
async fn desktop_voice_followup_keeps_previous_exchange() {
    let first_input = "比较 A 和 B 两个方案，背景代号 FIRST_VOICE_DETAIL";
    let first_reply = "建议使用方案 B，理由代号 FIRST_ASSISTANT_DETAIL";
    let (endpoint, server) = sse::serve(vec![
        serde_json::json!({"role":"assistant","content":first_reply}),
        serde_json::json!({"role":"assistant","content":"接着方案 B 继续。"}),
    ])
    .await;
    let mut session = desktop_session(endpoint, Arc::new(Host));
    for input in [first_input, "按刚才的方案继续"] {
        let (deltas, _received) = tokio::sync::mpsc::channel(16);
        session
            .turn_streaming(input, deltas, AgentInputSource::Voice)
            .await
            .expect("两轮桌面语音输入均应成功");
    }
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 2, "两次输入各产生一个模型请求");
    let second = requests[1]["messages"].to_string();
    assert!(second.contains(first_input), "追问必须保留首轮用户背景");
    assert!(second.contains(first_reply), "追问必须保留上一轮方案与理由");
    assert!(second.contains("按刚才的方案继续"), "保留当前追问");
    assert_eq!(
        second.matches("shared_context_data").count(),
        1,
        "不累积共享资料"
    );
    assert!(
        !requests[0]["messages"].to_string().contains(first_reply),
        "首轮请求不能提前包含尚未生成的助手回答"
    );
}

/// 场景：单个回答超过历史余额时必须裁掉，但不能影响后续短对话的上下文保留。
/// 构造：首个回答含 8,192 字节正文，超过桌面固定材料之外约 5 KiB 的历史余额。
/// 执行：公开语音入口依次接收超长回答、短回答和追问，观察最后实际 HTTP 请求。
/// 预期：超长回合已裁掉，最近短对话完整保留，每次请求加协议余量不超输入预算。
/// 数值：输入预算 = (16,384 - 2,048 回复 token) × 2 保守字节/token = 28,672 字节。
#[tokio::test]
async fn desktop_voice_history_drops_oversized_exchange_then_keeps_recent_one() {
    let oversized = format!("OVERSIZED_REPLY:{}", "x".repeat(8192));
    let (endpoint, server) = sse::serve(vec![
        serde_json::json!({"role":"assistant","content":oversized}),
        serde_json::json!({"role":"assistant","content":"RECENT_SHORT_REPLY"}),
        serde_json::json!({"role":"assistant","content":"继续最近的短对话。"}),
    ])
    .await;
    let mut session = desktop_session(endpoint, Arc::new(Host));
    for input in ["FIRST_REQUEST", "RECENT_REQUEST", "继续最近的短对话"] {
        let (deltas, _received) = tokio::sync::mpsc::channel(16);
        session
            .turn_streaming(input, deltas, AgentInputSource::Voice)
            .await
            .expect("裁剪旧历史后仍应成功请求");
    }
    let requests = server.await.unwrap();
    let after_oversized = requests[1]["messages"].to_string();
    assert!(
        !after_oversized.contains("FIRST_REQUEST"),
        "超长回合应整轮裁掉用户输入"
    );
    assert!(
        !after_oversized.contains("OVERSIZED_REPLY"),
        "超长回答不能进入后续请求"
    );
    let last = requests[2]["messages"].to_string();
    assert!(last.contains("RECENT_REQUEST"), "保留最近短对话的用户输入");
    assert!(
        last.contains("RECENT_SHORT_REPLY"),
        "保留最近短对话的完整助手回答"
    );
    assert!(
        requests
            .iter()
            .all(|body| body.to_string().len() + 512 <= 28_672),
        "全部实际请求均应满足原窗口预算"
    );
}

struct SnapshotHost;

impl AgentCardAdapter for SnapshotHost {
    fn shared_context<'a>(
        &'a self,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<ContextFrame>, AdapterError>> {
        Host.shared_context(context)
    }

    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            assert!(
                matches!(command, AdapterCommand::WorkspaceSnapshot { .. }),
                "仅允许只读项目状态查询"
            );
            Ok(AdapterResult::succeeded("项目快照")
                .with_snapshot(serde_json::json!({"review_proof":"r".repeat(3072)})))
        })
    }
}

/// 场景：多轮语音之后查询项目状态，旧历史不能挤掉本可容纳的工具回执。
/// 构造：先完成 6 轮各含 1,200 字节正文的回答，再返回 3 × 1,024 字节快照。
/// 执行：公开语音入口完成历史回合和一次 workspace_snapshot 工具续轮，检查实际 HTTP。
/// 预期：工具续轮成功，最近完整对话与快照保留，旧对话可裁且实际请求不超 28,672 字节。
#[tokio::test]
async fn desktop_voice_tool_after_long_history_keeps_receipt_and_recent_exchange() {
    let mut responses: Vec<_> = (0..6)
        .map(|index| {
            serde_json::json!({"role":"assistant",
            "content":format!("reply-{index}:{}", "x".repeat(1200))})
        })
        .collect();
    responses.push(serde_json::json!({"role":"assistant","tool_calls":[{
        "index":0,"id":"snapshot-1","type":"function","function":{
            "name":"workspace_snapshot","arguments":"{\"scope\":\"current_project\"}"}}]}));
    responses.push(serde_json::json!({"role":"assistant","content":"已读取项目快照。"}));
    let (endpoint, server) = sse::serve(responses).await;
    let mut session = desktop_session(endpoint, Arc::new(SnapshotHost));
    for input in (0..6)
        .map(|index| format!("request-{index}"))
        .chain(std::iter::once("查询当前项目状态".into()))
    {
        let (deltas, _received) = tokio::sync::mpsc::channel(16);
        session
            .turn_streaming(&input, deltas, AgentInputSource::Voice)
            .await
            .expect("裁剪旧历史后应能读取并返回实际项目状态");
    }
    let requests = server.await.unwrap();
    assert_eq!(requests.len(), 8, "六轮回答加一次工具回合共八个模型请求");
    let last = requests[7]["messages"].to_string();
    assert!(last.contains("request-5"), "工具续轮保留上一轮用户输入");
    assert!(last.contains("reply-5:"), "工具续轮保留上一轮助手回答");
    assert!(last.contains("review_proof"), "保留真实快照回执");
    assert!(!last.contains("request-0"), "可裁旧历史不能挤占回执空间");
    assert!(
        requests
            .iter()
            .all(|body| body.to_string().len() + 512 <= 28_672),
        "工具续轮仍须满足原窗口预算和协议余量"
    );
}
