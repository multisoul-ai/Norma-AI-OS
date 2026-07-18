//! L1 domain e2e：验证委派预检与逐 Agent 回执身份核对。

use agent::{
    AgentCommand, AgentEvent, AgentProvider, AgentRosterEntry, AgentSystem, AgentTrackingMode,
    CardTaskBudget, CardToolHost, CardToolRequest, CardToolResult, MemoryJournal, ProviderError,
    ProviderRequest, ProviderResponse,
};
use norma::protocol::{
    AgentProfile, AgentTaskReceipt, AgentTaskReceiptStatus, CardCommand, CardCommandResult,
};
use std::collections::VecDeque;

struct ScriptedProvider {
    responses: VecDeque<ProviderResponse>,
}

impl ScriptedProvider {
    fn dialogue_text(text: &str) -> Self {
        Self {
            responses: vec![ProviderResponse {
                text: text.to_string(),
                tool_calls: Vec::new(),
            }]
            .into(),
        }
    }

    fn never() -> Self {
        Self {
            responses: VecDeque::new(),
        }
    }
}

impl AgentProvider for ScriptedProvider {
    fn complete(&mut self, _: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.responses.pop_front().ok_or_else(|| {
            ProviderError::Message("delegation validation provider exhausted".to_string())
        })
    }
}

#[derive(Default)]
struct ReceiptHost {
    commands: Vec<CardCommand>,
    receipt_names: Option<Vec<String>>,
}

impl CardToolHost for ReceiptHost {
    fn execute(
        &mut self,
        request: CardToolRequest,
    ) -> Result<CardToolResult, agent::CardToolError> {
        self.commands.push(request.command.clone());
        let CardCommand::DispatchAgentTasks { assignments, .. } = request.command else {
            return Err(agent::CardToolError::Execution(
                "delegation validation only accepts dispatch_agent_tasks".to_string(),
            ));
        };
        let receipt_names = self
            .receipt_names
            .clone()
            .unwrap_or_else(|| assignments.iter().map(|item| item.name.clone()).collect());
        Ok(CardToolResult {
            result: CardCommandResult::AgentTasksDispatched {
                receipts: receipt_names
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| AgentTaskReceipt {
                        name,
                        node_id: 64 + index as u64,
                        task: assignments
                            .get(index)
                            .map(|assignment| assignment.task.clone())
                            .unwrap_or_else(|| "unexpected receipt".to_string()),
                        status: AgentTaskReceiptStatus::Submitted,
                    })
                    .collect(),
            },
        })
    }
}

fn roster_entry(definition_id: i64, name: &str, enabled: bool) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id,
        name: name.to_string(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled,
        active_node_id: Some(63 + definition_id as u64),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }
}

/// L1-DELEGATION-VALIDATION-01：完整数量的重复 Agent 回执不得冒充多 Agent 成功。
///
/// 数据构造（含关键数值的推导过程）：
///   expected assignments = 2：Bella + Alan
///   returned receipts    = 2：Bella + Bella
///   receipt count check  = 2 == 2，但唯一身份数 1 < 2
///   pump upper bound      = 4：启动(1) + 派发(1) + 核对(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 明确共同任务生成 Bella 与 Alan 两条 assignment
///   2. Host 故意返回两条同名 Bella Submitted 回执
///   3. CardTask 按 assignment 目标身份核对完整回执集合
///
/// 预期结果：
///   - 正断言：任务以缺失/重复目标回执的 TaskFailed 收口
///   - 正断言：Host 只收到一次包含 Bella、Alan 的原子派发
///   - 负断言：相同数量的重复回执不得产生 TaskSucceeded
#[test]
fn duplicate_agent_receipts_cannot_satisfy_multi_agent_completion() {
    let mut system = AgentSystem::new(
        "delegation-receipt-identity",
        ScriptedProvider::dialogue_text("收到"),
        ScriptedProvider::never(),
        ReceiptHost {
            commands: Vec::new(),
            receipt_names: Some(vec!["Bella".to_string(), "Bella".to_string()]),
        },
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(vec![
            roster_entry(1, "Bella", true),
            roster_entry(2, "Alan", true),
        ]))
        .expect("有效 Bella/Alan 名册必须可注入");

    system
        .submit(AgentCommand::UserTurn(
            "Bella 和 Alan 都检查当前项目".to_string(),
        ))
        .expect("明确多 Agent 委派必须进入 CardTask");
    // 快路径最多需要：启动(1) + 派发(1) + 回执核对(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
        let _ = system.pump();
    }

    assert!(
        system.events().iter().any(|event| matches!(
            event,
            AgentEvent::TaskFailed { reason, .. }
                if reason.contains("receipt") && reason.contains("Alan")
        )),
        "重复 Bella 回执必须明确报告缺失 Alan 回执"
    );
    assert_eq!(
        system.card_tool_host().commands.len(),
        1,
        "回执身份不匹配不得触发第二次派发"
    );
    assert!(
        !system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskSucceeded { .. })),
        "两条重复 Bella 回执不得冒充 Bella+Alan 派发成功"
    );
}

/// L1-DELEGATION-VALIDATION-02：禁用 Agent 必须在 CardTask 入队前被拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   roster agents        = 1：Bella
///   enabled agents       = 0
///   expected queued tasks = 0
///   expected host calls   = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 注入存在但 enabled=false 的 Bella
///   2. 提交明确“Bella 检查当前项目”指令
///   3. 在任何 TaskQueued 或 CardToolHost 调用前验证目标可用性
///
/// 预期结果：
///   - 正断言：UserTurn 返回包含 disabled 的明确错误
///   - 正断言：pending_task_count 保持 0
///   - 负断言：不得产生 TaskQueued 或任何 Host 副作用
#[test]
fn disabled_agent_is_rejected_before_card_task_queueing() {
    let mut system = AgentSystem::new(
        "delegation-disabled-preflight",
        ScriptedProvider::dialogue_text("收到"),
        ScriptedProvider::never(),
        ReceiptHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );
    system
        .submit(AgentCommand::UpdateAgentRoster(vec![roster_entry(
            1, "Bella", false,
        )]))
        .expect("禁用 Agent 仍可存在于项目名册");

    let result = system.submit(AgentCommand::UserTurn("Bella 检查当前项目".to_string()));

    assert!(
        result
            .as_ref()
            .is_err_and(|error| error.to_string().contains("disabled")),
        "禁用 Bella 必须在入队前返回明确 disabled 错误"
    );
    assert_eq!(
        system.pending_task_count(),
        0,
        "禁用目标不得创建任何待执行 CardTask"
    );
    assert!(
        !system
            .events()
            .iter()
            .any(|event| matches!(event, AgentEvent::TaskQueued { .. })),
        "禁用目标不得先发 TaskQueued 再失败"
    );
    assert!(
        system.card_tool_host().commands.is_empty(),
        "禁用目标不得到达 CardToolHost"
    );
}
