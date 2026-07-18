//! L1 real-provider e2e：验证真实 Dialogue Provider 到确定性 Agent 派发回执。

use agent::{
    AgentCommand, AgentEvent, AgentProvider, AgentRosterEntry, AgentSystem, AgentTrackingMode,
    CanvasCardContext, CanvasCardKind, CardTaskBudget, CardToolHost, CardToolRequest,
    CardToolResult, CurrentCanvasContext, MemoryJournal, OpenAiCompatibleProvider, ProviderConfig,
    ProviderError, ProviderProtocol, ProviderRequest, ProviderResponse,
};
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceipt, AgentTaskReceiptStatus, CardCommand,
    CardCommandResult,
};

struct NeverCardProvider;

impl AgentProvider for NeverCardProvider {
    fn complete(&mut self, _: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        Err(ProviderError::Message(
            "deterministic delegation must not call Card Provider".to_string(),
        ))
    }
}

#[derive(Default)]
struct DispatchHost {
    commands: Vec<CardCommand>,
}

impl CardToolHost for DispatchHost {
    fn execute(
        &mut self,
        request: CardToolRequest,
    ) -> Result<CardToolResult, agent::CardToolError> {
        self.commands.push(request.command.clone());
        let CardCommand::DispatchAgentTasks { assignments, .. } = request.command else {
            return Err(agent::CardToolError::Execution(
                "real delegation e2e only accepts dispatch_agent_tasks".to_string(),
            ));
        };
        Ok(CardToolResult {
            result: CardCommandResult::AgentTasksDispatched {
                receipts: assignments
                    .into_iter()
                    .map(|assignment| AgentTaskReceipt {
                        name: assignment.name,
                        node_id: 64,
                        task: assignment.task,
                        status: AgentTaskReceiptStatus::Submitted,
                    })
                    .collect(),
            },
        })
    }
}

/// L1-DETERMINISTIC-DELEGATION-REAL-01：真实 Dialogue Provider 必须贯通自然寻址到确定性派发回执。
///
/// 数据构造（含关键数值的推导过程）：
///   real provider calls    = 1：单个 UserTurn
///   addressed agents       = 1：Bella
///   card provider calls    = 0：确定性委派不得调用 Card Provider
///   dispatch requests      = 1 batch × 1 assignment = 1
///   pump upper bound        = 4：启动(1) + 派发(1) + 聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 从显式环境变量构造真实 OpenAI-compatible Dialogue Provider
///   2. 向完整 AgentSystem 提交“我想让贝拉查看当前项目的目录结构”
///   3. 无论模型选择文字还是 card_task，确定性路由都形成 Bella assignment
///   4. CardTask 绕过第二次模型规划并由记录型 Host 返回 Submitted 回执
///
/// 预期结果：
///   - 正断言：真实服务请求成功并只产生一个 Bella 派发命令
///   - 正断言：真实 Submitted 回执让任务进入 TaskSucceeded
///   - 负断言：NeverCardProvider 不得被调用，否则任务会失败
#[test]
#[ignore = "requires an explicitly configured real OpenAI-compatible Provider"]
fn real_intent_prefixed_dialogue_reaches_deterministic_agent_dispatch_end_to_end() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        required_env("NORMA_OPENAI_COMPLIANCE_BASE_URL"),
        required_env("NORMA_OPENAI_COMPLIANCE_MODEL"),
        Some(required_env("NORMA_OPENAI_COMPLIANCE_API_KEY")),
    );
    let dialogue = OpenAiCompatibleProvider::new(config);
    let mut system = AgentSystem::new(
        "real-deterministic-delegation",
        dialogue,
        NeverCardProvider,
        DispatchHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(vec![AgentRosterEntry {
            definition_id: 1,
            name: "Bella".to_string(),
            speech_alias: Some("贝拉".to_string()),
            profile: AgentProfile::Claude,
            enabled: true,
            active_node_id: Some(64),
            tracking: AgentTrackingMode::Managed,
            busy: false,
        }]))
        .expect("有效 Bella 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "我想让贝拉查看当前项目的目录结构".to_string(),
        ))
        .expect("真实 Dialogue Provider 必须成功处理带自然意图前缀的 Bella 委派");
    // 快路径最多需要：启动(1) + 派发(1) + 聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "查看当前项目的目录结构".to_string(),
            }],
            force_untracked: false,
        }],
        "真实 Dialogue Provider 之后必须只执行一次确定性 Bella 派发"
    );
    assert!(
        system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskSucceeded { summary, .. } if summary == "Bella 已派发"
        )),
        "真实端到端流程必须由 Bella Submitted 回执进入成功终态"
    );
    assert!(
        !system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskFailed { .. })),
        "确定性委派不得调用 NeverCardProvider 或产生其他失败"
    );
}

