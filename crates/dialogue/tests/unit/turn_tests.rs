use llm::{MockLlmClient, MockStep, ToolCall};
use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};

use crate::turn::{run_voice_turn, ToolExecutor};
use crate::DialogueError;

struct FakeCardExecutor;

impl ToolExecutor for FakeCardExecutor {
    fn execute(&mut self, command: CardCommand) -> Result<CardCommandResult, DialogueError> {
        match command {
            CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 7 }),
            CardCommand::SendText { node_id, .. } => Ok(CardCommandResult::Sent { node_id }),
            other => Err(DialogueError::Message(format!(
                "FakeCardExecutor 未实现的命令: {other:?}"
            ))),
        }
    }
}

/// TURN-1: 多轮工具调用后应返回最终回复，且命令按 Create → SendText 顺序执行。
///
/// 数据构造：
///   用户 transcript「创建终端并发送 ls」；
///   Mock 三步：Create 工具 → SendText(node_id:7) → 文本「好的，已经发到终端」；
///   FakeCardExecutor：Create→Created{7}，SendText→Sent{7}。
///
/// 执行过程：
///   1. 调用 run_voice_turn，max_tool_rounds=3
///
/// 预期结果：
///   - 正断言：reply_text ==「好的，已经发到终端」
///   - 正断言：executed_commands 顺序为 Create(Terminal) 再 SendText{7,"ls"}
///   - 正断言：command_results 为 Created{7} 与 Sent{7}
///   - 负断言：不应返回 Err
#[test]
fn test_run_voice_turn_create_then_send_text_returns_final_reply() {
    let mut llm = MockLlmClient::script(vec![
        MockStep::Tools(vec![ToolCall {
            id: "call_create".into(),
            name: "Create".into(),
            arguments_json: r#"{"kind":"terminal"}"#.into(),
        }]),
        MockStep::Tools(vec![ToolCall {
            id: "call_send".into(),
            name: "SendText".into(),
            arguments_json: r#"{"node_id":7,"text":"ls","submit":true}"#.into(),
        }]),
        MockStep::Text("好的，已经发到终端".into()),
    ]);
    let mut executor = FakeCardExecutor;

    let outcome = run_voice_turn(
        &mut llm,
        &mut executor,
        "创建终端并发送 ls",
        "你是 Norma 语音助手。",
        3,
    )
    .expect("Create→SendText→文本 三轮应成功完成");

    assert_eq!(
        outcome.reply_text, "好的，已经发到终端",
        "最终回复应来自 Mock 第三步文本"
    );
    assert_eq!(
        outcome.executed_commands.len(),
        2,
        "应执行 Create 与 SendText 两条命令"
    );
    assert_eq!(
        outcome.executed_commands[0],
        CardCommand::Create {
            kind: CardKind::Terminal { shell: None },
            placement: Placement::VisibleArea,
        },
        "第一条命令应为 Create Terminal"
    );
    assert_eq!(
        outcome.executed_commands[1],
        CardCommand::SendText {
            node_id: 7,
            text: "ls".into(),
            submit: true,
        },
        "第二条命令应为 SendText ls 到 node 7"
    );
    assert_eq!(
        outcome.command_results,
        vec![
            CardCommandResult::Created { node_id: 7 },
            CardCommandResult::Sent { node_id: 7 },
        ],
        "命令结果应与 FakeCardExecutor 返回值一致"
    );
}

/// TURN-2: 超过 max_tool_rounds 仍只有工具调用时应返回错误。
///
/// 数据构造：
///   Mock 两步均返回 Create 工具；max_tool_rounds=2。
///
/// 执行过程：
///   1. 调用 run_voice_turn
///
/// 预期结果：
///   - 负断言：返回 Err，提示超出 max_tool_rounds
///   - 负断言：不应返回 Ok 的最终文本
#[test]
fn test_run_voice_turn_errors_when_max_tool_rounds_exceeded() {
    let create_call = ToolCall {
        id: "call_create".into(),
        name: "Create".into(),
        arguments_json: r#"{"kind":"terminal"}"#.into(),
    };
    let mut llm = MockLlmClient::script(vec![
        MockStep::Tools(vec![create_call.clone()]),
        MockStep::Tools(vec![create_call]),
    ]);
    let mut executor = FakeCardExecutor;

    let err = run_voice_turn(&mut llm, &mut executor, "一直创建终端", "system", 2)
        .expect_err("两轮均为工具时应超出 max_tool_rounds");

    assert!(
        matches!(err, DialogueError::Message(ref msg) if msg.contains("max tool rounds")),
        "错误信息应说明超出 max tool rounds，实际: {err:?}"
    );
}
