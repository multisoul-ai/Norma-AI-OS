#[path = "support/sse.rs"]
pub mod sse;
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    AgentRosterEntry, AgentTaskState, AgentTrackingMode, CurrentCanvasContext, NormaSession,
    ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use norma::protocol::AgentProfile;
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Host(Mutex<Vec<(AdapterCommand, String)>>);
impl AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0
                .lock()
                .expect("记录锁")
                .push((command, context.user_input.clone()));
            Ok(AdapterResult::accepted("fixture-received"))
        })
    }
}
fn roster() -> Vec<AgentRosterEntry> {
    [(1, "Bruno", 11), (2, "Henry", 22)]
        .into_iter()
        .map(|(definition_id, name, node)| AgentRosterEntry {
            definition_id,
            name: name.into(),
            speech_alias: None,
            profile: AgentProfile::Codex,
            enabled: true,
            active_node_id: Some(node),
            tracking: AgentTrackingMode::Managed,
            busy: false,
            task_state: AgentTaskState::Ready,
        })
        .collect()
}

/// 场景：首次录音只说任务正文，第二次仅补答 Henry。
/// 构造：模型先猜 Bruno；补答时服务已关闭，宿主记录命令和实际本轮输入。
/// 执行：连续调用两轮公开单次入口。
/// 预期：第一轮零提交，第二轮保留原正文、只交给 Henry，不再调用模型，也不伪造本轮原话。
#[tokio::test]
async fn target_only_answer_preserves_the_pending_task_without_another_model_call() {
    let (endpoint,server)=sse::serve_openai(vec![
        json!({"role":"assistant","tool_calls":[{"index":0,"id":"guess","type":"function","function":{"name":"assign_agent_task","arguments":json!({"agent":"Bruno","task":"检查登录报错"}).to_string()}}]}),
        json!({"role":"assistant","content":"应该发给谁？"}),
    ]).await;
    let host = Arc::new(Host::default());
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, host.clone()).expect("会话");
    session.update_environment(
        CurrentCanvasContext {
            project_name: "project".into(),
            workspace_root: "/fixture".into(),
            total_cards: 2,
            cards: vec![],
            conversation_tails: vec![],
        },
        roster(),
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    let first = session
        .turn_streaming(
            "检查登录报错",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 1 },
        )
        .await
        .expect("先补问目标");
    assert!(
        matches!(
            first.voice_feedback,
            Some(agent::voice_command::VoiceFeedback::ClarifyTarget { .. })
        ),
        "猜测的目标不能执行"
    );
    assert!(
        host.0.lock().expect("记录锁").is_empty(),
        "补问前不得创建任务"
    );
    assert_eq!(
        server.await.expect("服务结束").len(),
        2,
        "只复用现有模型规划回合"
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    let second = session
        .turn_streaming(
            "Henry",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 2 },
        )
        .await
        .expect("目标补答不得再次连接已关闭的模型");
    assert!(second.voice_feedback.is_none(), "唯一的目标补答应可提交");
    assert_eq!(
        *host.0.lock().expect("记录锁"),
        vec![(
            AdapterCommand::AssignAgentTask {
                agent: "Henry".into(),
                task: "检查登录报错".into()
            },
            "Henry".into()
        )],
        "正文来自原录音，本轮 ToolContext 仍必须是目标补答"
    );
}

struct FailingReceiptHost;
impl AgentCardAdapter for FailingReceiptHost {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async {
            Ok(AdapterResult::succeeded("incorrect-success").with_reply_status("failed"))
        })
    }
}

/// 场景：宿主返回外层 succeeded，但回复队列明确记录 failed。
/// 构造：快速路径的 Bruno 续聊与失败队列回执，不连接模型。
/// 执行：公开单次语音入口。
/// 预期：返回结构化失败，不播报“已发送”，也不自动重发。
#[tokio::test]
async fn failed_reply_receipt_is_never_announced_as_success() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:1/v1",
        "fixture",
        Some("fixture".into()),
    );
    let mut session =
        NormaSession::new("project", &config, Arc::new(FailingReceiptHost)).expect("会话");
    session.update_environment(
        CurrentCanvasContext {
            project_name: "project".into(),
            workspace_root: "/fixture".into(),
            total_cards: 2,
            cards: vec![],
            conversation_tails: vec![],
        },
        roster(),
    );
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    let result = session
        .turn_streaming(
            "Bruno，继续刚才的任务",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 1 },
        )
        .await
        .expect("失败回执应结构化呈现");
    assert!(
        result.voice_feedback.is_some(),
        "reply_status=failed 不得当作成功"
    );
    assert_ne!(
        result.final_text, "incorrect-success",
        "不能播报与实际队列状态冲突的成功文本"
    );
    assert!(
        receiver.recv().await.is_none(),
        "失败时不得提前发送成功播报"
    );
}

