//! L1 domain e2e：从 Dialogue card_task 到 Card Subagent，再到可审计卡片命令。
//!
//! 该文件验证 AgentSystem 的公开 seam，不声称启动真实 `.app`；真实 GPUI/L3 流程仍需
//! 在真实麦克风、Provider 和 TTS 环境中单独运行。

use agent::{
    AgentCommand, AgentEvent, AgentProvider, AgentSystem, CardTaskBudget, CardToolHost,
    CardToolRequest, CardToolResult, MemoryJournal, ProviderError, ProviderRequest,
    ProviderResponse, ProviderToolCall,
};
use norma::protocol::{CardCommand, CardCommandResult};
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
/// 数据构造：Dialogue 只产生一个 card_task start；Subagent 产生 create agent 与 send_text。
/// 执行过程：submit UserTurn 后 pump 直到无新事件，再观察 host 与 MemoryJournal。
/// 预期结果：task 成功、runtime-like host 的节点数增加、提交标记为 true、审计 task id 一致。
#[test]
fn voice_intent_reaches_runtime_like_host_and_journal() {
    let dialogue = ScriptedProvider::new(vec![tools(vec![call(
        "dialogue-1",
        "card_task",
        r#"{"action":"start","intent":"创建 Codex 并发送分析 README"}"#,
    )])]);
    let card = ScriptedProvider::new(vec![
        tools(vec![call(
            "card-1",
            "create_card",
            r#"{"kind":"agent","profile":"codex"}"#,
        )]),
        tools(vec![call(
            "card-2",
            "send_text",
            r#"{"node_id":1,"text":"分析 README","submit":true}"#,
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
        .expect("Dialogue 应成功产生 queued receipt");
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
        CardCommand::SendText {
            node_id: 1,
            text: "分析 README".to_string(),
            submit: true,
        },
        "第二个卡片命令应指向新建节点并 submit=true"
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
