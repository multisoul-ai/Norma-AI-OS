pub mod support;

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

/// A-P-1：每个 Dialogue Provider 请求都必须以组合后的 system prompt 开始。
///
/// 数据构造：记录型 Dialogue Provider 返回一段普通文本；CardTask Provider 为空，因为普通聊天不派发任务。
/// 执行过程：提交一个 UserTurn，再从记录的公开 ProviderRequest 读取首条消息。
/// 预期结果：首条消息为 System 且内容等于公开组合 API；用户文本仍作为后续消息保留，不能被 prompt 覆盖。
#[test]
fn dialogue_request_starts_with_the_composed_system_prompt() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let dialogue = RecordingDialogueProvider {
        responses: VecDeque::from([text_response("你好，我在呢")]),
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
        .submit(AgentCommand::UserTurn("你好".to_string()))
        .expect("普通 Dialogue 请求应成功发送给 Provider");

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    let request = recorded
        .first()
        .expect("提交一个 UserTurn 应记录一次 Provider 请求");
    let first_message = request
        .messages
        .first()
        .expect("Dialogue Provider 请求必须包含 system message");
    assert_eq!(
        first_message.role,
        MessageRole::System,
        "Dialogue Provider 请求的首条消息必须是 system prompt"
    );
    assert_eq!(
        first_message.content,
        agent::dialogue_system_prompt(),
        "Dialogue Provider 请求必须注入公开组合 API 生成的完整 system prompt"
    );
    assert!(
        request
            .messages
            .iter()
            .any(|message| { message.role == MessageRole::User && message.content == "你好" }),
        "system prompt 注入后仍必须保留当前用户文本"
    );
}

/// A-P-2：CardTask Provider 请求必须注入独立执行 prompt，不能携带 Dialogue persona。
///
/// 数据构造：直接提交一个 CardTask，记录型 Card Provider 立即返回无工具调用的文本；Dialogue Provider 不参与。
/// 执行过程：调用一次 `pump` 启动任务并记录 Card ProviderRequest，读取其首条消息。
/// 预期结果：首条消息等于公开 CardTask prompt；当前任务意图仍存在；请求中没有 Dialogue persona 的助理人设标记。
#[test]
fn card_task_request_starts_with_the_isolated_execution_prompt() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let card = RecordingDialogueProvider {
        responses: VecDeque::from([text_response("没有需要执行的操作")]),
        requests: requests.clone(),
    };
    let mut system = AgentSystem::new(
        "project-a",
        ScriptedProvider::default(),
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );

    system
        .submit(AgentCommand::DirectCardTask("检查当前卡片".to_string()))
        .expect("直接 CardTask 应成功入队");
    system.pump();

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    let request = recorded
        .first()
        .expect("启动 CardTask 应记录一次 Card Provider 请求");
    let first_message = request
        .messages
        .first()
        .expect("CardTask Provider 请求必须包含 system message");
    assert_eq!(
        first_message.role,
        MessageRole::System,
        "CardTask Provider 请求的首条消息必须是独立 system prompt"
    );
    assert_eq!(
        first_message.content,
        agent::card_task_system_prompt(),
        "CardTask Provider 请求必须注入公开 API 提供的执行 prompt"
    );
    assert!(
        request.messages.iter().any(|message| {
            message.role == MessageRole::User && message.content == "检查当前卡片"
        }),
        "CardTask system prompt 注入后仍必须保留任务 intent"
    );
    assert!(
        !request
            .messages
            .iter()
            .any(|message| message.content.contains("Norma 多人工智能助理")),
        "CardTask Provider 请求不得泄漏 Dialogue persona"
    );
}

