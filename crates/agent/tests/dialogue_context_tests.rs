mod support;

use agent::{
    AgentCommand, AgentProvider, AgentSystem, CardTaskBudget, MemoryJournal, MessageRole,
    ProviderError, ProviderRequest, ProviderResponse,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use support::{response, text_response, tool, RecordingHost, ScriptedProvider};

struct RecordingDialogueProvider {
    responses: VecDeque<ProviderResponse>,
    requests: Arc<Mutex<Vec<ProviderRequest>>>,
}

impl AgentProvider for RecordingDialogueProvider {
    fn complete(&mut self, request: &ProviderRequest) -> Result<ProviderResponse, ProviderError> {
        self.requests
            .lock()
            .expect("request recording lock should not be poisoned")
            .push(request.clone());
        self.responses
            .pop_front()
            .ok_or_else(|| ProviderError::Message("recording provider exhausted".to_string()))
    }
}

/// A-1c：Dialogue 的 card_task tool call 必须在共享上下文中闭合为 queued receipt。
///
/// 数据构造：首轮返回 call-queued 的 start；第二轮返回普通文本，并记录两次 ProviderRequest。
/// 执行过程：连续提交两个 UserTurn，检查第二次请求携带的历史消息。
/// 预期结果：assistant tool call 后存在同 call id 的 Tool 消息，内容含 queued 与 task_id=1。
#[test]
fn dialogue_task_start_appends_queued_tool_receipt_to_shared_context() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let dialogue = RecordingDialogueProvider {
        responses: VecDeque::from([
            response([tool(
                "call-queued",
                "card_task",
                r#"{"action":"start","intent":"创建终端"}"#,
            )]),
            text_response("任务仍在后台执行"),
        ]),
        requests: requests.clone(),
    };
    let mut system = AgentSystem::new(
        "project-a",
        dialogue,
        ScriptedProvider::default(),
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );

    system
        .submit(AgentCommand::UserTurn("创建终端".to_string()))
        .expect("首轮应派发后台任务");
    system
        .submit(AgentCommand::UserTurn("进度如何".to_string()))
        .expect("带闭合 tool receipt 的第二轮应成功");

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    assert_eq!(
        recorded.len(),
        2,
        "连续两个 UserTurn 应产生两次 Provider 请求"
    );
    let tool_receipt = recorded[1]
        .messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("第二轮上下文应包含 queued tool receipt");
    assert_eq!(
        tool_receipt.tool_call_id.as_deref(),
        Some("call-queued"),
        "Tool receipt 必须闭合原始 OpenAI tool call id"
    );
    assert!(
        tool_receipt.content.contains(r#""status":"queued""#),
        "Tool receipt 应明确报告 queued 状态"
    );
    assert!(
        tool_receipt.content.contains(r#""task_id":1"#),
        "Tool receipt 应把真实 task id 回灌 Dialogue 上下文"
    );
}
