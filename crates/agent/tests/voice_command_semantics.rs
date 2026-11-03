use agent::voice_command::{
    plan_voice_command, resolve_voice_target, validate_voice_proposals, VoiceFeedback, VoicePlan,
    VoiceProposal, VoiceTargetResolution,
};
use agent::AgentTaskState;
#[path = "support/voice.rs"]
mod voice;
use voice::{member, Host};
#[path = "support/sse.rs"]
pub mod sse;
use agent::{
    AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter, AgentInputSource,
    CurrentCanvasContext, NormaSession, ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use serde_json::json;
use std::sync::Arc;

/// 场景：用户点名 Henry 检查 Bruno 的方案，两个成员均在画布。
/// 构造：Bruno 为节点 11，Henry 为节点 22，节点号作为固定身份夹具。
/// 执行：通过目标解析公共接口处理原话。
/// 预期：执行目标为 Henry 的稳定身份，不因正文提到 Bruno 改派。
#[test]
fn named_executor_is_not_the_person_mentioned_in_the_body() {
    let roster = [member(1, "Bruno", Some(11)), member(2, "Henry", Some(22))];
    let target = resolve_voice_target("Henry，帮我检查 Bruno 写的方案", &roster);
    assert_eq!(
        target,
        VoiceTargetResolution::CanvasAgent {
            definition_id: 2,
            node_id: 22
        },
        "应解析句首执行者 Henry，并保留节点身份"
    );
    assert_ne!(
        target,
        VoiceTargetResolution::CanvasAgent {
            definition_id: 1,
            node_id: 11
        },
        "被提及的 Bruno 不能替代执行者"
    );
}

/// 场景：同名成员在画布存在两张卡片。
/// 构造：同一 Bruno 定义投影为节点 11 与 12；顺序故意固定。
/// 执行：解析点名继续的原话；预期明确报告歧义，不取首个节点。
#[test]
fn duplicate_canvas_instances_require_clarification() {
    let roster = [member(1, "Bruno", Some(11)), member(1, "Bruno", Some(12))];
    assert_eq!(
        resolve_voice_target("Bruno，继续刚才的任务", &roster),
        VoiceTargetResolution::Ambiguous,
        "同名多卡片必须保留歧义，不能依赖目录顺序选第一个"
    );
}

/// 场景：目标来源包含语音别名、画布外成员和 Norma 本身。
/// 构造：Henry 有确定别名“亨利”，Alice 未上画布；成员名不是任意字符串前缀。
/// 执行：分别调用同一目标解析接口。
/// 预期：分类保留区别，多目标与名称边界不被强行缩为一个在场成员。
#[test]
fn target_categories_preserve_aliases_scope_and_name_boundaries() {
    let mut henry = member(2, "Henry", Some(22));
    henry.speech_alias = Some("亨利".into());
    let roster = [
        member(1, "Bruno", Some(11)),
        henry,
        member(3, "Alice", None),
    ];
    let cases = [
        (
            "请让亨利检查这个方案",
            VoiceTargetResolution::CanvasAgent {
                definition_id: 2,
                node_id: 22,
            },
        ),
        (
            "找 Alice 处理",
            VoiceTargetResolution::OutsideCanvas { definition_id: 3 },
        ),
        ("Norma，解释这段代码", VoiceTargetResolution::Assistant),
        ("Henryson，继续", VoiceTargetResolution::Unspecified),
        (
            "我觉得 Bruno 的方案不错",
            VoiceTargetResolution::Unspecified,
        ),
        ("Bruno 和 Henry，各自检查", VoiceTargetResolution::Ambiguous),
    ];
    for (input, expected) in cases {
        assert_eq!(
            resolve_voice_target(input, &roster),
            expected,
            "目标分类必须保留原话含义：{input}"
        );
    }
}

/// 场景：新任务关键词出现在否定、更正或引用中。
/// 构造：复用活跃 Bruno，输入此前纯规则容易误派的原话。
/// 执行：调用单次语音规划接口，与普通肯定句作对照。
/// 预期：复杂原话交主模型，简单肯定指令仍提供候选，均不执行工具。
#[test]
fn semantic_hazards_leave_the_fast_path_without_dispatching() {
    let roster = [member(1, "Bruno", Some(11))];
    for input in [
        "Bruno，不要创建新任务，继续刚才的方案",
        "Bruno，解释一下‘新任务’是什么意思",
        "Bruno，新建任务检查登录，不对，先别执行",
    ] {
        assert_eq!(
            plan_voice_command(input, &roster),
            VoicePlan::NeedsModel,
            "复杂语义不得被旧关键词规则直接执行：{input}"
        );
    }
    assert!(
        matches!(
            plan_voice_command("Bruno，新建任务，检查登录报错", &roster),
            VoicePlan::Proposals(_)
        ),
        "简单肯定请求仍应产生候选，不额外调用模型"
    );
}

/// 场景：模型把明确的“不要创建”错误规划为新任务。
/// 构造：本机 HTTP 模型夹具返回一次 assign；宿主只记录真实副作用。
/// 执行：公开单次语音入口，走完工具规划与最终回复。
/// 预期：宿主零副作用，并返回结构化禁止新建反馈，模型不能绕过原话限制。
#[tokio::test]
async fn model_proposal_cannot_override_no_new_task_constraint() {
    let (endpoint, server) = sse::serve_openai(vec![
        json!({"role":"assistant","tool_calls":[{"index":0,"id":"proposal-1","type":"function","function":{"name":"assign_agent_task","arguments":json!({"agent":"Bruno","task":"继续刚才的登录方案"}).to_string()}}]}),
        json!({"role":"assistant","content":"任务已新建"}),
    ]).await;
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        endpoint,
        "fixture",
        Some("fixture".into()),
    );
    let host = Arc::new(Host::default());
    let mut session = NormaSession::new("project", &config, host.clone()).expect("会话夹具");
    session.update_environment(
        CurrentCanvasContext {
            project_name: "project".into(),
            workspace_root: "/fixture".into(),
            total_cards: 1,
            cards: vec![],
            conversation_tails: vec![],
        },
        vec![member(1, "Bruno", Some(11))],
    );
    let (deltas, _received) = tokio::sync::mpsc::channel(32);
    let result = session
        .turn_streaming(
            "请不要创建新任务，Bruno 只需要继续刚才的登录方案",
            deltas,
            AgentInputSource::SingleVoiceCommand { generation: 1 },
        )
        .await
        .expect("应返回结构化拒绝，而非执行错误动作");
    assert_eq!(
        server.await.expect("模型夹具应完成").len(),
        2,
        "复用原模型工具往返，不新增串行判断模型"
    );
    assert!(
        host.0.lock().expect("宿主记录锁").is_empty(),
        "模型规划完成前后均不能执行被用户禁止的新任务"
    );
    assert_eq!(
        result.voice_feedback,
        Some(agent::voice_command::VoiceFeedback::NewTaskForbidden),
        "拒绝原因须结构化，由 app 展示边界翻译"
    );
    assert_ne!(
        result.final_text, "任务已新建",
        "不能播报模型未经回执支持的成功宣称"
    );
}

