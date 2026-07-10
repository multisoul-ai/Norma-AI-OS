use super::{ChatRequest, LlmClient, LlmError, MockLlmClient, MockStep, ToolCall};

/// MOCK-1: 第一次 chat 返回 tool_calls，第二次返回最终文本。
///
/// 数据构造：
///   script 两步：先 Tools(Create)，再 Text("已经创建终端")。
///   ChatRequest 使用空 messages/tools，Mock 只按剧本顺序响应。
///
/// 执行过程：
///   1. 第一次 chat → 应弹出 Tools 步，仅含 Create 工具调用
///   2. 第二次 chat → 应弹出 Text 步，返回最终回复
///
/// 预期结果：
///   - 正断言：第一次 tool_calls.len() == 1 且 assistant_text 为空
///   - 正断言：第二次 assistant_text == "已经创建终端" 且 tool_calls 为空
///   - 负断言：工具步不应携带用户可见文本；文本步不应额外发明工具
#[test]
fn test_mock_llm_returns_tools_then_text_across_two_chats() {
    let mut client = MockLlmClient::script(vec![
        MockStep::Tools(vec![ToolCall {
            id: "call_1".into(),
            name: "Create".into(),
            arguments_json: r#"{"kind":"terminal"}"#.into(),
        }]),
        MockStep::Text("已经创建终端".into()),
    ]);
    let req = ChatRequest {
        messages: vec![],
        tools: vec![],
    };
    let first = client.chat(&req).expect("first scripted step must succeed");
    assert_eq!(
        first.tool_calls.len(),
        1,
        "first step must expose exactly one Create tool call"
    );
    assert!(
        first.assistant_text.is_empty(),
        "tool-only step should not carry user-facing text"
    );
    let second = client
        .chat(&req)
        .expect("second scripted step must succeed");
    assert_eq!(
        second.assistant_text, "已经创建终端",
        "second step must return final reply text"
    );
    assert!(
        second.tool_calls.is_empty(),
        "final text step must not invent extra tools"
    );
}

/// MOCK-2: 剧本用完后 chat 应返回 mock exhausted 错误。
///
/// 数据构造：
///   MockLlmClient::script(vec![]) — 空剧本，无任何可弹出步骤。
///
/// 执行过程：
///   1. 对空剧本 client 调用 chat
///
/// 预期结果：
///   - 负断言：返回 Err(LlmError::Message("mock exhausted"))
///   - 负断言：不应返回 Ok 响应
#[test]
fn test_mock_llm_returns_exhausted_when_script_depleted() {
    let mut client = MockLlmClient::script(vec![]);
    let req = ChatRequest {
        messages: vec![],
        tools: vec![],
    };
    let err = client
        .chat(&req)
        .expect_err("empty script must not return Ok");
    assert_eq!(
        err,
        LlmError::Message("mock exhausted".into()),
        "depleted mock must surface mock exhausted error"
    );
}
