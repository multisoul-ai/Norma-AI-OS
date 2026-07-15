use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};
use serde_json::Value;

use super::{
    card_tool_specs, parse_card_tool_call, tool_result_json_for_command_result, LlmError, ToolCall,
};

/// CARD-TOOL-1: Create terminal 工具调用应解析为 CardCommand::Create(Terminal)。
///
/// 数据构造：
///   ToolCall{name="Create", arguments_json={"kind":"terminal"}}。
///
/// 执行过程：
///   1. 调用 parse_card_tool_call
///
/// 预期结果：
///   - 正断言：得到 Create + Terminal{shell:None} + VisibleArea
///   - 负断言：不应解析为 Agent 或其它命令
#[test]
fn test_parse_create_terminal_tool_call() {
    let call = ToolCall {
        id: "call_1".into(),
        name: "Create".into(),
        arguments_json: r#"{"kind":"terminal"}"#.into(),
    };
    let command = parse_card_tool_call(&call).expect("terminal Create 应解析成功");
    assert_eq!(
        command,
        CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        },
        "terminal Create 应映射为 Terminal + VisibleArea"
    );
}

/// CARD-TOOL-2: 通用 Create 工具不得创建匿名 Agent 卡片。
///
/// 数据构造：
///   两条 ToolCall：显式 profile=codex；仅 kind=agent 无 profile。
///
/// 执行过程：
///   1. 分别 parse 两条 Create agent 调用
///
/// 预期结果：
///   - 正断言：两条调用都返回 Agent definition workflow 错误
///   - 负断言：任一调用都不应产生匿名 CardCommand::Create
#[test]
fn test_parse_create_agent_tool_call_rejects_generic_bypass() {
    let explicit = ToolCall {
        id: "call_2".into(),
        name: "Create".into(),
        arguments_json: r#"{"kind":"agent","profile":"codex"}"#.into(),
    };
    let defaulted = ToolCall {
        id: "call_3".into(),
        name: "Create".into(),
        arguments_json: r#"{"kind":"agent"}"#.into(),
    };
    let explicit_error = parse_card_tool_call(&explicit)
        .expect_err("显式 profile 也不得绕过 Agent definition workflow");
    let defaulted_error =
        parse_card_tool_call(&defaulted).expect_err("缺省 profile 也不得创建匿名 Agent 卡片");
    assert!(
        explicit_error
            .to_string()
            .contains("Agent definition workflow"),
        "显式 profile 的拒绝错误应指向 Agent definition workflow"
    );
    assert!(
        defaulted_error
            .to_string()
            .contains("Agent definition workflow"),
        "缺省 profile 的拒绝错误应指向 Agent definition workflow"
    );
    assert_ne!(
        explicit_error.to_string(),
        String::new(),
        "显式匿名 Agent 创建不得被静默接受"
    );
}

/// CARD-TOOL-3: Focus 工具调用应解析 node_id。
///
/// 数据构造：
///   ToolCall{name="Focus", arguments_json={"node_id":12}}。
///
/// 执行过程：
///   1. 调用 parse_card_tool_call
///
/// 预期结果：
///   - 正断言：得到 CardCommand::Focus { node_id: 12 }
///   - 负断言：不应解析为 Create 或 SendText
#[test]
fn test_parse_focus_tool_call() {
    let call = ToolCall {
        id: "call_4".into(),
        name: "Focus".into(),
        arguments_json: r#"{"node_id":12}"#.into(),
    };
    let command = parse_card_tool_call(&call).expect("Focus 应解析成功");
    assert_eq!(
        command,
        CardCommand::Focus { node_id: 12 },
        "Focus 应映射 node_id"
    );
}

/// CARD-TOOL-4: SendText 工具调用应解析 node_id、text，submit 缺省为 true。
///
/// 数据构造：
///   两条 ToolCall：显式 submit=true；省略 submit 字段。
///
/// 执行过程：
///   1. 分别 parse 两条 SendText 调用
///
/// 预期结果：
///   - 正断言：显式 submit=true 保留
///   - 正断言：省略 submit 时默认为 true
///   - 负断言：不应解析为 Focus
#[test]
fn test_parse_send_text_tool_call_submit_defaults_true() {
    let explicit = ToolCall {
        id: "call_5".into(),
        name: "SendText".into(),
        arguments_json: r#"{"node_id":12,"text":"ls","submit":true}"#.into(),
    };
    let defaulted = ToolCall {
        id: "call_6".into(),
        name: "SendText".into(),
        arguments_json: r#"{"node_id":12,"text":"pwd"}"#.into(),
    };
    let explicit_cmd = parse_card_tool_call(&explicit).expect("显式 submit SendText 应解析成功");
    let defaulted_cmd = parse_card_tool_call(&defaulted).expect("缺省 submit SendText 应解析成功");
    assert_eq!(
        explicit_cmd,
        CardCommand::SendText {
            node_id: 12,
            text: "ls".into(),
            submit: true,
        },
        "显式 submit=true 应保留"
    );
    assert_eq!(
        defaulted_cmd,
        CardCommand::SendText {
            node_id: 12,
            text: "pwd".into(),
            submit: true,
        },
        "省略 submit 时应默认为 true"
    );
}

