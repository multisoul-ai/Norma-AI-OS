//! L1 domain e2e：验证自然呼叫从 Dialogue fallback 到高层 Agent 任务委派。

use agent::{
    AgentCommand, AgentEvent, AgentProvider, AgentRosterEntry, AgentSystem, AgentTrackingMode,
    CanvasCardContext, CanvasCardKind, CardTaskBudget, CardToolHost, CardToolRequest,
    CardToolResult, CurrentCanvasContext, MemoryJournal, ProviderError, ProviderRequest,
    ProviderResponse, ProviderToolCall,
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
        self.responses
            .pop_front()
            .ok_or_else(|| ProviderError::Message("delegation provider exhausted".to_string()))
    }
}

#[derive(Default)]
struct RecordingHost {
    commands: Vec<CardCommand>,
}

impl CardToolHost for RecordingHost {
    fn execute(
        &mut self,
        request: CardToolRequest,
    ) -> Result<CardToolResult, agent::CardToolError> {
        self.commands.push(request.command.clone());
        let result = match request.command {
            CardCommand::DispatchAgentTasks { assignments, .. } => {
                CardCommandResult::AgentTasksDispatched {
                    receipts: assignments
                        .into_iter()
                        .enumerate()
                        .map(|(index, assignment)| AgentTaskReceipt {
                            name: assignment.name,
                            node_id: (index + 101) as u64,
                            task: assignment.task,
                            status: AgentTaskReceiptStatus::Submitted,
                        })
                        .collect(),
                }
            }
            command => panic!("自然委派验收不应执行其他 CardCommand：{command:?}"),
        };
        Ok(CardToolResult { result })
    }
}

fn response_with_tool(arguments_json: &str) -> ProviderResponse {
    response_with_tool_id("dispatch-1", arguments_json)
}

fn response_with_tool_id(id: &str, arguments_json: &str) -> ProviderResponse {
    ProviderResponse {
        text: String::new(),
        tool_calls: vec![ProviderToolCall {
            id: id.to_string(),
            name: "dispatch_agent_tasks".to_string(),
            arguments_json: arguments_json.to_string(),
        }],
    }
}

fn text_response(text: &str) -> ProviderResponse {
    ProviderResponse {
        text: text.to_string(),
        tool_calls: Vec::new(),
    }
}

fn roster() -> Vec<AgentRosterEntry> {
    vec![
        AgentRosterEntry {
            definition_id: 1,
            name: "Bella".to_string(),
            speech_alias: Some("贝拉".to_string()),
            profile: AgentProfile::Claude,
            enabled: true,
            active_node_id: Some(101),
            tracking: AgentTrackingMode::Managed,
            busy: false,
        },
        AgentRosterEntry {
            definition_id: 2,
            name: "Alan".to_string(),
            speech_alias: Some("艾伦".to_string()),
            profile: AgentProfile::Codex,
            enabled: true,
            active_node_id: Some(102),
            tracking: AgentTrackingMode::Managed,
            busy: false,
        },
    ]
}

fn canvas_context() -> CurrentCanvasContext {
    CurrentCanvasContext {
        project_name: "norma-ai-os".to_string(),
        workspace_root: "/workspace/norma-ai-os".to_string(),
        total_cards: 1,
        cards: vec![CanvasCardContext {
            node_id: 101,
            kind: CanvasCardKind::Agent,
            title: "Bella".to_string(),
            selected: true,
            focused: true,
        }],
    }
}

fn run_to_completion(
    system: &mut AgentSystem<ScriptedProvider, ScriptedProvider, RecordingHost, MemoryJournal>,
) {
    // 4 = task 启动(1) + 工具执行(1) + 终轮文本(1) + 保护 tick(1)。
    for _ in 0..4 {
        let _ = system.pump();
    }
}

