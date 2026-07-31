use agent::{
    parse_direct_address, AdapterCommand, AdapterError, AdapterResult, AgentCardAdapter,
    AgentRosterEntry, AgentTaskState, AgentTrackingMode, CurrentCanvasContext, DirectAddressIntent,
    NormaSession, ProviderConfig, ProviderProtocol, ToolContext,
};
use futures::future::BoxFuture;
use norma::protocol::AgentProfile;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingAdapter {
    commands: Mutex<Vec<AdapterCommand>>,
}

impl AgentCardAdapter for RecordingAdapter {
    fn execute<'a>(
        &'a self,
        command: AdapterCommand,
        _context: &'a ToolContext,
    ) -> BoxFuture<'a, Result<AdapterResult, AdapterError>> {
        Box::pin(async move {
            self.commands
                .lock()
                .expect("直接寻址测试的 Adapter 记录锁不应被毒化")
                .push(command);
            Ok(AdapterResult::accepted("Alan 已接收新任务").with_task_id(71))
        })
    }
}

fn alan(active: bool) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id: 1,
        name: "Alan".to_string(),
        speech_alias: Some("阿兰".to_string()),
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: active.then_some(11),
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: agent::AgentTaskState::Ready,
    }
}

fn bella(active: bool) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id: 2,
        name: "Bella".to_string(),
        speech_alias: None,
        profile: AgentProfile::Claude,
        enabled: true,
        active_node_id: active.then_some(22),
        tracking: AgentTrackingMode::Managed,
        busy: false,
        task_state: agent::AgentTaskState::Ready,
    }
}

/// 活跃 Agent 的句首点名默认进入会话回复，而不是新任务。
#[test]
fn direct_address_active_agent_routes_to_session_reply_not_new_task() {
    let intent = parse_direct_address("Alan，按照 A 方案执行", &[alan(true)]);
    assert!(
        !matches!(intent, DirectAddressIntent::NewTask { .. }),
        "不得误判为新任务"
    );
    match intent {
        DirectAddressIntent::SessionReply { agents, message } => {
            assert_eq!(agents.len(), 1, "应只匹配 Alan");
            assert_eq!(agents[0].name, "Alan", "目标应为 Alan");
            assert_eq!(message, "按照 A 方案执行", "应剥离称呼后保留正文");
        }
        other => panic!("活跃续聊应路由为 SessionReply，实际：{other:?}"),
    }
}

/// Ctrl+C 中断后的活跃 AgentCard 必须把新的可执行请求重新派发为任务。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definitions = 1（Alan）；
///   active AgentCard = 1（node_id=11，卡片仍留在画布）；
///   task state = Ended（Ctrl+C 已中断会话）；
///   executable requests = 1（“检查 Ctrl+C 恢复路径”）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造仍有活跃卡片、但任务状态为 Ended 的 Alan。
///   2. 以句首点名向 Norma 发出一条自包含的可执行请求。
///   3. 读取直接寻址路由意图，确认它选择新任务派发而非旧会话回复。
///
/// 预期结果：
///   - 正断言：路由结果为 NewTask，且目标仍为 Alan、正文完整保留。
///   - 负断言：不得产生 SessionReply，否则消息会进入 Ended 卡片的回复队列并永久等待。
#[test]
fn ended_active_agent_routes_new_request_to_new_task() {
    let mut ended_alan = alan(true);
    ended_alan.task_state = AgentTaskState::Ended;

    let intent = parse_direct_address("Alan，检查 Ctrl+C 恢复路径", &[ended_alan]);

    assert!(
        matches!(intent, DirectAddressIntent::NewTask { .. }),
        "Ended AgentCard 的新可执行请求必须改走 NewTask，实际：{intent:?}"
    );
    assert!(
        !matches!(intent, DirectAddressIntent::SessionReply { .. }),
        "Ended AgentCard 不得走 SessionReply，否则会再次进入不可消费的回复队列"
    );
    match intent {
        DirectAddressIntent::NewTask { agents, message } => {
            assert_eq!(agents.len(), 1, "单个点名请求必须只派发给 Alan");
            assert_eq!(agents[0].name, "Alan", "新任务目标必须保留为 Alan");
            assert_eq!(
                message, "检查 Ctrl+C 恢复路径",
                "派发任务必须完整保留用户的可执行正文"
            );
        }
        other => panic!("Ended AgentCard 应路由为 NewTask，实际：{other:?}"),
    }
}

