//! L1 domain e2e：验证 Queued 委派回执生成 TTS 上游消费的用户文案。

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
            .ok_or_else(|| ProviderError::Message("queued wording provider exhausted".into()))
    }
}

struct QueuedReceiptHost;

impl CardToolHost for QueuedReceiptHost {
    fn execute(
        &mut self,
        request: CardToolRequest,
    ) -> Result<CardToolResult, agent::CardToolError> {
        let CardCommand::DispatchAgentTasks { assignments, .. } = request.command else {
            panic!("Queued 文案回归测试只应执行 DispatchAgentTasks");
        };
        let receipts = assignments
            .into_iter()
            .map(|assignment| AgentTaskReceipt {
                name: assignment.name,
                node_id: 64,
                task: assignment.task,
                status: AgentTaskReceiptStatus::Queued,
            })
            .collect();
        Ok(CardToolResult {
            result: CardCommandResult::AgentTasksDispatched { receipts },
        })
    }
}

fn text_response(text: &str) -> ProviderResponse {
    ProviderResponse {
        text: text.to_string(),
        tool_calls: Vec::new(),
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
        busy: true,
    }]
}

/// L1-QUEUED-WORDING-01：Queued 派发回执应使用“正在执行”的用户文案。
///
/// 数据构造（含关键数值的推导过程）：
///   addressed agents = 1：Bella
///   queued receipts  = 1：Bella 的派发回执状态为 Queued
///   terminal events  = 1：聚合为一个 TaskSucceeded
///   pump upper bound = 4：启动(1) + 派发(1) + 聚合(1) + 保护 tick(1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 提交“Bella 查看当前项目的功能”并进入确定性委派路径。
///   2. CardToolHost 返回 Bella/Queued 真实回执。
///   3. CardTask 聚合回执并生成 TTS 上游消费的 TaskSucceeded.summary。
///
/// 预期结果：
///   - 正断言：终态摘要必须是“Bella 正在执行”。
///   - 负断言：用户可见摘要不得继续出现“已排队”。
#[test]
fn queued_delegation_receipt_uses_running_user_message() {
    let dialogue = ScriptedProvider::new(vec![text_response("收到")]);
    let card = ScriptedProvider::new(Vec::new());
    let mut system = AgentSystem::new(
        "queued-delegation-wording",
        dialogue,
        card,
        QueuedReceiptHost,
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

    let summary = system.events().iter().find_map(|event| match event {
        AgentEvent::TaskSucceeded { summary, .. } => Some(summary.as_str()),
        _ => None,
    });
    assert_eq!(
        summary,
        Some("Bella 正在执行"),
        "Queued 派发回执必须生成用户指定的“Bella 正在执行”"
    );
    assert!(
        summary.is_some_and(|summary| !summary.contains("已排队")),
        "Queued 派发回执的用户文案不得继续出现“已排队”"
    );
}
