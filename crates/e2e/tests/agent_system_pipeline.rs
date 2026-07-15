//! L1 domain e2e：从明确 Dialogue 用户意图到 Card Subagent，再到可审计卡片命令。
//!
//! 该文件验证 AgentSystem 的公开 seam，不声称启动真实 `.app`；真实 GPUI/L3 流程仍需
//! 在真实麦克风、Provider 和 TTS 环境中单独运行。

use agent::{
    AgentCommand, AgentEvent, AgentProvider, AgentSystem, CardTaskBudget, CardToolHost,
    CardToolRequest, CardToolResult, MemoryJournal, ProviderError, ProviderRequest,
    ProviderResponse, ProviderToolCall,
};
use norma::protocol::{
    AgentTaskAssignment, AgentTaskReceipt, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
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
            .ok_or_else(|| ProviderError::Message("e2e provider exhausted".to_string()))
    }
}

#[derive(Default)]
struct RuntimeLikeHost {
    created: usize,
    commands: Vec<CardCommand>,
}

impl CardToolHost for RuntimeLikeHost {
    fn execute(
        &mut self,
        request: CardToolRequest,
    ) -> Result<CardToolResult, agent::CardToolError> {
        self.commands.push(request.command.clone());
        let result = match request.command {
            CardCommand::Create { .. } => {
                self.created += 1;
                CardCommandResult::Created {
                    node_id: self.created as u64,
                }
            }
            CardCommand::SendText { node_id, .. } => CardCommandResult::Sent { node_id },
            CardCommand::List => CardCommandResult::Listed { cards: Vec::new() },
            CardCommand::Focus { node_id } => CardCommandResult::Focused { node_id },
            CardCommand::Delete { node_id } => CardCommandResult::Deleted { node_id },
            CardCommand::ListAgents => CardCommandResult::AgentsListed { agents: Vec::new() },
            CardCommand::CreateAgent { name, activate, .. } => {
                self.created += 1;
                CardCommandResult::AgentCreated {
                    definition_id: self.created as i64,
                    name: name.unwrap_or_else(|| "Alan".to_string()),
                    node_id: activate.then_some(self.created as u64),
                }
            }
            CardCommand::ActivateAgent { name } => CardCommandResult::AgentActivated {
                definition_id: 1,
                name,
                node_id: 1,
                reused: true,
            },
            CardCommand::DispatchAgentTasks { assignments, .. } => {
                CardCommandResult::AgentTasksDispatched {
                    receipts: assignments
                        .into_iter()
                        .map(|assignment| AgentTaskReceipt {
                            name: assignment.name,
                            node_id: 1,
                            task: assignment.task,
                            status: AgentTaskReceiptStatus::Submitted,
                        })
                        .collect(),
                }
            }
        };
        Ok(CardToolResult { result })
    }
}

fn call(id: &str, name: &str, arguments_json: &str) -> ProviderToolCall {
    ProviderToolCall {
        id: id.to_string(),
        name: name.to_string(),
        arguments_json: arguments_json.to_string(),
    }
}

fn tools(calls: Vec<ProviderToolCall>) -> ProviderResponse {
    ProviderResponse {
        text: String::new(),
        tool_calls: calls,
    }
}

/// L1-A-1：明确语音意图应在 queued receipt 后完成 create + send_text submit，并落审计。
///
/// 数据构造：Dialogue Provider 只返回确认文本，证明漏调用时会 fallback；Subagent 产生 create_agent 与 dispatch_agent_tasks。
/// 执行过程：submit UserTurn 后 pump 直到无新事件，再观察 host 与 MemoryJournal。
/// 预期结果：task 成功、runtime-like host 的节点数增加、提交标记为 true、审计 task id 一致。
#[test]
fn voice_intent_reaches_runtime_like_host_and_journal() {
    let dialogue = ScriptedProvider::new(vec![ProviderResponse {
        text: "请确认要创建哪种 Agent".to_string(),
        tool_calls: Vec::new(),
    }]);
    let card = ScriptedProvider::new(vec![
        tools(vec![call(
            "card-1",
            "create_agent",
            r#"{"name":"Alan","profile":"codex","activate":true}"#,
        )]),
        tools(vec![call(
            "card-2",
            "dispatch_agent_tasks",
            r#"{"assignments":[{"name":"Alan","task":"分析 README"}]}"#,
        )]),
        ProviderResponse {
            text: "完成".to_string(),
            tool_calls: Vec::new(),
        },
    ]);
    let mut system = AgentSystem::new(
        "project-e2e",
        dialogue,
        card,
        RuntimeLikeHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );

    let queued = system
        .submit(AgentCommand::UserTurn(
            "创建 Codex 并发送分析 README".to_string(),
        ))
        .expect("明确创建意图应确定性产生 queued receipt");
    let task_id = queued
        .iter()
        .find_map(|event| match event {
            AgentEvent::TaskQueued { task_id, .. } => Some(*task_id),
            _ => None,
        })
        .expect("queued receipt 必须包含 task id");
    assert!(
        system.card_tool_host().commands.is_empty(),
        "queued receipt 返回时不得已有画布副作用"
    );

    // 6 = 启动/首轮响应、create、第二轮响应、send_text、终轮文本和一次保护 tick。
    for _ in 0..6 {
        let _ = system.pump();
    }

    assert!(
        system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskSucceeded { task_id: id, .. } if *id == task_id
        )),
        "CardTask 应到达 Succeeded 终态"
    );
    assert_eq!(
        system.card_tool_host().created,
        1,
        "应只创建一个 Agent 节点"
    );
    assert_eq!(
        system.card_tool_host().commands[1],
        CardCommand::DispatchAgentTasks {
            assignments: vec![AgentTaskAssignment {
                name: "Alan".to_string(),
                task: "分析 README".to_string(),
            }],
            force_untracked: false,
        },
        "第二个高层命令应按定义名称委派 README 分析任务"
    );
    assert!(
        system
            .journal()
            .entries()
            .iter()
            .any(|entry| entry.task_id() == task_id && entry.tool_call_id() == Some("card-2")),
        "send_text 工具审计应关联同一个 task id"
    );
}

