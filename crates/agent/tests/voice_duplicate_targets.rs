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
struct Host(Mutex<Vec<u64>>);
impl AgentCardAdapter for Host {
    fn execute<'a>(
        &'a self,
        _: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.0.lock().expect("记录锁").push(
                context
                    .voice_commit
                    .as_ref()
                    .and_then(|c| c.binding.as_ref())
                    .and_then(|b| b.node_id)
                    .expect("必须有节点绑定"),
            );
            Ok(AdapterResult::accepted("fixture-received"))
        })
    }
}

/// 场景：Bruno 有两张同名卡片，用户补答明确卡片号。
/// 构造：同一成员定义对应节点 11、12；模型仅知道名字并提议回复。
/// 执行：首轮澄清，再只回答“卡片 #12”；模型服务已经结束。
/// 预期：补问列出两个稳定节点，不取首个；补答只提交节点 12，且不再调用模型。
#[tokio::test]
async fn duplicate_names_can_be_clarified_by_stable_card_identity() {
    let (endpoint,server)=sse::serve_openai(vec![json!({"role":"assistant","tool_calls":[{"index":0,"id":"guess","type":"function","function":{"name":"reply_agent","arguments":json!({"agent":"Bruno","message":"继续刚才的任务"}).to_string()}}]}),json!({"role":"assistant","content":"是哪一张卡片？"})]).await;
    let roster = [11, 12]
        .map(|node| AgentRosterEntry {
            definition_id: 1,
            name: "Bruno".into(),
            speech_alias: None,
            profile: AgentProfile::Codex,
            enabled: true,
            active_node_id: Some(node),
            tracking: AgentTrackingMode::Managed,
            busy: false,
            task_state: AgentTaskState::Ready,
        })
        .to_vec();
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
        roster,
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    let first = session
        .turn_streaming(
            "Bruno，继续刚才的任务",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 1 },
        )
        .await
        .expect("补问");
    let Some(agent::voice_command::VoiceFeedback::ClarifyTarget { candidates }) =
        first.voice_feedback
    else {
        panic!("重名必须补问而不是拒绝或取首个");
    };
    assert_eq!(candidates.len(), 2, "必须保留两张卡片，不能按名字去重");
    assert!(
        candidates.iter().any(|name| name.contains("#11"))
            && candidates.iter().any(|name| name.contains("#12")),
        "补问必须标识可选择的稳定节点"
    );
    assert_eq!(server.await.expect("模型结束").len(), 2, "原模型规划已结束");
    for (generation, answer) in [(2, "Bruno"), (3, "布鲁娜")] {
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        let retry = session
            .turn_streaming(
                answer,
                sender,
                AgentInputSource::SingleVoiceCommand { generation },
            )
            .await
            .expect("不明补答仍保留原草稿，不再次请求模型");
        assert!(
            matches!(
                retry.voice_feedback,
                Some(agent::voice_command::VoiceFeedback::ClarifyTarget { .. })
            ),
            "不明目标应继续补问"
        );
        assert!(
            host.0.lock().expect("记录锁").is_empty(),
            "补答不明时不派发"
        );
    }
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    let second = session
        .turn_streaming(
            "卡片 #12",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 4 },
        )
        .await
        .expect("节点补答不应再次请求模型");
    assert!(
        second.voice_feedback.is_none(),
        "明确节点后应可形成合法提交"
    );
    assert_eq!(
        *host.0.lock().expect("记录锁"),
        vec![12],
        "只能提交选定节点，节点 11 不得收到任何命令"
    );
}