/// L1-DETERMINISTIC-DELEGATION-REAL-02：真实 Dialogue Provider 必须为自然后置寻址返回结构化 assignment。
///
/// 数据构造（含关键数值的推导过程）：
///   real provider calls    = 1：现有 Dialogue 理解阶段
///   canvas cards          = 1：Bella(node 64)
///   addressed agents     = 1：自然口语后半句中的 Bella
///   expected assignments = 1 agent × 1 项目检查任务 = 1
///   card provider calls  = 0：结构化委派必须绕过第二个模型
///   pump upper bound      = 4：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入真实项目同形的 Agent 名册与当前画布上下文
///   2. 向真实 MiniMax 提交“任务描述在前、让 Bella 在后”的自然口语
///   3. Dialogue 输出 agent_delegation 与 Bella assignment
///   4. CardTask 绕过 NeverCardProvider，直接获得 Submitted 回执
///
/// 预期结果：
///   - 正断言：只产生一次 Bella 的确定性派发，任务正文保持简短且包含项目目标
///   - 正断言：真实回执产生“Bella 已派发”成功事件
///   - 负断言：不得调用 list_cards/list_agents 或 NeverCardProvider
#[test]
#[ignore = "requires an explicitly configured real OpenAI-compatible Provider"]
fn real_natural_tail_address_returns_structured_assignment_and_dispatches() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        required_env("NORMA_OPENAI_COMPLIANCE_BASE_URL"),
        required_env("NORMA_OPENAI_COMPLIANCE_MODEL"),
        Some(required_env("NORMA_OPENAI_COMPLIANCE_API_KEY")),
    );
    let dialogue = OpenAiCompatibleProvider::new(config);
    let mut system = AgentSystem::new(
        "real-structured-tail-address",
        dialogue,
        NeverCardProvider,
        DispatchHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(vec![AgentRosterEntry {
            definition_id: 98,
            name: "Bella".to_string(),
            speech_alias: Some("贝拉".to_string()),
            profile: AgentProfile::Codex,
            enabled: true,
            active_node_id: Some(64),
            tracking: AgentTrackingMode::Managed,
            busy: false,
        }]))
        .expect("真实 Bella 名册必须可注入");
    system
        .submit(AgentCommand::UpdateCurrentCanvasContext(
            CurrentCanvasContext {
                project_name: "plugin-demo".to_string(),
                workspace_root: "/Users/alan/Documents/codes/plugin-demo".to_string(),
                total_cards: 1,
                cards: vec![CanvasCardContext {
                    node_id: 64,
                    kind: CanvasCardKind::Agent,
                    title: "Bella".to_string(),
                    selected: false,
                    focused: false,
                }],
            },
        ))
        .expect("真实项目同形的当前画布上下文必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "帮我看一下当前项目是做什么的，然后让 Bella 检查一下".to_string(),
        ))
        .expect("真实 MiniMax 必须处理自然后置寻址委派");
    // 快路径最多需要：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "自然后置寻址必须只执行一次确定性派发"
    );
    let CardCommand::DispatchAgentTasks { assignments, .. } = &system.card_tool_host().commands[0]
    else {
        panic!("自然后置寻址必须直接执行 DispatchAgentTasks");
    };
    assert_eq!(assignments.len(), 1, "真实委派必须只包含 Bella 一项目标");
    assert_eq!(assignments[0].name, "Bella", "真实委派目标必须是 Bella");
    assert!(
        assignments[0].task.contains("项目"),
        "轻量任务正文必须保留当前项目目标，实际：{:?}",
        assignments[0].task
    );
    assert!(
        assignments[0].task.chars().count() <= 80,
        "轻量任务正文不得扩写成长篇简报，实际字符数：{}",
        assignments[0].task.chars().count()
    );
    assert!(
        system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskSucceeded { summary, .. } if summary == "Bella 已派发"
        )),
        "真实 Bella Submitted 回执必须产生派发成功事件"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(command, CardCommand::List | CardCommand::ListAgents)),
        "结构化委派不得执行 list_cards 或 list_agents"
    );
}

