//! L1 domain e2e：验证明确 Agent 委派不依赖 Card Provider 选择派发工具。

use agent::{
    AgentCommand, AgentEvent, AgentProvider, AgentRosterEntry, AgentSystem, AgentTrackingMode,
    CardTaskBudget, CardToolHost, CardToolRequest, CardToolResult, MemoryJournal, ProviderError,
    ProviderRequest, ProviderResponse, ProviderToolCall,
};
use norma::protocol::{
    AgentProfile, AgentTaskAssignment, AgentTaskReceipt, AgentTaskReceiptStatus, CardCommand,
    CardCommandResult,
};
use std::collections::VecDeque;

struct ScriptedProvider {
    responses: VecDeque<ProviderResponse>,
}

impl ScriptedProvider {
    fn new(responses: Vec<ProviderResponse>) -> Self {
        Self {
            responses: responses.into(),
        }
    }
}

impl AgentProvider for ScriptedProvider {
    fn complete(&mut self, _: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.responses.pop_front().ok_or_else(|| {
            ProviderError::Message("deterministic delegation provider exhausted".into())
        })
    }
}

#[derive(Default)]
struct RecordingHost {
    commands: Vec<CardCommand>,
    failed_agents: Vec<String>,
    trace_ids: Vec<String>,
    execution_error: Option<String>,
}

impl CardToolHost for RecordingHost {
    fn execute(
        &mut self,
        request: CardToolRequest,
    ) -> Result<CardToolResult, agent::CardToolError> {
        self.trace_ids.push(request.trace_id.clone());
        self.commands.push(request.command.clone());
        if let Some(error) = &self.execution_error {
            return Err(agent::CardToolError::Execution(error.clone()));
        }
        let result = match request.command {
            CardCommand::List => CardCommandResult::Listed { cards: Vec::new() },
            CardCommand::ListAgents => CardCommandResult::AgentsListed { agents: Vec::new() },
            CardCommand::DispatchAgentTasks { assignments, .. } => {
                CardCommandResult::AgentTasksDispatched {
                    receipts: assignments
                        .into_iter()
                        .map(|assignment| AgentTaskReceipt {
                            status: if self.failed_agents.contains(&assignment.name) {
                                AgentTaskReceiptStatus::Failed
                            } else {
                                AgentTaskReceiptStatus::Submitted
                            },
                            name: assignment.name,
                            node_id: 64,
                            task: assignment.task,
                        })
                        .collect(),
                }
            }
            command => panic!("确定性委派回归测试不应执行其他命令：{command:?}"),
        };
        Ok(CardToolResult { result })
    }
}

fn text_response(text: &str) -> ProviderResponse {
    ProviderResponse {
        text: text.to_string(),
        tool_calls: Vec::new(),
    }
}

fn tool_response(id: &str, name: &str) -> ProviderResponse {
    ProviderResponse {
        text: String::new(),
        tool_calls: vec![ProviderToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments_json: "{}".to_string(),
        }],
    }
}

fn bella_roster() -> Vec<AgentRosterEntry> {
    vec![AgentRosterEntry {
        definition_id: 1,
        name: "Bella".to_string(),
        speech_alias: Some("贝拉".to_string()),
        profile: AgentProfile::Claude,
        enabled: true,
        active_node_id: Some(64),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }]
}

/// L1-DETERMINISTIC-DELEGATION-01：明确委派必须直接执行派发，不能被只读工具成功短路。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents       = 1：Bella
///   expected assignments   = 1 agent × 1 task = 1
///   misleading card calls  = 2：list_cards + list_agents
///   pump upper bound        = 6：旧路径启动/执行两个工具/终轮/保护 tick
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Dialogue Provider 对明确指令漏调 card_task，由确定性路由补入委派
///   2. Card Provider 被配置为只查询卡片和 Agent 后停止，复现真实故障
///   3. AgentSystem 推进任务并观察到达 CardToolHost 的高层命令
///
/// 预期结果：
///   - 正断言：只产生一次 Bella 的 DispatchAgentTasks，任务正文已移除呼叫词
///   - 正断言：任务在真实 Submitted 回执后成功
///   - 负断言：list_cards 与 list_agents 都不得进入委派执行路径
#[test]
fn explicit_delegation_dispatches_even_when_card_provider_would_only_query() {
    let dialogue = ScriptedProvider::new(vec![text_response("我先查看有哪些卡片")]);
    let card = ScriptedProvider::new(vec![
        tool_response("list-cards-1", "list_cards"),
        tool_response("list-agents-1", "list_agents"),
        text_response("已获取卡片"),
    ]);
    let mut system = AgentSystem::new(
        "deterministic-delegation",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(bella_roster()))
        .expect("有效 Bella 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella 查看当前项目的功能".to_string(),
        ))
        .expect("明确 Bella 委派必须进入 CardTask");
    // 旧路径最多需要：启动(1) + list_cards(1) + 下一轮(1) + list_agents(1)
    // + 终轮(1) + 保护 tick(1) = 6；新快路径应更早结束。
    for _ in 0..6 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "查看当前项目的功能".to_string(),
            }],
            force_untracked: false,
        }],
        "明确委派必须绕过只读 Card Provider 路径，只向 Bella 派发一次"
    );
    assert!(
        system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskSucceeded { .. })),
        "只有收到 Bella 的 Submitted 回执后，委派 CardTask 才应成功"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(command, CardCommand::List | CardCommand::ListAgents)),
        "明确委派快路径不得执行 list_cards 或 list_agents"
    );
}