/// 场景：同轮候选先正确指向 Henry，又错误指向正文里的 Bruno。
/// 构造：通用 OpenAI、严格 OpenAI、Ollama 三种回环模型响应，同一画布两名成员。
/// 执行：完整单次语音回合，核对宿主真实命令记录。
/// 预期：整批先校验，任一目标冲突就零提交，不留下首条已执行的部分副作用。
#[tokio::test]
async fn all_proposals_are_checked_before_any_executor_is_called() {
    for dialect in 0..3 {
        let calls: Vec<_> = ["Henry", "Bruno"].into_iter().enumerate().map(|(index,name)| {
            let args=json!({"agent":name,"message":"帮我检查 Bruno 写的方案"});
            json!({"index":index,"id":format!("proposal-{index}"),"type":"function","function":{"name":"reply_agent","arguments":if dialect==2 {args} else {serde_json::Value::String(args.to_string())}}})
        }).collect();
        let responses = vec![
            json!({"role":"assistant","tool_calls":calls}),
            json!({"role":"assistant","content":"两个任务均已发送"}),
        ];
        let (endpoint, server) = match dialect {
            0 => sse::serve(responses).await,
            1 => sse::serve_openai(responses).await,
            _ => sse::serve_ollama(responses).await,
        };
        let protocol = if dialect == 2 {
            ProviderProtocol::Ollama
        } else {
            ProviderProtocol::OpenAiCompatible
        };
        let config = ProviderConfig::new(protocol, endpoint, "fixture", Some("fixture".into()));
        let host = Arc::new(Host::default());
        let mut session = NormaSession::new("project", &config, host.clone()).expect("会话夹具");
        session.update_environment(
            CurrentCanvasContext {
                project_name: "project".into(),
                workspace_root: "/fixture".into(),
                total_cards: 2,
                cards: vec![],
                conversation_tails: vec![],
            },
            vec![member(1, "Bruno", Some(11)), member(2, "Henry", Some(22))],
        );
        let (deltas, _received) = tokio::sync::mpsc::channel(32);
        let result = session
            .turn_streaming(
                "Henry，帮我检查 Bruno 写的方案",
                deltas,
                AgentInputSource::SingleVoiceCommand { generation: 1 },
            )
            .await
            .expect("冲突应返回结构化反馈");
        assert_eq!(
            server.await.expect("模型夹具").len(),
            2,
            "不得为校验新增串行模型：dialect={dialect}"
        );
        assert!(
            host.0.lock().expect("宿主记录").is_empty(),
            "必须先验证整批，再开始任何提交：dialect={dialect}"
        );
        assert_eq!(
            result.voice_feedback,
            Some(agent::voice_command::VoiceFeedback::TargetMismatch),
            "Bruno 是被提及者，不能成为第二执行者"
        );
    }
}