/// 场景：补答前用户已取消，或画布目标节点已重建。
/// 构造：两种真实回环模型协议先产生目标歧义；随后取消或更新目录。
/// 执行：只补答 Henry。
/// 预期：原草稿失效，不再请求已关闭的模型，宿主仍零副作用。
#[tokio::test]
async fn cancelled_or_changed_drafts_never_resume() {
    for cancelled in [true, false] {
        let args = json!({"agent":"Bruno","task":"检查登录报错"});
        let responses = vec![
            json!({"role":"assistant","tool_calls":[{"index":0,"id":"guess","type":"function","function":{"name":"assign_agent_task","arguments":if cancelled {serde_json::Value::String(args.to_string())} else {args}}}]}),
            json!({"role":"assistant","content":"应该发给谁？"}),
        ];
        let (endpoint, server) = if cancelled {
            sse::serve(responses).await
        } else {
            sse::serve_ollama(responses).await
        };
        let protocol = if cancelled {
            ProviderProtocol::OpenAiCompatible
        } else {
            ProviderProtocol::Ollama
        };
        let host = Arc::new(Host::default());
        let config = ProviderConfig::new(protocol, endpoint, "fixture", Some("fixture".into()));
        let mut session = NormaSession::new("project", &config, host.clone()).expect("会话");
        let canvas = CurrentCanvasContext {
            project_name: "project".into(),
            workspace_root: "/fixture".into(),
            total_cards: 2,
            cards: vec![],
            conversation_tails: vec![],
        };
        session.update_environment(canvas.clone(), roster());
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        let first = session
            .turn_streaming(
                "检查登录报错",
                sender,
                AgentInputSource::SingleVoiceCommand { generation: 1 },
            )
            .await
            .expect("先补问目标");
        assert!(first.voice_feedback.is_some(), "目标不明确必须先补问");
        assert_eq!(server.await.expect("服务结束").len(), 2, "原规划回合结束");
        if cancelled {
            session.cancel_handle().cancel();
        } else {
            let mut changed = roster();
            changed[1].active_node_id = Some(33);
            session.update_environment(canvas, changed);
        }
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        let result = session
            .turn_streaming(
                "Henry",
                sender,
                AgentInputSource::SingleVoiceCommand { generation: 2 },
            )
            .await
            .expect("失效草稿应直接拒绝");
        assert_eq!(
            result.voice_feedback,
            Some(agent::voice_command::VoiceFeedback::ScopeChanged),
            "取消或目录变更使草稿失效"
        );
        assert!(
            host.0.lock().expect("记录锁").is_empty(),
            "旧草稿不得在新状态产生写副作用"
        );
    }
}

struct PartialHost(std::sync::atomic::AtomicUsize);
impl AgentCardAdapter for PartialHost {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            if self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Ok(AdapterResult::accepted("first-received"))
            } else {
                Err(AdapterError::VoiceCommand(
                    agent::voice_command::VoiceFeedback::ScopeChanged,
                ))
            }
        })
    }
}