/// L1-DETERMINISTIC-DELEGATION-REAL-03：真实 Dialogue Provider 必须保留多 Agent 独立分工并原子派发。
///
/// 数据构造（含关键数值的推导过程）：
///   real provider calls    = 1：现有 Dialogue 理解阶段
///   addressed agents     = 2：Bella + Alan
///   distinct assignments = 2：Bella/检查前端 + Alan/检查后端
///   dispatch batches     = 1：两项任务必须原子进入同一批次
///   card provider calls  = 0：委派快路径不得请求第二个模型
///   pump upper bound      = 4：启动(1) + 批量派发(1) + 回执聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入 Bella 与 Alan 的真实同形名册
///   2. 向真实 MiniMax 提交一条包含两个独立分工的自然指令
///   3. Dialogue 输出或本地兜底恢复两个 assignments
///   4. CardTask 用一次 DispatchAgentTasks 获得两条 Submitted 回执
///
/// 预期结果：
///   - 正断言：唯一批次恰好包含 Bella 与 Alan，且前后端任务不串线
///   - 正断言：结果摘要同时包含 Bella/Alan 已派发
///   - 负断言：不得拆成多个批次、调用 Card Provider 或执行只读查询
#[test]
#[ignore = "requires an explicitly configured real OpenAI-compatible Provider"]
fn real_multi_agent_division_dispatches_one_atomic_batch() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        required_env("NORMA_OPENAI_COMPLIANCE_BASE_URL"),
        required_env("NORMA_OPENAI_COMPLIANCE_MODEL"),
        Some(required_env("NORMA_OPENAI_COMPLIANCE_API_KEY")),
    );
    let dialogue = OpenAiCompatibleProvider::new(config);
    let mut system = AgentSystem::new(
        "real-structured-multi-agent-division",
        dialogue,
        NeverCardProvider,
        DispatchHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(vec![
            AgentRosterEntry {
                definition_id: 98,
                name: "Bella".to_string(),
                speech_alias: Some("贝拉".to_string()),
                profile: AgentProfile::Codex,
                enabled: true,
                active_node_id: Some(64),
                tracking: AgentTrackingMode::Managed,
                busy: false,
            },
            AgentRosterEntry {
                definition_id: 93,
                name: "Alan".to_string(),
                speech_alias: Some("艾伦".to_string()),
                profile: AgentProfile::Codex,
                enabled: true,
                active_node_id: Some(63),
                tracking: AgentTrackingMode::Managed,
                busy: false,
            },
        ]))
        .expect("真实 Bella/Alan 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella 检查当前项目的前端，Alan 检查当前项目的后端".to_string(),
        ))
        .expect("真实 MiniMax 必须处理多 Agent 独立分工");
    // 快路径最多需要：启动(1) + 批量派发(1) + 回执聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "多 Agent 分工必须只产生一个原子派发批次"
    );
    let CardCommand::DispatchAgentTasks { assignments, .. } = &system.card_tool_host().commands[0]
    else {
        panic!("多 Agent 分工必须直接执行 DispatchAgentTasks");
    };
    assert_eq!(assignments.len(), 2, "唯一批次必须恰好包含两项任务");
    let bella = assignments
        .iter()
        .find(|assignment| assignment.name == "Bella")
        .expect("唯一批次必须包含 Bella assignment");
    let alan = assignments
        .iter()
        .find(|assignment| assignment.name == "Alan")
        .expect("唯一批次必须包含 Alan assignment");
    assert!(bella.task.contains("前端"), "Bella 必须收到前端检查任务");
    assert!(alan.task.contains("后端"), "Alan 必须收到后端检查任务");
    assert!(
        !bella.task.contains("后端"),
        "Bella 的独立分工不得混入 Alan 的后端任务"
    );
    assert!(
        !alan.task.contains("前端"),
        "Alan 的独立分工不得混入 Bella 的前端任务"
    );
    assert!(
        system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskSucceeded { summary, .. }
                if summary.contains("Bella 已派发") && summary.contains("Alan 已派发")
        )),
        "两条 Submitted 回执必须汇总为 Bella/Alan 均已派发"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(command, CardCommand::List | CardCommand::ListAgents)),
        "多 Agent 结构化委派不得执行 list_cards 或 list_agents"
    );
}

