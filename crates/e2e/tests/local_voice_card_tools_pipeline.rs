//! Domain-level e2e：本地语音管线扁平工具链（Create → SendText）与模板播报决策。
//!
//! 不依赖 GPUI 或 app 私有 `#[cfg(test)]` helper，仅通过公开 crate API 验收跨层边界。

use card_task::CardCommandRunner;
use dialogue::{
    decide_template_report, run_voice_turn, DialogueError, DialoguePhase, ReportPlayDecision,
    ToolExecutor,
};
use llm::{MockLlmClient, MockStep, ToolCall};
use norma::protocol::{CardCommand, CardCommandError, CardCommandResult, CardKind, Placement};

/// 记录已执行命令的 `CardCommandRunner`，供 e2e 断言命令顺序。
struct RecordingCardRunner {
    recorded: Vec<CardCommand>,
    handler: Box<dyn FnMut(CardCommand) -> Result<CardCommandResult, CardCommandError> + Send>,
}

impl RecordingCardRunner {
    fn new(
        handler: impl FnMut(CardCommand) -> Result<CardCommandResult, CardCommandError> + Send + 'static,
    ) -> Self {
        Self {
            recorded: Vec::new(),
            handler: Box::new(handler),
        }
    }

    fn recorded_commands(&self) -> &[CardCommand] {
        &self.recorded
    }
}

impl CardCommandRunner for RecordingCardRunner {
    fn run(&mut self, command: CardCommand) -> Result<CardCommandResult, CardCommandError> {
        self.recorded.push(command.clone());
        (self.handler)(command)
    }
}

/// 将 `card_task::CardCommandRunner` 适配为 `dialogue::ToolExecutor`，与 app 层接线一致。
struct CardRunnerToolExecutor<'a> {
    runner: &'a mut RecordingCardRunner,
}

impl ToolExecutor for CardRunnerToolExecutor<'_> {
    fn execute(&mut self, command: CardCommand) -> Result<CardCommandResult, DialogueError> {
        self.runner
            .run(command)
            .map_err(|error| DialogueError::Message(error.to_string()))
    }
}

/// LVCP-1: Mock LLM Create→SendText 剧本应经 run_voice_turn 按序执行并返回最终回复。
///
/// 数据构造：
///   用户 transcript「创建终端并发送 ls」；
///   Mock 三步：Create 工具 → SendText(node_id:7) → 文本「好的，已经发到终端」；
///   RecordingCardRunner：Create→Created{7}，SendText→Sent{7}。
///
/// 执行过程：
///   1. 构造 RecordingCardRunner + CardRunnerToolExecutor
///   2. 调用 run_voice_turn，max_tool_rounds=3
///
/// 预期结果：
///   - 正断言：reply_text ==「好的，已经发到终端」
///   - 正断言：recorded_commands 顺序为 Create(Terminal) 再 SendText{7,"ls"}
///   - 正断言：command_results 为 Created{7} 与 Sent{7}
///   - 负断言：不应返回 Err
#[test]
fn test_local_voice_create_then_send_text_pipeline() {
    let mut runner = RecordingCardRunner::new(|command| match command {
        CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 7 }),
        CardCommand::SendText { node_id, .. } => Ok(CardCommandResult::Sent { node_id }),
        other => Err(CardCommandError::Unsupported {
            message: format!("RecordingCardRunner 未实现: {other:?}"),
        }),
    });
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
    let mut executor = CardRunnerToolExecutor {
        runner: &mut runner,
    };

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
        runner.recorded_commands(),
        &[
            CardCommand::Create {
                kind: CardKind::Terminal { shell: None },
                placement: Placement::VisibleArea,
            },
            CardCommand::SendText {
                node_id: 7,
                text: "ls".into(),
                submit: true,
            },
        ],
        "RecordingCardRunner 应按 Create→SendText 顺序记录命令"
    );
    assert_eq!(
        outcome.command_results,
        vec![
            CardCommandResult::Created { node_id: 7 },
            CardCommandResult::Sent { node_id: 7 },
        ],
        "命令结果应与 RecordingCardRunner 返回值一致"
    );
    assert_ne!(
        outcome.executed_commands.len(),
        0,
        "不应出现零条已执行命令的成功路径"
    );
}

/// LVCP-2: Speaking 阶段模板播报应排队（Queue），避免打断当前 TTS。
///
/// 数据构造：
///   phase=Speaking，user_speech_active=false。
///
/// 执行过程：
///   1. 调用 decide_template_report
///
/// 预期结果：
///   - 正断言：决策为 Queue
///   - 负断言：决策不是 PlayNow
#[test]
fn test_template_report_decision_speaking_queues() {
    let decision = decide_template_report(DialoguePhase::Speaking, false);

    assert_eq!(
        decision,
        ReportPlayDecision::Queue,
        "Speaking 且用户未说话时模板播报应排队"
    );
    assert_ne!(
        decision,
        ReportPlayDecision::PlayNow,
        "Speaking 时不应立即播放模板"
    );
}

/// LVCP-3: Listening 阶段模板播报应立即播放（PlayNow），任务完成可即时接回对话。
///
/// 数据构造：
///   phase=Listening，user_speech_active=false。
///
/// 执行过程：
///   1. 调用 decide_template_report
///
/// 预期结果：
///   - 正断言：决策为 PlayNow
///   - 负断言：决策不是 Queue
#[test]
fn test_template_report_decision_listening_plays_now() {
    let decision = decide_template_report(DialoguePhase::Listening, false);

    assert_eq!(
        decision,
        ReportPlayDecision::PlayNow,
        "Listening 且用户未说话时应立即播放模板播报"
    );
    assert_ne!(
        decision,
        ReportPlayDecision::Queue,
        "Listening 时不应排队等待"
    );
}