/// L1-DETERMINISTIC-DELEGATION-02：明确的逐 Agent 分工必须保留为不同 assignment。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents      = 2：Bella + Alan
///   explicit task clauses = 2：搜索前端 + 运行后端测试
///   expected assignments  = 2 agents × 1 own clause = 2
///   pump upper bound       = 4：启动(1) + 派发(1) + 完成判断(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Dialogue Provider 漏调 card_task，确定性路由识别两个句首分工目标
///   2. Dialogue 为 Bella 与 Alan 构造各自的简短 assignment
///   3. CardTask 快路径用一个原子 DispatchAgentTasks 提交整批任务
///
/// 预期结果：
///   - 正断言：Bella 只收到“搜索前端”，Alan 只收到“运行后端测试”
///   - 正断言：整批只执行一个高层派发命令
///   - 负断言：任一 Agent 都不得收到包含另一个 Agent 分工的整句任务
#[test]
fn explicit_multi_agent_division_preserves_each_agents_own_task() {
    let dialogue = ScriptedProvider::new(vec![text_response("请分别说明任务")]);
    let card = ScriptedProvider::new(Vec::new());
    let mut system = AgentSystem::new(
        "deterministic-delegation-division",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(vec![
            bella_roster()[0].clone(),
            AgentRosterEntry {
                definition_id: 2,
                name: "Alan".to_string(),
                speech_alias: Some("艾伦".to_string()),
                profile: AgentProfile::Codex,
                enabled: true,
                active_node_id: Some(65),
                tracking: AgentTrackingMode::Managed,
                busy: false,
            },
        ]))
        .expect("有效 Bella/Alan 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella 搜索前端，Alan 运行后端测试".to_string(),
        ))
        .expect("明确逐 Agent 分工必须进入 CardTask");
    // 快路径最多需要：启动(1) + 原子派发(1) + 完成判断(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![
                AgentTaskAssignment {
                    name: "Bella".to_string(),
                    task: "搜索前端".to_string(),
                },
                AgentTaskAssignment {
                    name: "Alan".to_string(),
                    task: "运行后端测试".to_string(),
                },
            ],
            force_untracked: false,
        }],
        "逐 Agent 分工必须以一个原子命令提交两条独立 assignment"
    );
    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "多 Agent 分工必须且只能执行一次高层派发"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(
                command,
                CardCommand::DispatchAgentTasks { assignments, .. }
                    if assignments.iter().any(|assignment|
                        assignment.task.contains("Bella") || assignment.task.contains("Alan"))
            )),
        "assignment 正文不得包含另一个 Agent 的寻址与分工"
    );
}

/// L1-DETERMINISTIC-DELEGATION-03：混合派发回执必须保留逐 Agent 成败事实。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents      = 2：Bella + Alan
///   submitted receipts    = 1：Bella
///   failed receipts       = 1：Alan
///   total receipts        = 1 + 1 = 2，与 assignments 数量一致
///   pump upper bound       = 4：启动(1) + 派发(1) + 聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 一次明确共同任务生成 Bella 与 Alan 两条 assignment
///   2. CardToolHost 返回 Bella Submitted、Alan Failed 的完整回执集合
///   3. CardTask 按每条回执聚合派发阶段终态
///
/// 预期结果：
///   - 正断言：产生 PartiallyDispatched，摘要同时说明 Bella 已派发与 Alan 失败
///   - 正断言：任务记录进入 PartiallyDispatched，而不是笼统成功或失败
///   - 负断言：不得产生 TaskSucceeded 或 TaskFailed 覆盖其中一侧事实
#[test]
fn mixed_dispatch_receipts_report_partial_result_without_losing_either_side() {
    let dialogue = ScriptedProvider::new(vec![text_response("收到")]);
    let card = ScriptedProvider::new(Vec::new());
    let mut system = AgentSystem::new(
        "deterministic-delegation-partial",
        dialogue,
        card,
        RecordingHost {
            commands: Vec::new(),
            failed_agents: vec!["Alan".to_string()],
            trace_ids: Vec::new(),
            execution_error: None,
        },
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(vec![
            bella_roster()[0].clone(),
            AgentRosterEntry {
                definition_id: 2,
                name: "Alan".to_string(),
                speech_alias: Some("艾伦".to_string()),
                profile: AgentProfile::Codex,
                enabled: true,
                active_node_id: Some(65),
                tracking: AgentTrackingMode::Managed,
                busy: false,
            },
        ]))
        .expect("有效 Bella/Alan 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella 和 Alan 都检查当前项目".to_string(),
        ))
        .expect("明确多 Agent 共同任务必须进入 CardTask");
    // 快路径最多需要：启动(1) + 原子派发(1) + 回执聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    let partial_summary = system.events().iter().find_map(|event| match event {
        AgentEvent::TaskPartiallyDispatched { summary, .. } => Some(summary.as_str()),
        _ => None,
    });
    assert_eq!(
        partial_summary,
        Some("Bella 已派发；Alan 派发失败"),
        "混合回执摘要必须同时保留成功 Agent 与失败 Agent"
    );
    assert_eq!(
        system.task_record(1).map(|record| &record.state),
        Some(&agent::TaskState::PartiallyDispatched),
        "混合回执必须持久化为独立 PartiallyDispatched 状态"
    );
    assert!(
        !system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskSucceeded { .. } | AgentEvent::TaskFailed { .. }
        )),
        "混合回执不得被笼统 TaskSucceeded 或 TaskFailed 覆盖"
    );
}

