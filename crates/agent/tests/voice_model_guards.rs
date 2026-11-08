use agent::voice_command::{validate_voice_proposals, VoiceProposal};
use agent::{
    AgentInputSource, CurrentCanvasContext, NormaSession, ProviderConfig, ProviderProtocol,
};
#[path = "support/sse.rs"]
pub mod sse;
#[path = "support/voice.rs"]
mod voice;
use serde_json::json;
use std::sync::Arc;

/// 场景：原话已经停止执行，或给出歧义/未知目标，但模型仍提议写工具。
/// 构造：两个在场成员；候选分别尝试新建和转交大脑。
/// 执行：公共整批校验；预期所有候选均拒绝，不让其他写工具绕过语义护栏。
#[test]
fn paused_and_ambiguous_commands_cannot_bypass_proposal_validation() {
    let roster = [
        voice::member(1, "Bruno", Some(11)),
        voice::member(2, "Henry", Some(22)),
    ];
    for (input, tool, args) in [
        (
            "Bruno，新建任务检查登录，不对，先别执行",
            "agent_assign",
            json!({"agent":"Bruno","task":"检查登录"}),
        ),
        (
            "Bruno 和 Henry，检查方案",
            "handoff_to_brain",
            json!({"request":"检查方案"}),
        ),
        (
            "布鲁娜，检查方案",
            "handoff_to_brain",
            json!({"request":"检查方案"}),
        ),
        (
            "让 Brunno 检查登录",
            "agent_create",
            json!({"profile":"codex","task":"检查登录"}),
        ),
        (
            "让 Brunno 检查登录",
            "agent_assign",
            json!({"workspace_id":"w1","area_id":"a1","node_id":11,"task":"检查登录"}),
        ),
    ] {
        assert!(
            validate_voice_proposals(
                input,
                &roster,
                vec![VoiceProposal {
                    tool: tool.into(),
                    arguments: args
                }]
            )
            .is_err(),
            "停止或目标未知/歧义时不能写：{input}"
        );
    }
}

/// 场景：模型没有调用任何写工具，却直接声称任务已派发。
/// 构造：同轮回环响应只有虚假完成文案，宿主记录真实副作用。
/// 执行：公开单次模型路径。
/// 预期：零副作用且返回结构化无回执反馈，不向 TTS 输出成功宣称。
#[tokio::test]
async fn tool_free_model_text_cannot_claim_a_dispatch_receipt() {
    // 三个不同动作都明确要求 Henry 执行，不能依赖动作动词白名单。
    for input in [
        "Henry，帮我检查 Bruno 写的方案",
        "Henry，帮我优化 Bruno 的方案",
        "Henry，审阅并总结 Bruno 的方案",
        "Henry，新建一个任务，修复登录并解释原因",
    ] {
        let (endpoint, server) = sse::serve_openai(vec![
            json!({"role":"assistant","content":"我已交由 Henry 负责了。"}),
        ])
        .await;
        let host = Arc::new(voice::Host::default());
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
            vec![
                voice::member(1, "Bruno", Some(11)),
                voice::member(2, "Henry", Some(22)),
            ],
        );
        let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
        let result = session
            .turn_streaming(
                input,
                sender,
                AgentInputSource::SingleVoiceCommand { generation: 1 },
            )
            .await
            .expect("应结构化拒绝虚假回执");
        assert_eq!(
            server.await.expect("服务结束").len(),
            1,
            "不能为判断回执增加模型调用"
        );
        assert!(
            host.0.lock().expect("宿主记录").is_empty(),
            "确实没有执行工具"
        );
        assert!(result.voice_feedback.is_some(), "无真实回执必须明确反馈");
        assert!(
            receiver.recv().await.is_none(),
            "虚假成功文本不能进入播报流"
        );
    }
}

/// 场景：普通进展查询的答案提到历史任务已经完成，并未声称本轮派发。
/// 构造：回环模型只返回查询答复，原话明确询问 Bruno 的进展。
/// 执行：公开单次入口。
/// 预期：查询回答正常显示；不能仅因为包含“任务已完成”就误判为无回执派发。
#[tokio::test]
async fn status_answer_is_not_mistaken_for_a_new_dispatch_receipt() {
    for input in ["Bruno，现在进展如何？", "Bruno，登录修复进展如何？"] {
        let answer = "日志显示任务已完成。";
        let (endpoint, server) =
            sse::serve(vec![json!({"role":"assistant","content":answer})]).await;
        let host = Arc::new(voice::Host::default());
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
                total_cards: 1,
                cards: vec![],
                conversation_tails: vec![],
            },
            vec![voice::member(1, "Bruno", Some(11))],
        );
        let (sender, _receiver) = tokio::sync::mpsc::channel(8);
        let result = session
            .turn_streaming(
                input,
                sender,
                AgentInputSource::SingleVoiceCommand { generation: 1 },
            )
            .await
            .expect("普通查询");
        assert_eq!(
            server.await.expect("模型结束").len(),
            1,
            "查询不增加额外模型调用"
        );
        assert_eq!(
            result.final_text, answer,
            "历史状态回答可以包含完成描述，不能用文案黑名单误挡"
        );
        assert!(result.voice_feedback.is_none(), "普通查询不是本轮执行回执");
        assert!(host.0.lock().expect("记录锁").is_empty(), "查询不写入任务");
    }
}

/// 场景：待执行写请求补齐目标后，主模型不再提议工具，却声称已经派发。
/// 构造：首轮无目标的 handoff 候选进入澄清，补答 Henry 后仅返回成功文本。
/// 执行：连续两个公开单次回合；预期以待执行草稿状态要求真实回执，不重新猜动作。
#[tokio::test]
async fn clarified_write_request_requires_a_real_receipt_even_without_a_named_original_input() {
    let (endpoint, server) = sse::serve_openai(vec![
        json!({"role":"assistant","tool_calls":[{"index":0,"id":"guess","type":"function","function":{"name":"handoff_to_brain","arguments":json!({"request":"帮我检查方案"}).to_string()}}]}),
        json!({"role":"assistant","content":"要找谁？"}),
        json!({"role":"assistant","content":"我已交由 Henry 负责了。"}),
    ]).await;
    let host = Arc::new(voice::Host::default());
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
        vec![
            voice::member(1, "Bruno", Some(11)),
            voice::member(2, "Henry", Some(22)),
        ],
    );
    let (sender, _receiver) = tokio::sync::mpsc::channel(8);
    let first = session
        .turn_streaming(
            "帮我检查方案",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 1 },
        )
        .await
        .expect("首轮应补问目标");
    assert!(
        matches!(
            first.voice_feedback,
            Some(agent::voice_command::VoiceFeedback::ClarifyTarget { .. })
        ),
        "无目标写请求必须保留为草稿"
    );
    let (sender, mut receiver) = tokio::sync::mpsc::channel(8);
    let second = session
        .turn_streaming(
            "Henry",
            sender,
            AgentInputSource::SingleVoiceCommand { generation: 2 },
        )
        .await
        .expect("补答回合");
    assert_eq!(
        server.await.expect("模型结束").len(),
        3,
        "首轮工具回合与补答规划之外不能加模型调用"
    );
    assert_eq!(
        second.voice_feedback,
        Some(agent::voice_command::VoiceFeedback::NoActionReceipt),
        "待执行草稿不能仅凭文字声称派发成功"
    );
    assert!(
        host.0.lock().expect("宿主记录").is_empty(),
        "确实没有提交任何工具"
    );
    assert!(receiver.recv().await.is_none(), "虚假成功不能进入播报流");
}