/// A-P-3：Dialogue 历史达到上限时只保留最近轮次，但 system prompt 永远保留。
///
/// 数据构造：发送 `MAX_DIALOGUE_HISTORY_TURNS + 2` 个普通 turn；每轮回复为可区分的 `reply-N`。
/// 执行过程：记录最后一个 ProviderRequest，检查它保留的用户轮次、最早轮次和首条 system message。
/// 预期结果：最后请求含当前轮与最近历史、不含最早 `turn-1`；用户轮次数不超过上限；组合 system prompt 始终在首位。
#[test]
fn dialogue_history_discards_old_turns_without_dropping_system_prompt() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let turn_count = agent::MAX_DIALOGUE_HISTORY_TURNS + 2;
    let dialogue = RecordingDialogueProvider {
        responses: (1..=turn_count)
            .map(|index| text_response(&format!("reply-{index}")))
            .collect(),
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

    for index in 1..=turn_count {
        system
            .submit(AgentCommand::UserTurn(format!("turn-{index}")))
            .expect("每个普通 Dialogue turn 都应能发送给 Provider");
    }

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    let request = recorded
        .last()
        .expect("最后一个 turn 应产生可检查的 Provider 请求");
    let first_message = request
        .messages
        .first()
        .expect("裁剪后的请求仍必须包含 system message");
    assert_eq!(
        first_message.role,
        MessageRole::System,
        "裁剪后的 Dialogue 请求必须仍以 system prompt 开始"
    );
    assert_eq!(
        first_message.content,
        agent::dialogue_system_prompt(),
        "裁剪不得替换或删除完整的 Dialogue system prompt"
    );
    assert!(
        request.messages.iter().any(|message| {
            message.role == MessageRole::User && message.content == format!("turn-{turn_count}")
        }),
        "最后请求必须保留当前用户 turn"
    );
    assert!(
        request.messages.iter().any(|message| {
            message.role == MessageRole::User
                && message.content == format!("turn-{}", turn_count - 1)
        }),
        "最后请求必须保留最近的已完成用户 turn"
    );
    assert!(
        !request
            .messages
            .iter()
            .any(|message| message.content == "turn-1" || message.content == "reply-1"),
        "历史超过上限后最早的完整 turn 不得继续出现在 Provider 请求中"
    );
    let user_turn_count = request
        .messages
        .iter()
        .filter(|message| message.role == MessageRole::User)
        .count();
    assert_eq!(
        user_turn_count,
        agent::MAX_DIALOGUE_HISTORY_TURNS,
        "请求只应保留上限数量的最近用户 turn"
    );
}