/// L1-DETERMINISTIC-DELEGATION-REAL-04：真实 Provider 的多候选误调用必须被运行时拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   candidate agents    = 2：待创建 Alan + Bob
///   singular references = 1：“让它分析 README”
///   raw provider behavior = 1：MiniMax 已实测可能返回 card_task start
///   expected tasks      = 0：候选数 2 > 可唯一解析数 1
///   expected host calls = 0：拦截必须发生在任何 CardTask 副作用前
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用真实 MiniMax 提交多候选单数指代指令
///   2. Dialogue 本地策略把该轮标记为只允许澄清
///   3. 无论 Provider 返回文本还是错误 start，都在解析/入队前移除工具调用
///   4. 检查事件、任务队列与 Host 命令
///
/// 预期结果：
///   - 正断言：真实 UserTurn 可安全返回且任务队列保持空
///   - 负断言：不得产生 TaskQueued、部分创建 Alan 或调用 Card Provider/Host
#[test]
#[ignore = "requires an explicitly configured real OpenAI-compatible Provider"]
fn real_ambiguous_multi_agent_pronoun_is_blocked_before_side_effects() {
    let config = ProviderConfig::new(
        ProviderProtocol::OpenAiCompatible,
        required_env("NORMA_OPENAI_COMPLIANCE_BASE_URL"),
        required_env("NORMA_OPENAI_COMPLIANCE_MODEL"),
        Some(required_env("NORMA_OPENAI_COMPLIANCE_API_KEY")),
    );
    let dialogue = OpenAiCompatibleProvider::new(config);
    let mut system = AgentSystem::new(
        "real-ambiguous-multi-agent-pronoun",
        dialogue,
        NeverCardProvider,
        DispatchHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );

    let events = system
        .submit(AgentCommand::UserTurn(
            "创建 Codex Agent Alan 和 Claude Agent Bob，让它分析 README".to_string(),
        ))
        .expect("真实 Provider 的多候选误调用必须由运行时安全拦截");

    assert_eq!(
        system.pending_task_count(),
        0,
        "多候选单数指代不得创建任何待执行 CardTask"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "真实 Provider 即使误调用 start，也不得产生 TaskQueued"
    );
    assert!(
        system.card_tool_host().commands.is_empty(),
        "澄清前不得部分创建 Alan、调用 Card Provider 或触碰 Host"
    );
}

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("运行真实 Provider e2e 前必须设置 {name}"))
}