/// 场景：第一项已被宿主接收，第二项提交前作用域变更。
/// 构造：同一 Bruno 的两个不同动作（新建、补充）和先成功后拒绝的宿主。
/// 执行：完整单次模型候选提交流程。
/// 预期：反馈明确记为 1/2，不能谎称整批未发送或已经回滚。
#[tokio::test]
async fn partial_submission_reports_the_number_actually_received() {
    let calls:Vec<_>=[("assign_agent_task",json!({"agent":"Bruno","task":"检查 Henry 方案"})),("reply_agent",json!({"agent":"Bruno","message":"补充测试"}))].into_iter().enumerate().map(|(index,(name,args))|json!({"index":index,"id":format!("proposal-{index}"),"type":"function","function":{"name":name,"arguments":args.to_string()}})).collect();
    let (endpoint, server) = sse::serve_openai(vec![
        json!({"role":"assistant","tool_calls":calls}),
        json!({"role":"assistant","content":"全部发送成功"}),
    ])
    .await;
    let host = Arc::new(PartialHost(std::sync::atomic::AtomicUsize::new(0)));
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, host.clone()).expect("会话");
    session.update_environment(
        CurrentCanvasContext {
            project_name: "project".into(),
            workspace_root: "/fixture".into(),
            total_cards: 2,
            cards: vec![],
            conversation_tails: vec![],
        },
        roster(),
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    let result = session
        .turn_streaming(
            "Bruno，新建任务检查 Henry 的方案，再补充测试",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 1 },
        )
        .await
        .expect("部分回执应结构化返回");
    assert_eq!(
        server.await.expect("服务结束").len(),
        2,
        "不为部分结果追加模型调用"
    );
    assert_eq!(
        result.voice_feedback,
        Some(agent::voice_command::VoiceFeedback::Submission {
            completed: 1,
            total: 2,
            uncertain: false
        }),
        "应说明已经接收一项，不能声称整批未发送"
    );
    assert_eq!(
        host.0.load(std::sync::atomic::Ordering::SeqCst),
        2,
        "一次成功、一次拒绝，不自动重试"
    );
    assert_ne!(
        result.final_text, "全部发送成功",
        "模型规划结束的文本不能冒充实际回执"
    );
}

/// 场景：首轮模型提议转交大脑但目标不明，用户下一轮明确选择 Henry。
/// 构造：两轮既有模型规划回合，第二轮返回 Henry 回复；记录实际模型输入与宿主正文。
/// 执行：通过公开单次入口补答，保留原请求与独立补答来源。
/// 预期：需要重新规划时只走本轮现有主模型，原正文不能被裸目标替代。
#[tokio::test]
async fn target_clarification_replans_unbound_tool_with_original_request() {
    let (endpoint,server)=sse::serve_openai(vec![
        json!({"role":"assistant","tool_calls":[{"index":0,"id":"guess","type":"function","function":{"name":"handoff_to_brain","arguments":json!({"request":"检查登录报错"}).to_string()}}]}),
        json!({"role":"assistant","content":"要找谁？"}),
        json!({"role":"assistant","tool_calls":[{"index":0,"id":"resolved","type":"function","function":{"name":"reply_agent","arguments":json!({"agent":"Henry","message":"检查登录报错"}).to_string()}}]}),
        json!({"role":"assistant","content":"已发送"}),
    ]).await;
    let host = Arc::new(Host::default());
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, host.clone()).expect("会话");
    session.update_environment(
        CurrentCanvasContext {
            project_name: "project".into(),
            workspace_root: "/fixture".into(),
            total_cards: 2,
            cards: vec![],
            conversation_tails: vec![],
        },
        roster(),
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    let first = session
        .turn_streaming(
            "检查登录报错",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 1 },
        )
        .await
        .expect("首轮补问");
    assert!(
        matches!(
            first.voice_feedback,
            Some(agent::voice_command::VoiceFeedback::ClarifyTarget { .. })
        ),
        "无目标的转交不能执行"
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    session
        .turn_streaming(
            "Henry",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 2 },
        )
        .await
        .expect("目标补答");
    let requests = server.await.expect("模型结束");
    assert_eq!(
        requests.len(),
        4,
        "两轮分别使用原有工具规划回合，不增加串行纠错模型"
    );
    let user = requests[2]["messages"]
        .as_array()
        .expect("消息")
        .iter()
        .rev()
        .find(|m| m["role"] == "user")
        .expect("第二轮用户数据")
        .to_string();
    assert!(
        user.contains("original_input") && user.contains("clarification_input"),
        "主模型应收到分开的原请求与补答数据，不能只收到 Henry"
    );
    assert_eq!(
        *host.0.lock().expect("记录锁"),
        vec![(
            AdapterCommand::ReplyAgent {
                agent: "Henry".into(),
                message: "检查登录报错".into()
            },
            "Henry".into()
        )],
        "只提交一次原正文，实际本轮输入仍为 Henry"
    );
}