/// L1-DETERMINISTIC-DELEGATION-04：一次用户委派的任务记录与派发请求必须共享 TraceId。
///
/// 数据构造（含关键数值的推导过程）：
///   user intents          = 1
///   CardTask records      = 1 intent × 1 task = 1
///   dispatch requests     = 1 deterministic batch = 1
///   expected trace ids    = 1 shared id
///   pump upper bound       = 4：启动(1) + 派发(1) + 聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. UserTurn 进入 Dialogue 路由并生成一次用户意图 TraceId
///   2. 确定性委派创建 CardTask 记录并执行一个 CardToolRequest
///   3. 对比持久化任务记录与工具桥请求携带的 TraceId
///
/// 预期结果：
///   - 正断言：任务记录携带非空 TraceId
///   - 正断言：派发请求与任务记录使用完全相同的 TraceId
///   - 负断言：系统不得用空字符串或工具调用 id 代替 TraceId
#[test]
fn one_user_delegation_reuses_trace_id_for_task_record_and_dispatch_request() {
    let dialogue = ScriptedProvider::new(vec![text_response("收到")]);
    let card = ScriptedProvider::new(Vec::new());
    let mut system = AgentSystem::new(
        "deterministic-delegation-trace",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(bella_roster()))
        .expect("有效 Bella 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella 查看当前项目的功能".to_string(),
        ))
        .expect("明确 Bella 委派必须进入 CardTask");
    // 快路径最多需要：启动(1) + 派发(1) + 聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    let task_trace_id = system
        .task_record(1)
        .map(|record| record.trace_id.as_str())
        .expect("委派必须创建 task_id=1 的任务记录");
    assert!(
        !task_trace_id.is_empty(),
        "一次完整用户意图必须生成非空 TraceId"
    );
    assert_eq!(
        system.card_tool_host().trace_ids,
        vec![task_trace_id.to_string()],
        "确定性派发请求必须复用所属用户意图的 TraceId"
    );
    assert_ne!(
        task_trace_id, "delegation-dispatch-1",
        "TraceId 不得退化为局部 tool_call_id"
    );
}

/// L1-DETERMINISTIC-DELEGATION-05：派发桥失败后不得回退到 Card Provider 重新规划。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents       = 1：Bella
///   deterministic calls    = 1
///   host execution errors  = 1：dispatch bridge unavailable
///   card provider scripts  = 0：任何调用都会返回 provider exhausted
///   pump upper bound        = 4：启动(1) + 失败派发(1) + 失败收口(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 明确 Bella 委派进入确定性快路径
///   2. CardToolHost 对唯一 DispatchAgentTasks 返回执行错误
///   3. CardTask 下一 tick 根据确定性派发失败收口，不请求 Card Provider
///
/// 预期结果：
///   - 正断言：TaskFailed 保留派发桥错误
///   - 正断言：只尝试一次高层派发命令
///   - 负断言：失败原因不得变成 Card Provider exhausted
#[test]
fn deterministic_dispatch_bridge_failure_does_not_fall_back_to_card_provider() {
    let dialogue = ScriptedProvider::new(vec![text_response("收到")]);
    let card = ScriptedProvider::new(Vec::new());
    let mut system = AgentSystem::new(
        "deterministic-delegation-host-failure",
        dialogue,
        card,
        RecordingHost {
            commands: Vec::new(),
            failed_agents: Vec::new(),
            trace_ids: Vec::new(),
            execution_error: Some("dispatch bridge unavailable".to_string()),
        },
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(bella_roster()))
        .expect("有效 Bella 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella 查看当前项目的功能".to_string(),
        ))
        .expect("明确 Bella 委派必须进入 CardTask");
    // 快路径最多需要：启动(1) + 失败派发(1) + 失败收口(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    let failure = system.events().iter().find_map(|event| match event {
        AgentEvent::TaskFailed { reason, .. } => Some(reason.as_str()),
        _ => None,
    });
    assert!(
        failure.is_some_and(|reason| reason.contains("dispatch bridge unavailable")),
        "确定性派发桥错误必须成为事实性 TaskFailed 原因"
    );
    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "派发桥失败后不得重复执行或改走另一个派发路径"
    );
    assert!(
        failure.is_some_and(|reason| !reason.contains("provider exhausted")),
        "委派快路径失败不得回退调用 Card Provider"
    );
}