/// A-1c：明确创建 fallback 生成的 card_task tool call 必须在共享上下文中闭合为 queued receipt。
///
/// 数据构造：首轮 Provider 只追问确认且不调用工具；后续进度查询返回普通文本，并记录两次 ProviderRequest。
/// 执行过程：先提交明确创建指令触发 fallback，再提交进度查询，检查查询请求携带的合成历史消息。
/// 预期结果：assistant tool call 后存在同 call id 的 Tool 消息，内容含 queued 与 task_id=1。
#[test]
fn dialogue_task_start_appends_queued_tool_receipt_to_shared_context() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let dialogue = RecordingDialogueProvider {
        responses: VecDeque::from([
            text_response("请确认需要哪一种终端"),
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
        "明确创建应先经过 Dialogue Provider，漏调用时再由策略 fallback"
    );
    let tool_receipt = recorded[1]
        .messages
        .iter()
        .find(|message| message.role == MessageRole::Tool)
        .expect("第二轮上下文应包含 queued tool receipt");
    assert_eq!(
        tool_receipt.tool_call_id.as_deref(),
        Some("dialogue-policy-card-task-1"),
        "Tool receipt 必须闭合确定性策略生成的稳定 tool call id"
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

/// A-1d：未知 Dialogue 工具调用也必须在共享上下文中有对应回执。
///
/// 数据构造：首轮返回一个未注册的 `unsupported_action` tool call；第二轮返回普通文本，并记录两次公开 ProviderRequest。
/// 执行过程：连续提交两个 UserTurn，读取第二次请求中首轮 assistant tool call 及其 Tool 消息。
/// 预期结果：未知调用不派发任务，但其 call id 必须有 `unsupported_tool` 回执，避免向 Provider 发送未闭合工具调用历史。
#[test]
fn dialogue_unknown_tool_call_appends_an_ignored_receipt_to_shared_context() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let dialogue = RecordingDialogueProvider {
        responses: VecDeque::from([
            response([tool("unknown-call", "unsupported_action", r#"{"value":1}"#)]),
            text_response("我只会调用已声明的工具"),
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
        .submit(AgentCommand::UserTurn("帮我处理一下".to_string()))
        .expect("未知工具调用仍应以可恢复的回执结束当前 Dialogue turn");
    system
        .submit(AgentCommand::UserTurn("继续说".to_string()))
        .expect("含未知工具回执的后续 Dialogue 请求应成功");

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    let request = recorded
        .get(1)
        .expect("连续两个 UserTurn 应记录第二次 Provider 请求");
    assert!(
        request.messages.iter().any(|message| {
            message.role == MessageRole::Assistant
                && message
                    .tool_calls
                    .iter()
                    .any(|call| call.id == "unknown-call" && call.name == "unsupported_action")
        }),
        "历史必须保留原始 assistant unknown tool call，以便回执有明确归属"
    );
    assert!(
        request.messages.iter().any(|message| {
            message.role == MessageRole::Tool
                && message.tool_call_id.as_deref() == Some("unknown-call")
                && message.content.contains("unsupported_tool")
                && message.content.contains("unsupported_action")
        }),
        "每个未知 tool call 必须有相同 call id 的 ignored Tool 回执，保持消息协议闭合"
    );
}

/// A-1e：Dialogue 的取消工具回执必须反映任务真实状态，而非统一声称“请求中”。
///
/// 数据构造：通过 DirectCardTask 排队 task #1 并在启动前取消，再尝试取消不存在的 #999；随后直接排队 task #2、启动其 Subagent，再取消运行中的任务。
/// 执行过程：在每次取消后的下一次公开 ProviderRequest 中查找相同 tool call id 的 Tool 回执。
/// 预期结果：排队任务记录 `cancelled`，未知任务记录 `error`，运行中任务记录 `cancel_requested`；三者不得相互混淆。
#[test]
fn dialogue_cancel_receipts_reflect_queued_active_and_unknown_task_states() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let dialogue = RecordingDialogueProvider {
        responses: VecDeque::from([
            response([tool(
                "cancel-queued",
                "card_task",
                r#"{"action":"cancel","task_id":1}"#,
            )]),
            response([tool(
                "cancel-unknown",
                "card_task",
                r#"{"action":"cancel","task_id":999}"#,
            )]),
            response([tool(
                "cancel-active",
                "card_task",
                r#"{"action":"cancel","task_id":2}"#,
            )]),
            text_response("已说明取消状态"),
        ]),
        requests: requests.clone(),
    };
    let card = ScriptedProvider::new([response([tool(
        "card-create",
        "create_card",
        r#"{"kind":"terminal"}"#,
    )])]);
    let mut system = AgentSystem::new(
        "project-a",
        dialogue,
        card,
        RecordingHost::default(),
        MemoryJournal::default(),
        CardTaskBudget::default(),
    );

    system
        .submit(AgentCommand::DirectCardTask("先创建一个终端".to_string()))
        .expect("首轮应创建排队任务");
    system
        .submit(AgentCommand::UserTurn("取消第一个任务".to_string()))
        .expect("取消排队任务应返回正常 Dialogue 回执");
    system
        .submit(AgentCommand::UserTurn("取消不存在的任务".to_string()))
        .expect("未知任务必须以 Tool 错误回执而非中断 Dialogue turn 收口");
    system
        .submit(AgentCommand::DirectCardTask("再创建一个终端".to_string()))
        .expect("第二个任务应成功入队");
    let _ = system.pump();
    system
        .submit(AgentCommand::UserTurn("取消第二个任务".to_string()))
        .expect("运行中任务应返回取消请求回执");
    system
        .submit(AgentCommand::UserTurn("确认状态".to_string()))
        .expect("后续请求必须保留运行中任务的取消回执");

    let recorded = requests
        .lock()
        .expect("request recording lock should not be poisoned");
    let queued_cancel_follow_up = recorded
        .get(1)
        .expect("第二次 Provider 请求必须保留排队任务的取消回执");
    let unknown_cancel_follow_up = recorded
        .get(2)
        .expect("第三次 Provider 请求必须保留未知任务的错误回执");
    let active_cancel_follow_up = recorded
        .get(3)
        .expect("第四次 Provider 请求必须保留运行中任务的取消请求回执");
    assert!(
        request_contains_tool_receipt(queued_cancel_follow_up, "cancel-queued", "cancelled"),
        "已从 pending 队列移除的任务必须回执为 cancelled"
    );
    assert!(
        request_contains_tool_receipt(unknown_cancel_follow_up, "cancel-unknown", "error"),
        "不存在的任务必须记录可解释的 error 回执"
    );
    assert!(
        request_contains_tool_receipt(active_cancel_follow_up, "cancel-active", "cancel_requested"),
        "仍在运行的任务必须回执为 cancel_requested，而非误报已取消"
    );
}

fn request_contains_tool_receipt(request: &ProviderRequest, call_id: &str, status: &str) -> bool {
    request.messages.iter().any(|message| {
        message.role == MessageRole::Tool
            && message.tool_call_id.as_deref() == Some(call_id)
            && message.content.contains(&format!(r#""status":"{status}""#))
    })
}