/// CARD-TOOL-5: 未知工具名应拒绝解析。
///
/// 数据构造：
///   ToolCall{name="Delete", arguments_json={}}。
///
/// 执行过程：
///   1. 调用 parse_card_tool_call
///
/// 预期结果：
///   - 负断言：返回 Err，不应返回 Ok
///   - 负断言：错误信息应提及未知工具
#[test]
fn test_parse_rejects_unknown_tool_name() {
    let call = ToolCall {
        id: "call_7".into(),
        name: "Delete".into(),
        arguments_json: "{}".into(),
    };
    let err = parse_card_tool_call(&call).expect_err("未知工具名应拒绝");
    assert!(
        matches!(err, LlmError::Message(ref msg) if msg.contains("unknown tool")),
        "未知工具错误应说明 unknown tool，实际: {err:?}"
    );
}

/// CARD-TOOL-6: SendText 缺少 node_id 应拒绝解析。
///
/// 数据构造：
///   ToolCall{name="SendText", arguments_json={"text":"ls"}}，无 node_id。
///
/// 执行过程：
///   1. 调用 parse_card_tool_call
///
/// 预期结果：
///   - 负断言：返回 Err，不应返回 Ok
///   - 负断言：错误信息应提及 node_id
#[test]
fn test_parse_send_text_rejects_missing_node_id() {
    let call = ToolCall {
        id: "call_8".into(),
        name: "SendText".into(),
        arguments_json: r#"{"text":"ls"}"#.into(),
    };
    let err = parse_card_tool_call(&call).expect_err("缺少 node_id 的 SendText 应拒绝");
    assert!(
        matches!(err, LlmError::Message(ref msg) if msg.contains("node_id")),
        "缺少 node_id 错误应提及 node_id，实际: {err:?}"
    );
}

/// CARD-TOOL-7: Create 成功结果 JSON 必须包含 node_id 供下一轮 LLM 读取。
///
/// 数据构造：
///   CardCommandResult::Created { node_id: 7 }。
///
/// 执行过程：
///   1. 调用 tool_result_json_for_command_result
///   2. 将输出解析为 JSON
///
/// 预期结果：
///   - 正断言：JSON 对象含 node_id == 7
///   - 负断言：不应缺少 node_id 字段
#[test]
fn test_tool_result_json_for_created_includes_node_id() {
    let json = tool_result_json_for_command_result(&CardCommandResult::Created { node_id: 7 });
    let value: Value = serde_json::from_str(&json).expect("Create 结果 JSON 必须是合法 JSON");
    assert_eq!(
        value.get("node_id").and_then(Value::as_u64),
        Some(7),
        "Created 结果 JSON 必须包含 node_id=7"
    );
}

/// CARD-TOOL-8: card_tool_specs 应暴露 Create/Focus/SendText 三个工具定义。
///
/// 数据构造：
///   无额外输入，直接调用 card_tool_specs()。
///
/// 执行过程：
///   1. 收集工具名列表
///
/// 预期结果：
///   - 正断言：恰好 3 个工具
///   - 正断言：名称集合为 Create、Focus、SendText
///   - 负断言：不应包含 Delete 等 MVP 外工具
#[test]
fn test_card_tool_specs_exposes_three_mvp_tools() {
    let specs = card_tool_specs();
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();
    assert_eq!(names.len(), 3, "MVP 应恰好暴露 3 个卡片工具");
    assert!(names.contains(&"Create"), "应包含 Create 工具定义");
    assert!(names.contains(&"Focus"), "应包含 Focus 工具定义");
    assert!(names.contains(&"SendText"), "应包含 SendText 工具定义");
    assert!(!names.contains(&"Delete"), "MVP 不应暴露 Delete 工具");
}
