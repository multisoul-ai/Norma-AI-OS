use super::*;
use futures::future::BoxFuture;
use std::sync::atomic::{AtomicUsize, Ordering};
#[derive(Default)]
struct Host(AtomicUsize);
impl crate::AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        _: crate::AdapterCommand,
        _: &'a ToolContext,
    ) -> BoxFuture<'a, Result<crate::AdapterResult, crate::AdapterError>> {
        Box::pin(async move {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(crate::AdapterResult::accepted("fixture-received"))
        })
    }
}

/// 场景：模型候选已经完成，但原回合在草稿安装前被取消。
/// 构造：候选提交 seam 的已取消 ToolContext，以及唯一 Henry；无真实模型或音频。
/// 执行：提交尚缺目标的候选，再通过公开下一轮入口只补答 Henry。
/// 预期：取消状态不能被新 token 覆盖；原请求不复活，宿主零副作用。
#[tokio::test]
async fn cancelled_proposal_context_cannot_install_a_fresh_voice_draft() {
    let config = crate::ProviderConfig::new(
        crate::ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:1/v1",
        "fixture",
        Some("fixture".into()),
    );
    let host = Arc::new(Host::default());
    let mut session = NormaSession::new("fixture", &config, host.clone()).expect("会话");
    session.update_environment(
        crate::CurrentCanvasContext {
            project_name: "fixture".into(),
            workspace_root: "fixture".into(),
            total_cards: 1,
            cards: vec![],
            conversation_tails: vec![],
        },
        vec![crate::AgentRosterEntry {
            definition_id: 1,
            name: "Henry".into(),
            speech_alias: None,
            profile: norma::protocol::AgentProfile::Codex,
            enabled: true,
            active_node_id: Some(11),
            tracking: crate::AgentTrackingMode::Managed,
            busy: false,
            task_state: crate::AgentTaskState::Ready,
        }],
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    let context = ToolContext {
        project_id: "fixture".into(),
        turn_id: "old".into(),
        user_input: "检查登录报错".into(),
        input_source: crate::AgentInputSource::SingleVoiceCommand { generation: 1 },
        voice_commit: None,
        contextual_delete_targets: vec![],
        tool_call_id: String::new(),
        operation_id: None,
        cancellation: cancel,
    };
    let first = session
        .submit_voice_proposals(
            vec![VoiceProposal {
                tool: "assign_agent_task".into(),
                arguments: serde_json::json!({"agent":"Henry","task":"检查登录报错"}),
            }],
            &context,
            None,
        )
        .await
        .expect("取消应结构化拒绝");
    assert_eq!(
        first.voice_feedback,
        Some(VoiceFeedback::ScopeChanged),
        "已取消的原回合不能安装新草稿 token"
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(100),
        session.turn_streaming(
            "Henry",
            sender,
            crate::AgentInputSource::SingleVoiceCommand { generation: 2 },
        ),
    )
    .await;
    assert_eq!(
        host.0.load(Ordering::SeqCst),
        0,
        "后续裸目标不能使已取消的请求复活"
    );
}
