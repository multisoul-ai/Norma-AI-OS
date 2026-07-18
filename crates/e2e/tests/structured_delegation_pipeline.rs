//! L1 domain e2e：验证 Dialogue 提供的结构化 Agent assignments 必须进入确定性派发快路径。

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
    fn new(responses: impl IntoIterator<Item = ProviderResponse>) -> Self {
        Self {
            responses: responses.into_iter().collect(),
        }
    }

    fn never() -> Self {
        Self::new([])
    }
}

impl AgentProvider for ScriptedProvider {
    fn complete(&mut self, _: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.responses.pop_front().ok_or_else(|| {
            ProviderError::Message("structured delegation must not call Card Provider".to_string())
        })
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
                "structured delegation accepts only dispatch_agent_tasks".to_string(),
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

fn bella_roster() -> Vec<AgentRosterEntry> {
    vec![AgentRosterEntry {
        definition_id: 98,
        name: "Bella".to_string(),
        speech_alias: Some("贝拉".to_string()),
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(64),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }]
}

/// L1-STRUCTURED-DELEGATION-01：自然后置寻址必须使用 Dialogue assignments 确定性派发。
///
/// 数据构造（含关键数值的推导过程）：
///   user turns          = 1：任务描述在前，Bella 寻址在后
///   dialogue calls      = 1：现有 Dialogue 理解阶段
///   assignments        = 1：Bella × 1 个项目检查任务
///   card provider calls = 0：委派快路径不得请求第二个模型
///   dispatch calls      = 1 batch × 1 assignment = 1
///   pump upper bound    = 4：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Dialogue 返回 `agent_delegation` 与 Bella 的结构化 assignment
///   2. AgentSystem 按当前项目名册验证 Bella
///   3. CardTask 应直接执行唯一一次 DispatchAgentTasks
///   4. Host 返回 Bella Submitted，CardTask 以真实回执成功
///
/// 预期结果：
///   - 正断言：Host 只收到一次 Bella 的 DispatchAgentTasks
///   - 正断言：任务摘要为“Bella 已派发”
///   - 负断言：Card Provider 不得被调用，也不得执行 list_cards/list_agents
#[test]
fn natural_tail_address_uses_structured_assignment_without_card_provider() {
    let dialogue = ScriptedProvider::new([ProviderResponse {
        text: String::new(),
        tool_calls: vec![ProviderToolCall {
            id: "dialogue-structured-1".to_string(),
            name: "card_task".to_string(),
            arguments_json: serde_json::json!({
                "action": "start",
                "task_kind": "agent_delegation",
                "intent": "查看当前项目的用途和主要功能",
                "assignments": [{
                    "name": "Bella",
                    "task": "查看当前项目的用途和主要功能"
                }]
            })
            .to_string(),
        }],
    }]);
    let mut system = AgentSystem::new(
        "structured-natural-tail-address",
        dialogue,
        ScriptedProvider::never(),
        DispatchHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(bella_roster()))
        .expect("真实 Bella 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "帮我看一下当前项目是做什么的，然后让 Bella 检查一下".to_string(),
        ))
        .expect("自然后置寻址必须进入结构化委派任务");
    // 快路径最多需要：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "查看当前项目的用途和主要功能".to_string(),
            }],
            force_untracked: false,
        }],
        "结构化委派必须绕过 Card Provider，只向 Bella 派发一次"
    );
    assert!(
        system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskSucceeded { summary, .. } if summary == "Bella 已派发"
        )),
        "Bella Submitted 真实回执必须让委派任务成功"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(command, CardCommand::List | CardCommand::ListAgents)),
        "结构化委派不得退回 list_cards 或 list_agents"
    );
}

