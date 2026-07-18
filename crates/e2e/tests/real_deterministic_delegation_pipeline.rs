//! L1 real-provider e2e：验证真实 Dialogue Provider 到确定性 Agent 派发回执。

use agent::{
    AgentCommand, AgentEvent, AgentProvider, AgentRosterEntry, AgentSystem, AgentTrackingMode,
    CardTaskBudget, CardToolHost, CardToolRequest, CardToolResult, MemoryJournal,
    OpenAiCompatibleProvider, ProviderConfig, ProviderError, ProviderProtocol, ProviderRequest,
    ProviderResponse,
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
///   2. 向完整 AgentSystem 提交“Bella 查看当前项目的功能”
///   3. 无论模型选择文字还是 card_task，确定性路由都形成 Bella assignment
///   4. CardTask 绕过第二次模型规划并由记录型 Host 返回 Submitted 回执
///
/// 预期结果：
///   - 正断言：真实服务请求成功并只产生一个 Bella 派发命令
///   - 正断言：真实 Submitted 回执让任务进入 TaskSucceeded
///   - 负断言：NeverCardProvider 不得被调用，否则任务会失败
#[test]
#[ignore = "requires an explicitly configured real OpenAI-compatible Provider"]
fn real_dialogue_provider_reaches_deterministic_agent_dispatch_end_to_end() {
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
            "Bella 查看当前项目的功能".to_string(),
        ))
        .expect("真实 Dialogue Provider 必须成功处理明确 Bella 委派");
    // 快路径最多需要：启动(1) + 派发(1) + 聚合(1) + 保护 tick(1) = 4。
    for _ in 0..4 {
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

fn required_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("运行真实 Provider e2e 前必须设置 {name}"))
}