/// L1-DELEGATION-01：用户直接呼叫 Bella 时，应完成一次当前项目任务委派。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents = 2：Bella + Alan
///   canvas cards = 1：聚焦的 Bella Agent
///   dialogue tool calls = 0，模拟模型漏调 card_task
///   card tool calls = 1：dispatch_agent_tasks(Bella)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入当前名册和画布上下文
///   2. 提交“Bella 你去搜索当前项目”
///   3. 确定性路由补入 CardTask，随后执行高层 dispatch 命令
///
/// 预期结果：
///   - 正断言：任务进入 Succeeded，Bella 收到“搜索当前项目”
///   - 负断言：不得要求用户改说“指派任务”，也不得派给 Alan
#[test]
fn direct_bella_address_dispatches_current_project_task() {
    let dialogue = ScriptedProvider::new(vec![text_response("请改说“指派任务给 Bella”")]);
    let card = ScriptedProvider::new(vec![
        response_with_tool(r#"{"assignments":[{"name":"Bella","task":"搜索当前项目"}]}"#),
        text_response("已交给 Bella"),
    ]);
    let mut system = AgentSystem::new(
        "natural-delegation-single",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(roster()))
        .expect("有效 Bella/Alan 名册应成功注入");
    system
        .submit(AgentCommand::UpdateCurrentCanvasContext(canvas_context()))
        .expect("当前项目画布上下文应成功注入");

    let queued = system
        .submit(AgentCommand::UserTurn("Bella 你去搜索当前项目".to_string()))
        .expect("直接呼叫 Bella 应进入 CardTask");
    run_to_completion(&mut system);

    assert!(
        queued
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "直接呼叫 Bella 必须产生可审计 TaskQueued"
    );
    assert!(
        system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskSucceeded { .. })),
        "Bella 委派链路必须到达 Succeeded 终态"
    );
    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "搜索当前项目".to_string(),
            }],
            force_untracked: false,
        }],
        "应只向 Bella 派发一次当前项目搜索任务，不得误派 Alan"
    );
    assert!(
        !queued.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text } if text.contains("指派任务")
        )),
        "自然呼叫命中 fallback 后不得展示旧句式提示"
    );
}

/// L1-DELEGATION-02：句首呼叫多个 Agent 并说“都”时，应为每个目标派发同一任务。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents = 2：Bella + Alan
///   shared tasks = 1：“搜索当前项目”
///   expected assignments = 2 agents × 1 shared task = 2
///   dialogue tool calls = 0，模拟模型错误要求分别说明
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入 Bella/Alan 名册和当前项目上下文
///   2. 提交“Bella 和 Alan，你们都搜索当前项目”
///   3. fallback 启动一个 CardTask，并一次派发两条 assignment
///
/// 预期结果：
///   - 正断言：Bella 与 Alan 各收到同一任务，任务成功
///   - 负断言：不得要求分别补充任务，也不得声称两者实时协作
#[test]
fn multi_agent_shared_task_dispatches_one_assignment_per_target() {
    let dialogue = ScriptedProvider::new(vec![text_response("请分别说明他们要做什么")]);
    let card = ScriptedProvider::new(vec![
        response_with_tool(
            r#"{"assignments":[{"name":"Bella","task":"搜索当前项目"},{"name":"Alan","task":"搜索当前项目"}]}"#,
        ),
        text_response("已分别派发"),
    ]);
    let mut system = AgentSystem::new(
        "natural-delegation-multiple",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(roster()))
        .expect("有效 Bella/Alan 名册应成功注入");
    system
        .submit(AgentCommand::UpdateCurrentCanvasContext(canvas_context()))
        .expect("当前项目画布上下文应成功注入");

    let queued = system
        .submit(AgentCommand::UserTurn(
            "Bella 和 Alan，你们都搜索当前项目".to_string(),
        ))
        .expect("共同任务句式应进入 CardTask");
    run_to_completion(&mut system);

    assert!(
        system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskSucceeded { .. })),
        "多 Agent 共同任务委派必须到达 Succeeded 终态"
    );
    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![
                AgentTaskAssignment {
                    name: "Bella".to_string(),
                    task: "搜索当前项目".to_string(),
                },
                AgentTaskAssignment {
                    name: "Alan".to_string(),
                    task: "搜索当前项目".to_string(),
                },
            ],
            force_untracked: false,
        }],
        "共同任务必须展开成 Bella 与 Alan 各一条相同 assignment"
    );
    assert!(
        !queued.iter().any(|event| matches!(
            event,
            AgentEvent::DialogueTextDelta { text }
                if text.contains("分别说明") || text.contains("实时协作")
        )),
        "明确共享任务不得退回补充分工，也不得虚构实时协作"
    );
}

