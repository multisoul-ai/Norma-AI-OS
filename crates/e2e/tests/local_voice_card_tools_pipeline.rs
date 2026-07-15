//! Domain-level e2e：本地语音管线扁平工具链（Create → SendText）与模板播报决策。
//!
//! 不依赖 GPUI 或 app 私有 `#[cfg(test)]` helper，仅通过公开 crate API 验收跨层边界。

use card_task::CardCommandRunner;
use dialogue::{
    apply_asr_final, decide_template_report, handle_speaking_speech_start, run_voice_turn,
    DialogueError, DialoguePhase, DialogueSession, ReportPlayDecision, SpeakingSpeechSource,
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

/// LVCP-4: assistant 播放期间的未分类麦克风事件不得截断回复，确认用户事件仍可打断。
///
/// 数据构造：
///   两个 session 均进入 Speaking；分别注入 UnclassifiedMicrophone 与 ConfirmedUser。
///
/// 执行过程：
///   1. 对 assistant 回声候选执行 speaking speech policy；
///   2. 对已确认用户插话执行同一 policy。
///
/// 预期结果：
///   - 正断言：未分类事件保持 Speaking，完整 TTS 不被截断；
///   - 正断言：已确认用户插话回到 Listening；
///   - 负断言：两类来源不得继续被无条件当成 barge-in。
#[test]
fn test_speaking_input_policy_distinguishes_echo_from_confirmed_user() {
    fn speaking_session() -> DialogueSession {
        let mut session = DialogueSession::new();
        session.on_listening_started();
        session.on_utterance_final().expect("应进入 Thinking");
        session.on_reply_ready().expect("应进入 Speaking");
        session
    }

    let mut echo_session = speaking_session();
    let echo_interrupted = handle_speaking_speech_start(
        &mut echo_session,
        SpeakingSpeechSource::UnclassifiedMicrophone,
    );
    let mut user_session = speaking_session();
    let user_interrupted =
        handle_speaking_speech_start(&mut user_session, SpeakingSpeechSource::ConfirmedUser);

    assert!(!echo_interrupted, "assistant 回声候选不应触发 barge-in");
    assert_eq!(
        echo_session.phase(),
        DialoguePhase::Speaking,
        "assistant 回声候选后应继续完整播放"
    );
    assert!(user_interrupted, "已确认用户插话应触发 barge-in");
    assert_eq!(
        user_session.phase(),
        DialoguePhase::Listening,
        "已确认用户插话后应恢复 Listening"
    );
}

/// LVCP-5：持续聆听应完成 ASR→LLM/tool→Speaking→Listening，并接受下一句。
///
/// 数据构造（含关键数值的推导过程）：
///   utterances = 2（第一句执行工具，第二句验证会话可继续）
///   first-turn LLM steps = 2（Create tool + 最终文本）
///   executed commands = 1（Create Terminal）
///   assistant echo events = 1（未分类麦克风输入，不得打断）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. DialogueSession 从 Idle 启动到 Listening
///   2. apply_asr_final(第一句) 保存 transcript 并推进 Thinking
///   3. run_voice_turn 执行 Create，拿到 node_id=11，再返回最终回复
///   4. on_reply_ready 推进 Speaking；注入 assistant 回声候选并继续播放
///   5. 播放完成恢复 Listening；apply_asr_final(第二句) 再次推进 Thinking
///
/// 预期结果：
///   - 正断言：第一轮工具结果、回复文本和每个 phase 均精确匹配
///   - 正断言：播放结束后模型保持驻留，第二句可继续进入 Thinking
///   - 负断言：assistant 回声不得截断 Speaking 或清除会话
///   - 负断言：完整一轮后不得错误回到 Idle
#[test]
fn test_continuous_voice_round_trip_returns_to_listening_and_accepts_next_utterance() {
    let mut session = DialogueSession::new();
    session.on_listening_started();
    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "持续会话启动后应进入 Listening"
    );

    apply_asr_final(&mut session, "创建一个终端").expect("第一句 ASR final 应被 Listening 接受");
    assert_eq!(
        session.phase(),
        DialoguePhase::Thinking,
        "第一句 final 后应进入 Thinking"
    );
    assert_eq!(
        session.pending_transcript(),
        Some("创建一个终端"),
        "Thinking 前应保存第一句 transcript"
    );

    let mut runner = RecordingCardRunner::new(|command| match command {
        CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 11 }),
        other => Err(CardCommandError::Unsupported {
            message: format!("连续对话矩阵未实现命令: {other:?}"),
        }),
    });
    let mut executor = CardRunnerToolExecutor {
        runner: &mut runner,
    };
    let mut llm = MockLlmClient::script(vec![
        MockStep::Tools(vec![ToolCall {
            id: "call_create_round_trip".into(),
            name: "Create".into(),
            arguments_json: r#"{"kind":"terminal"}"#.into(),
        }]),
        MockStep::Text("终端已经创建".into()),
    ]);
    let outcome = run_voice_turn(
        &mut llm,
        &mut executor,
        session
            .pending_transcript()
            .expect("第一句 pending transcript 应存在"),
        "你是 Norma 语音助手。",
        2,
    )
    .expect("第一轮 LLM/tool 应成功完成");
    assert_eq!(
        outcome.reply_text, "终端已经创建",
        "第一轮最终回复应来自 Mock 文本步骤"
    );
    assert_eq!(
        outcome.command_results,
        vec![CardCommandResult::Created { node_id: 11 }],
        "第一轮应返回精确的 Terminal node_id"
    );
    assert_eq!(
        runner.recorded_commands().len(),
        1,
        "第一轮应且只应执行一个 Create 命令"
    );

    session
        .on_reply_ready()
        .expect("LLM 回复就绪后应进入 Speaking");
    let echo_interrupted =
        handle_speaking_speech_start(&mut session, SpeakingSpeechSource::UnclassifiedMicrophone);
    assert!(!echo_interrupted, "assistant 回声候选不得打断 Speaking");
    assert_eq!(
        session.phase(),
        DialoguePhase::Speaking,
        "回声候选后应继续 Speaking"
    );

    session.on_playback_finished_or_interrupted(true);
    assert_eq!(
        session.phase(),
        DialoguePhase::Listening,
        "播放完成后持续会话应恢复 Listening"
    );
    assert!(
        session.models_resident(),
        "完成一轮后识别模型应保持进程内驻留"
    );
    assert_ne!(
        session.phase(),
        DialoguePhase::Idle,
        "持续会话完成一轮后不得错误停止"
    );

    apply_asr_final(&mut session, "再创建一个浏览器")
        .expect("恢复 Listening 后第二句 ASR final 应继续被接受");
    assert_eq!(
        session.phase(),
        DialoguePhase::Thinking,
        "第二句 final 应证明会话可继续进入下一轮 Thinking"
    );
    assert_eq!(
        session.pending_transcript(),
        Some("再创建一个浏览器"),
        "第二轮应替换为新的 pending transcript"
    );
}