/// L1-A-2：一个语音 turn 的多个 card_task start 必须按队列顺序全部执行。
///
/// 数据构造：Dialogue Provider 在同一响应中返回两个 start；两个独立 Card Subagent 回合各创建
/// 一个 terminal 后结束。RuntimeLikeHost 为每次 Create 分配递增 node id。
/// 执行过程：提交一次 UserTurn，验证两个 queued receipt；连续 pump 直到队列清空。
/// 预期结果：两个 Terminal Create 均执行且 task #1 先成功、task #2 后成功；第二任务不与第一任务并行。
#[test]
fn one_voice_turn_queues_and_serially_executes_multiple_card_tasks() {
    let dialogue = ScriptedProvider::new(vec![tools(vec![
        call(
            "dialogue-create-1",
            "card_task",
            r#"{"action":"start","intent":"创建第一个终端"}"#,
        ),
        call(
            "dialogue-create-2",
            "card_task",
            r#"{"action":"start","intent":"创建第二个终端"}"#,
        ),
    ])]);
    let card = ScriptedProvider::new(vec![
        tools(vec![call(
            "card-create-1",
            "create_card",
            r#"{"kind":"terminal"}"#,
        )]),
        ProviderResponse {
            text: "第一个终端已创建".to_string(),
            tool_calls: Vec::new(),
        },
        tools(vec![call(
            "card-create-2",
            "create_card",
            r#"{"kind":"terminal"}"#,
        )]),
        ProviderResponse {
            text: "第二个终端已创建".to_string(),
            tool_calls: Vec::new(),
        },
    ]);
    let mut system = AgentSystem::new(
        "project-e2e-multiple-tasks",
        dialogue,
        card,
        RuntimeLikeHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );

    let queued = system
        .submit(AgentCommand::UserTurn("创建两个终端卡片".to_string()))
        .expect("同一语音 turn 的两个 card_task start 都应成功入队");
    let task_ids = queued
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TaskQueued { task_id, .. } => Some(*task_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        task_ids,
        vec![1, 2],
        "两个 start 必须各自产生递增且有序的 task id"
    );
    assert_eq!(
        system.pending_task_count(),
        2,
        "两个任务都应进入同一项目的串行队列"
    );
    assert!(
        system.card_tool_host().commands.is_empty(),
        "queued receipt 返回前不得提前创建任何终端"
    );

    let first_start = system.pump();
    assert!(
        first_start.contains(&AgentEvent::TaskRunning { task_id: 1 }),
        "首个 pump 只能启动队首任务 #1"
    );
    assert!(
        !first_start.iter().any(|event| matches!(
            event,
            AgentEvent::TaskRunning { task_id: 2 } | AgentEvent::ToolStarted { task_id: 2, .. }
        )),
        "首个 pump 不得同时启动或执行 task #2"
    );
    let first_create = system.pump();
    assert_eq!(
        system.card_tool_host().created,
        1,
        "task #1 执行时只能先创建第一个 Terminal"
    );
    assert!(
        !first_create
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskRunning { task_id: 2 })),
        "task #1 尚未终态时 task #2 不得并行进入 Running"
    );
    assert!(
        !system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::ToolStarted { task_id: 2, .. })),
        "task #1 执行期间 task #2 不得开始底层工具调用"
    );
    let first_finish = system.pump();
    assert!(
        first_finish
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskSucceeded { task_id: 1, .. })),
        "task #1 的终态必须先于 task #2 启动"
    );
    assert!(
        !system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskRunning { task_id: 2 } | AgentEvent::ToolStarted { task_id: 2, .. }
        )),
        "task #1 终态前的完整事件前缀不得包含 task #2 的运行或工具事件"
    );

    // 每个任务最多启动、执行一个工具、收尾三次 pump；首任务已消费三次，剩余上限为五次。
    for _ in 0..5 {
        if system.pending_task_count() == 0 {
            break;
        }
        let _ = system.pump();
    }

    assert_eq!(
        system.pending_task_count(),
        0,
        "有限 pump 后两个串行任务都必须完成，不能遗留 pending 或 active 状态"
    );

    assert_eq!(
        system.card_tool_host().created,
        2,
        "两个任务必须各自创建一个 Terminal"
    );
    assert!(
        system
            .card_tool_host()
            .commands
            .iter()
            .all(|command| matches!(command, CardCommand::Create { .. })),
        "这条流程只应向 Runtime bridge 发送两个 Create 命令"
    );
    let succeeded_ids = system
        .events()
        .iter()
        .filter_map(|event| match event {
            AgentEvent::TaskSucceeded { task_id, .. } => Some(*task_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        succeeded_ids,
        vec![1, 2],
        "第二任务必须在第一任务终态之后才成功，保持项目内串行执行"
    );
}