/// 场景：用户只说继续，模型猜了一个画布成员；或原会话已经结束。
/// 构造：一个活跃 Bruno，另一份 Ended 快照；候选均指向原成员。
/// 执行：通过候选校验接口检查，不访问模型或宿主。
/// 预期：缺少目标就澄清，缺少会话就保留续聊限制，不能以新任务替代。
#[test]
fn unresolved_target_and_ended_session_do_not_become_assignments() {
    let roster = [member(1, "Bruno", Some(11))];
    let proposal = VoiceProposal {
        tool: "reply_agent".into(),
        arguments: json!({"agent":"Bruno","message":"继续刚才那个"}),
    };
    assert_eq!(
        validate_voice_proposals("继续刚才那个", &roster, vec![proposal.clone()]),
        Err(VoiceFeedback::ClarifyTarget {
            candidates: vec!["Bruno".into()]
        }),
        "模型猜测不能替代明确目标"
    );
    let mut ended = roster[0].clone();
    ended.task_state = AgentTaskState::Ended;
    assert_eq!(
        validate_voice_proposals("Bruno，继续刚才的任务", &[ended.clone()], vec![proposal]),
        Err(VoiceFeedback::MissingSession),
        "Ended 会话不能接受续聊"
    );
    let assign = VoiceProposal {
        tool: "assign_agent_task".into(),
        arguments: json!({"agent":"Bruno","task":"继续刚才的任务"}),
    };
    assert_eq!(
        validate_voice_proposals("Bruno，继续刚才的任务", &[ended], vec![assign]),
        Err(VoiceFeedback::MissingSession),
        "继续语义不能被模型降级为新建"
    );
}