/// L1-STRUCTURED-DELEGATION-02：结构化 assignments 不得扩张本地已识别的明确目标集。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed targets   = 1：句首 Bella
///   task mentions       = 1：正文中的 Alan 仅为被检查对象
///   provider assignments = 2：故意错误返回 Bella + Alan
///   expected queued tasks = 0：目标集不一致时整批拒绝
///   expected host calls = 0：验证必须发生在任何副作用前
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用户句首明确寻址 Bella，并在任务正文中提到 Alan
///   2. Dialogue Provider 故意把 Bella 和 Alan 都输出为 assignment 目标
///   3. AgentSystem 对比本地明确寻址集合与模型 assignments
///   4. 在 CardTask 入队和 Host 调用前拒绝扩张目标集
///
/// 预期结果：
///   - 正断言：UserTurn 返回目标不匹配错误，且 pending_task_count 为 0
///   - 负断言：不得产生 TaskQueued，不得向 Alan 或 Bella 执行任何派发
#[test]
fn structured_assignments_cannot_add_agent_only_mentioned_in_task_body() {
    let dialogue = ScriptedProvider::new([ProviderResponse {
        text: String::new(),
        tool_calls: vec![ProviderToolCall {
            id: "dialogue-target-expansion-1".to_string(),
            name: "card_task".to_string(),
            arguments_json: serde_json::json!({
                "action": "start",
                "task_kind": "agent_delegation",
                "intent": "检查 Alan 写的代码",
                "assignments": [
                    {"name": "Bella", "task": "检查 Alan 写的代码"},
                    {"name": "Alan", "task": "检查自己的代码"}
                ]
            })
            .to_string(),
        }],
    }]);
    let mut roster = bella_roster();
    roster.push(AgentRosterEntry {
        definition_id: 93,
        name: "Alan".to_string(),
        speech_alias: Some("艾伦".to_string()),
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(63),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    });
    let mut system = AgentSystem::new(
        "structured-target-expansion",
        dialogue,
        ScriptedProvider::never(),
        DispatchHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(roster))
        .expect("Bella/Alan 有效名册必须可注入");

    let result = system.submit(AgentCommand::UserTurn(
        "Bella 检查 Alan 写的代码".to_string(),
    ));

    assert!(
        result
            .as_ref()
            .is_err_and(|error| error.to_string().to_lowercase().contains("target")),
        "模型扩张明确目标集时必须返回 target mismatch，实际：{result:?}"
    );
    assert_eq!(
        system.pending_task_count(),
        0,
        "目标集不一致时不得创建任何待执行 CardTask"
    );
    assert!(
        !system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "目标集不一致时不得先发 TaskQueued 再失败"
    );
    assert!(
        system.card_tool_host().commands.is_empty(),
        "正文提到的 Alan 不得成为派发目标，整批也不得产生副作用"
    );
}

/// L1-STRUCTURED-DELEGATION-03：单 Agent assignment 不得扩写轻量 intent 的任务范围。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents    = 1：Bella
///   lightweight intent = 1 句：查看当前项目的用途和主要功能
///   invented details   = 3 类：README、package.json、配置文件检查
///   expected dispatch  = 1：只携带原轻量 intent
///   pump upper bound   = 4：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Dialogue 返回正确 Bella 目标与轻量 intent
///   2. 同一响应的 assignment task 故意追加用户未要求的文件检查清单
///   3. 单 Agent 委派以 intent 作为完整任务载荷并执行确定性派发
///   4. 检查最终到达 Host 的 Bella 任务正文
///
/// 预期结果：
///   - 正断言：Bella 收到且只收到轻量 intent
///   - 负断言：最终任务不得包含 README、package.json 或配置文件等臆造范围
#[test]
fn single_agent_delegation_uses_lightweight_intent_instead_of_expanded_assignment_task() {
    let dialogue = ScriptedProvider::new([ProviderResponse {
        text: String::new(),
        tool_calls: vec![ProviderToolCall {
            id: "dialogue-expanded-task-1".to_string(),
            name: "card_task".to_string(),
            arguments_json: serde_json::json!({
                "action": "start",
                "task_kind": "agent_delegation",
                "intent": "查看当前项目的用途和主要功能",
                "assignments": [{
                    "name": "Bella",
                    "task": "查看当前项目的用途和主要功能，并检查 README、package.json 和所有配置文件"
                }]
            })
            .to_string(),
        }],
    }]);
    let mut system = AgentSystem::new(
        "structured-lightweight-single-assignment",
        dialogue,
        ScriptedProvider::never(),
        DispatchHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(bella_roster()))
        .expect("真实 Bella 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "帮我看一下当前项目是做什么的，然后让 Bella 检查一下".to_string(),
        ))
        .expect("单 Agent 结构化委派必须可入队");
    // 快路径最多需要：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "查看当前项目的用途和主要功能".to_string(),
            }],
            force_untracked: false,
        }],
        "单 Agent 委派必须以轻量 intent 作为唯一任务正文"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(
                command,
                CardCommand::DispatchAgentTasks { assignments, .. }
                    if assignments[0].task.contains("README")
                        || assignments[0].task.contains("package.json")
                        || assignments[0].task.contains("配置文件")
            )),
        "Bella 最终任务不得包含模型臆造的文件检查范围"
    );
}