/// L1-DELEGATION-03：任务正文提及 Alan 时，Bella 应是唯一允许的派发目标。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents = 1：句首 Bella
///   mentioned sources = 1：任务正文中的 Alan
///   first assignments = 2：错误 Bella + Alan，应在 host 前拒绝
///   retry assignments = 1：仅 Bella，保留“Alan 写的代码”正文
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Dialogue 漏调工具，确定性 fallback 解析唯一目标 Bella
///   2. CardTask 首次错误地把 Alan 也作为目标，执行边界返回失败
///   3. CardTask 根据回执重试，仅向 Bella 派发原任务
///
/// 预期结果：
///   - 正断言：最终只有 Bella assignment 到达 host，正文完整保留 Alan
///   - 正断言：任务可在安全重试后成功
///   - 负断言：Alan assignment 不得产生任何画布副作用
#[test]
fn task_body_agent_mention_is_rejected_as_an_additional_target_before_side_effects() {
    let dialogue = ScriptedProvider::new(vec![text_response("请确认是否也要派给 Alan")]);
    let card = ScriptedProvider::new(vec![
        response_with_tool(
            r#"{"assignments":[{"name":"Bella","task":"检查 Alan 写的代码"},{"name":"Alan","task":"检查自己的代码"}]}"#,
        ),
        response_with_tool_id(
            "dispatch-2",
            r#"{"assignments":[{"name":"Bella","task":"检查 Alan 写的代码"}]}"#,
        ),
        text_response("已交给 Bella"),
    ]);
    let mut system = AgentSystem::new(
        "natural-delegation-body-mention",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(roster()))
        .expect("有效 Bella/Alan 名册应成功注入");
    system
        .submit(AgentCommand::UpdateCurrentCanvasContext(canvas_context()))
        .expect("当前项目画布上下文应成功注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella，检查 Alan 写的代码".to_string(),
        ))
        .expect("句首 Bella 应进入带唯一目标约束的 CardTask");
    // 6 = 启动首轮(1) + 拒绝错误工具(1) + 请求重试(1) + 执行正确工具(1)
    //   + 读取终轮文本(1) + 保护 tick(1)。
    for _ in 0..6 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "检查 Alan 写的代码".to_string(),
            }],
            force_untracked: false,
        }],
        "执行边界必须拒绝正文中的 Alan 目标，只允许 Bella assignment 到达 host"
    );
    assert!(
        system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskSucceeded { .. })),
        "CardTask 修正目标后应到达 Succeeded 终态"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(
                command,
                CardCommand::DispatchAgentTasks { assignments, .. }
                    if assignments.iter().any(|assignment| assignment.name == "Alan")
            )),
        "任务正文中的 Alan 不得作为 assignment 到达画布 host"
    );
}

/// L1-DELEGATION-04：逗号后“Alan 写的部分”是任务范围，不是第二个委派目标。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents = 1：句首 Bella
///   mentioned source clause = 1：“Alan 写的部分也要覆盖”
///   expected assignments = 1：仅 Bella
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 提交含逗号来源从句的 Bella 指令
///   2. Dialogue 漏调工具后进入带结构化目标的 CardTask
///   3. CardTask 向 Bella 派发完整原始范围
///
/// 预期结果：
///   - 正断言：Bella-only assignment 到达 host 且保留 Alan 来源从句
///   - 负断言：执行边界不得要求或产生 Alan assignment
#[test]
fn comma_separated_agent_source_clause_does_not_expand_target_set() {
    let dialogue = ScriptedProvider::new(vec![text_response("请确认是否也要派给 Alan")]);
    let card = ScriptedProvider::new(vec![
        response_with_tool(
            r#"{"assignments":[{"name":"Bella","task":"检查代码，Alan 写的部分也要覆盖"}]}"#,
        ),
        text_response("已交给 Bella"),
    ]);
    let mut system = AgentSystem::new(
        "natural-delegation-source-clause",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(roster()))
        .expect("有效 Bella/Alan 名册应成功注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella 检查代码，Alan 写的部分也要覆盖".to_string(),
        ))
        .expect("来源从句不应阻止 Bella 任务进入 CardTask");
    run_to_completion(&mut system);

    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "检查代码，Alan 写的部分也要覆盖".to_string(),
            }],
            force_untracked: false,
        }],
        "逗号后的 Alan 来源从句必须保留在 Bella 任务正文，而非扩展目标集"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(
                command,
                CardCommand::DispatchAgentTasks { assignments, .. }
                    if assignments.iter().any(|assignment| assignment.name == "Alan")
            )),
        "来源从句中的 Alan 不得成为 assignment"
    );
}