/// 场景：模型自由改写任务内容，且候选等待期间目标卡片可能被重建或改名。
/// 构造：原话要求修复登录；模型候选却写发布生产；再构造节点变更与改名的目录。
/// 执行：验证候选并在提交前重新核验目标。
/// 预期：发送内容保留原话，身份变化时拒绝，不依赖同名或首个匹配。
#[test]
fn commit_keeps_original_content_and_rechecks_stable_target_identity() {
    let input = "Bruno，修复登录报错";
    let roster = [member(1, "Bruno", Some(11))];
    let proposals = vec![VoiceProposal {
        tool: "assign_agent_task".into(),
        arguments: json!({"agent":"Bruno","task":"发布生产"}),
    }];
    let commits = validate_voice_proposals(input, &roster, proposals).expect("完整候选应可验证");
    assert_eq!(
        commits[0].proposal().arguments["task"],
        input,
        "模型的自由改写不能替代原始授权内容"
    );
    assert_eq!(
        agent::voice_command::revalidate_voice_commit(&commits[0], &roster),
        Ok(()),
        "未变化的目标仍可提交"
    );
    for changed in [member(1, "Bruno", Some(12)), member(1, "Henry", Some(11))] {
        assert_eq!(
            agent::voice_command::revalidate_voice_commit(&commits[0], &[changed]),
            Err(VoiceFeedback::TargetMismatch),
            "目标节点或名称变化后，旧候选必须失效"
        );
    }
}

/// 场景：规划返回时用户已开始新录音、切换项目或修改原话。
/// 构造：generation=3 的单次指令与已验证候选，对照不同宿主快照。
/// 执行：调用提交前公共作用域校验接口。
/// 预期：只有同项目、同代次、同原话且未取消的候选可提交。
#[test]
fn host_commit_rejects_old_generations_projects_and_changed_input() {
    let input = "Bruno，修复登录报错";
    let roster = [member(1, "Bruno", Some(11))];
    let commit = validate_voice_proposals(
        input,
        &roster,
        vec![VoiceProposal {
            tool: "assign_agent_task".into(),
            arguments: json!({"agent":"Bruno","task":input}),
        }],
    )
    .expect("候选")
    .remove(0);
    let context = ToolContext {
        project_id: "project".into(),
        turn_id: "turn-3".into(),
        user_input: input.into(),
        input_source: AgentInputSource::SingleVoiceCommand { generation: 3 },
        voice_commit: None,
        contextual_delete_targets: vec![],
        tool_call_id: "voice-0".into(),
        operation_id: Some("voice-0".into()),
        cancellation: tokio_util::sync::CancellationToken::new(),
    };
    assert_eq!(
        agent::voice_command::validate_voice_commit_scope(&context, "project", 3, &commit, &roster),
        Ok(()),
        "当前有效作用域应可提交"
    );
    for (project, generation) in [("other-project", 3), ("project", 4)] {
        assert_eq!(
            agent::voice_command::validate_voice_commit_scope(
                &context, project, generation, &commit, &roster
            ),
            Err(VoiceFeedback::ScopeChanged),
            "旧项目或旧代次不得执行"
        );
    }
    let mut changed = context.clone();
    changed.user_input = "改成交给 Henry".into();
    assert_eq!(
        agent::voice_command::validate_voice_commit_scope(&changed, "project", 3, &commit, &roster),
        Err(VoiceFeedback::ScopeChanged),
        "改写原话不能复用旧候选"
    );
    context.cancellation.cancel();
    assert_eq!(
        agent::voice_command::validate_voice_commit_scope(&context, "project", 3, &commit, &roster),
        Err(VoiceFeedback::ScopeChanged),
        "取消后不得执行"
    );
}