/// L1-STRUCTURED-DELEGATION-04：明确后续委派子句必须覆盖模型误判的 card_operation。
///
/// 数据构造（含关键数值的推导过程）：
///   explicit clauses     = 1：“然后让 Bella 检查一下”
///   provider task_kind  = card_operation：故意重放 MiniMax 偶发误判
///   invented files      = 2：README + 主要代码文件
///   expected assignments = 1：Bella
///   card provider calls = 0：本地确定性兜底必须生效
///   pump upper bound     = 4：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用户在第二个逗号子句中明确说“然后让 Bella”
///   2. Dialogue Provider 错误输出 card_operation，并在 intent 中臆造文件范围
///   3. 本地路由从明确委派子句恢复 Bella，并替换错误模型动作
///   4. CardTask 使用原始简短任务直接派发
///
/// 预期结果：
///   - 正断言：Host 只收到一次 Bella 的确定性派发
///   - 正断言：任务正文来自原始用户语义并保持简短
///   - 负断言：不得包含 README/主要代码文件，也不得调用 Card Provider
#[test]
fn explicit_later_delegation_clause_overrides_provider_card_operation_misclassification() {
    let dialogue = ScriptedProvider::new([ProviderResponse {
        text: String::new(),
        tool_calls: vec![ProviderToolCall {
            id: "dialogue-misclassified-card-operation-1".to_string(),
            name: "card_task".to_string(),
            arguments_json: serde_json::json!({
                "action": "start",
                "task_kind": "card_operation",
                "intent": "查看当前项目的 README 或主要代码文件，了解项目功能和用途"
            })
            .to_string(),
        }],
    }]);
    let mut system = AgentSystem::new(
        "structured-later-clause-fallback",
        dialogue,
        ScriptedProvider::never(),
        DispatchHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(bella_roster()))
        .expect("真实 Bella 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "帮我看一下当前项目是做什么的，然后让 Bella 检查一下".to_string(),
        ))
        .expect("明确后续委派子句必须由本地兜底恢复");
    // 快路径最多需要：启动(1) + 派发(1) + 回执聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    assert_eq!(
        system.card_tool_host().commands,
        vec![CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Bella".to_string(),
                task: "看一下当前项目是做什么的，然后让 Bella 检查一下".to_string(),
            }],
            force_untracked: false,
        }],
        "模型误判 card_operation 时仍必须从明确子句恢复 Bella 派发"
    );
    assert!(
        !system
            .card_tool_host()
            .commands
            .iter()
            .any(|command| matches!(
                command,
                CardCommand::DispatchAgentTasks { assignments, .. }
                    if assignments[0].task.contains("README")
                        || assignments[0].task.contains("主要代码文件")
            )),
        "确定性兜底不得把模型臆造的文件范围派发给 Bella"
    );
    assert!(
        system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskSucceeded { summary, .. } if summary == "Bella 已派发"
        )),
        "本地恢复的 Bella 委派必须由真实 Submitted 回执成功收口"
    );
}