/// Ctrl+C 后经真实 NormaSession 处理的点名请求必须只调用派发适配器命令。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent definitions = 1（Alan）；
///   active AgentCard = 1（node_id=11）；
///   task state = Ended（对应 Ctrl+C 终止）；
///   Norma input = 1（“Alan，检查恢复路径”）；
///   expected adapter commands = 1（AssignAgentTask）- 0（ReplyAgent）= 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建不需要联网的 NormaSession，并注入已中断的 Alan roster 快照。
///   2. 调用公开的 `NormaSession::turn`，直接寻址分支在模型调用之前执行。
///   3. RecordingAdapter 记录 Norma 实际下发的 typed AdapterCommand。
///
/// 预期结果：
///   - 正断言：回合成功返回，并且唯一命令为 Alan 的 AssignAgentTask。
///   - 负断言：命令集合中不得存在 ReplyAgent，避免消息进入 Ended 状态的回复队列。
#[tokio::test]
async fn norma_session_assigns_ended_agent_instead_of_replying() {
    let adapter = Arc::new(RecordingAdapter::default());
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        "https://provider.example/v1",
        "test-model",
        Some("test-key".to_string()),
    );
    let mut session = NormaSession::new("project-a", &config, adapter.clone())
        .expect("合法 ProviderConfig 应创建无需联网的 NormaSession");
    let mut ended_alan = alan(true);
    ended_alan.task_state = AgentTaskState::Ended;
    session.update_environment(
        CurrentCanvasContext {
            project_name: "project-a".to_string(),
            workspace_root: "/workspace/project-a".to_string(),
            total_cards: 1,
            cards: Vec::new(),
            conversation_tails: Vec::new(),
        },
        vec![ended_alan],
    );

    let result = session
        .turn("Alan，检查恢复路径")
        .await
        .expect("已中断 AgentCard 的新任务应在不访问模型网络时被确定性处理");
    let commands = adapter
        .commands
        .lock()
        .expect("直接寻址测试的 Adapter 记录锁不应被毒化")
        .clone();

    assert_eq!(
        result.tool_calls, 0,
        "确定性直接寻址不应进入 Rig 模型工具循环"
    );
    assert_eq!(
        commands.len(),
        1,
        "Ended AgentCard 的新请求必须只执行一次 Adapter 命令"
    );
    assert_eq!(
        commands[0],
        AdapterCommand::AssignAgentTask {
            agent: "Alan".to_string(),
            task: "检查恢复路径".to_string(),
        },
        "Ended AgentCard 必须走 AssignAgentTask，并保留目标和任务正文"
    );
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command, AdapterCommand::ReplyAgent { .. })),
        "Ended AgentCard 不得调用 ReplyAgent，否则消息会卡在不可消费的队列中"
    );
}

/// 明确“新任务”措辞优先于默认续聊语义。
#[test]
fn explicit_new_task_phrase_overrides_default_reply_semantics() {
    let intent = parse_direct_address("给 Alan 一个新任务：检查支付模块", &[alan(true)]);
    match intent {
        DirectAddressIntent::NewTask { agents, message } => {
            assert_eq!(agents[0].name, "Alan", "新任务目标应为 Alan");
            assert_eq!(message, "检查支付模块", "应保留任务正文");
        }
        other => panic!("明确新任务应路由为 NewTask，实际：{other:?}"),
    }
}

/// 不活跃 + 上下文依赖请求必须零副作用拒绝。
#[test]
fn inactive_agent_context_dependent_request_is_rejected_with_zero_side_effects() {
    let continue_intent = parse_direct_address("Alan，继续刚才那个", &[alan(false)]);
    let scheme_intent = parse_direct_address("Alan，按 A 方案执行", &[alan(false)]);
    match &continue_intent {
        DirectAddressIntent::RejectContextDependent { reason, .. } => {
            assert!(
                reason.contains("还没有可续接的卡片") || reason.contains("卡片或会话"),
                "拒绝话术应指向无卡/无会话，不得引导新建，实际：{reason}"
            );
        }
        other => panic!("继续刚才那个应拒绝，实际：{other:?}"),
    }
    assert!(
        matches!(
            scheme_intent,
            DirectAddressIntent::RejectContextDependent { .. }
        ),
        "按 A 方案执行在不活跃时应拒绝，实际：{scheme_intent:?}"
    );
}

/// 普通提及不得触发回复或任务。
#[test]
fn ordinary_mention_of_agent_name_is_not_a_reply_or_task() {
    let intent = parse_direct_address("我觉得 Alan 这个名字不错", &[alan(true)]);
    assert_eq!(
        intent,
        DirectAddressIntent::NoSideEffect,
        "普通提及必须无副作用"
    );
}

/// 多 Agent 句首点名应按表达顺序形成回复目标集合。
#[test]
fn multi_agent_leading_address_preserves_order() {
    let intent = parse_direct_address("Alan 和 Bella，都按 A 方案执行", &[alan(true), bella(true)]);
    match intent {
        DirectAddressIntent::SessionReply { agents, message } => {
            assert_eq!(agents.len(), 2, "应匹配两个 Agent");
            assert_eq!(agents[0].name, "Alan", "第一目标为 Alan");
            assert_eq!(agents[1].name, "Bella", "第二目标为 Bella");
            assert!(
                message.contains("按 A 方案执行") || message.contains("都按 A 方案执行"),
                "应保留共享回复正文，实际：{message}"
            );
        }
        other => panic!("多 Agent 续聊应路由为 SessionReply，实际：{other:?}"),
    }
}

/// 状态问答不得发送消息。
#[test]
fn status_question_is_read_only() {
    let intent = parse_direct_address("Alan 刚才建议什么？", &[alan(true)]);
    assert_eq!(
        intent,
        DirectAddressIntent::NoSideEffect,
        "状态问答必须只读"
    );
}

/// 不活跃但自包含请求可自动激活派发。
#[test]
fn inactive_self_contained_request_activates_and_assigns() {
    let intent = parse_direct_address("Alan，检查 README", &[alan(false)]);
    match intent {
        DirectAddressIntent::ActivateAndAssign { agents, message } => {
            assert_eq!(agents[0].name, "Alan");
            assert_eq!(message, "检查 README");
        }
        other => panic!("自包含请求应 ActivateAndAssign，实际：{other:?}"),
    }
}