/// 场景：代码绕过规划器，直接以单次语音来源调用写工具。
/// 构造：无验证信封的 ToolContext；宿主记录真实命令。
/// 执行：通过 NormaToolRuntime 公共入口调用 assign。
/// 预期：缺少候选验证信封时拒绝，宿主零副作用。
#[tokio::test]
async fn single_voice_write_requires_a_validated_commit_envelope() {
    let host = Arc::new(Host::default());
    let tools = agent::NormaToolRuntime::new(host.clone());
    let context = ToolContext {
        project_id: "project".into(),
        turn_id: "voice-1".into(),
        user_input: "Bruno，修复登录".into(),
        input_source: AgentInputSource::SingleVoiceCommand { generation: 1 },
        voice_commit: None,
        contextual_delete_targets: vec![],
        tool_call_id: "write-1".into(),
        operation_id: Some("write-1".into()),
        cancellation: tokio_util::sync::CancellationToken::new(),
    };
    let result = tools
        .execute(
            "assign_agent_task",
            json!({"agent":"Bruno","task":"修复登录"}),
            &context,
        )
        .await;
    assert!(
        result.is_err(),
        "缺少验证信封的写操作必须被公共执行入口拒绝"
    );
    assert!(
        host.0.lock().expect("宿主记录").is_empty(),
        "未验证调用不能抵达宿主执行器"
    );
}

/// 场景：执行者与被提及者属于不同成员，仍需主模型理解任务角色。
/// 构造：画布上 Bruno 和 Henry 均唯一；原话让 Henry 检查 Bruno 的方案。
/// 执行：公共快速规划接口；预期退出快速路径，不能沿用旧的点名即回复规则。
#[test]
fn different_executor_and_mentioned_agent_use_the_main_model() {
    let roster = [member(1, "Bruno", Some(11)), member(2, "Henry", Some(22))];
    assert_eq!(
        plan_voice_command("Henry，帮我检查 Bruno 写的方案", &roster),
        VoicePlan::NeedsModel,
        "多角色语义必须由主模型解析"
    );
}

struct NoContextHost(Host);
impl AgentCardAdapter for NoContextHost {
    fn shared_context<'a>(
        &'a self,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<Option<agent::context::ContextFrame>, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("fixture-context-unavailable".into())) })
    }
    fn memory_context(
        &self,
    ) -> BoxFuture<'_, Result<Option<agent::MemorySessionContext>, AdapterError>> {
        Box::pin(async { Err(AdapterError::Rejected("fixture-memory-unavailable".into())) })
    }
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        self.0.execute(command, context)
    }
}

/// 场景：简单明确的单次指令不应等待长期记忆或模型网络。
/// 构造：无法读取上下文的宿主和未运行的回环模型端点，Bruno 唯一在场。
/// 执行：通过公开单次入口新建任务。
/// 预期：候选经同一验证提交入口直接交宿主，最终播报只来自实际回执。
#[tokio::test]
async fn explicit_single_voice_command_skips_model_and_memory_loading() {
    let host = Arc::new(NoContextHost(Host::default()));
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "http://127.0.0.1:1/v1",
        "fixture",
        Some("fixture".into()),
    );
    let mut session = NormaSession::new("project", &config, host.clone()).expect("会话夹具");
    session.update_environment(
        CurrentCanvasContext {
            project_name: "project".into(),
            workspace_root: "/fixture".into(),
            total_cards: 1,
            cards: vec![],
            conversation_tails: vec![],
        },
        vec![member(1, "Bruno", Some(11))],
    );
    let (deltas, mut received) = tokio::sync::mpsc::channel(4);
    let result = session
        .turn_streaming(
            "Bruno，新建任务，检查登录报错",
            deltas,
            AgentInputSource::SingleVoiceCommand { generation: 1 },
        )
        .await
        .expect("直接路径不应加载不可用的上下文或连接模型");
    assert_eq!(
        host.0 .0.lock().expect("宿主记录").len(),
        1,
        "有效候选只提交一次"
    );
    assert_eq!(
        result.final_text, "fixture-received",
        "结果必须来自实际宿主回执"
    );
    assert_eq!(
        received.recv().await.as_deref(),
        Some("fixture-received"),
        "仅在真实回执后发送播报文本"
    );
}
