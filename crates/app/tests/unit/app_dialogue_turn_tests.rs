use super::{
    build_template_reports, default_echo_mock_script, run_dialogue_turn_with_mock,
    run_voice_turn_job, TemplateReportItem, VoiceTurnJob, VoiceTurnLlm, DIALOGUE_SYSTEM_PROMPT,
};
use crate::app::dialogue::card_tools::RecordingCardCommandRunner;
use dialogue::{decide_template_report, DialoguePhase, ReportPlayDecision};
use llm::{MockStep, ToolCall};
use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};

/// DLT-1: 默认 Mock 剧本应回显用户 transcript，与 Plan 01 stub 文案一致。
///
/// 数据构造：
///   transcript =「你好」
///
/// 执行过程：
///   1. 调用 default_echo_mock_script
///
/// 预期结果：
///   - 正断言：单步 Text 为「收到：你好」
///   - 负断言：不应包含工具调用步骤
#[test]
fn test_default_echo_mock_script_prefixes_transcript() {
    let script = default_echo_mock_script("你好");

    assert_eq!(
        script,
        vec![MockStep::Text("收到：你好".into())],
        "默认 Mock 剧本应回显 transcript"
    );
    assert!(
        script
            .iter()
            .all(|step| !matches!(step, MockStep::Tools(_))),
        "默认 Mock 剧本不应包含工具调用"
    );
}

/// DLT-5: 生产 system prompt 必须要求实质回答并输出 TTS 友好纯文本。
///
/// 数据构造：
///   prompt = DIALOGUE_SYSTEM_PROMPT。
///
/// 执行过程：
///   1. 检查对泛化确认词、回复长度和不可播报格式的约束。
///
/// 预期结果：
///   - 正断言：明确禁止只回复「好的/收到」，要求直接回答；
///   - 正断言：限制为简短中文纯文本且禁止 emoji/Markdown；
///   - 负断言：不得继续使用只有身份声明的空泛 prompt。
#[test]
fn test_dialogue_system_prompt_requires_substantive_tts_friendly_answers() {
    assert!(
        DIALOGUE_SYSTEM_PROMPT.contains("不能只回复")
            && DIALOGUE_SYSTEM_PROMPT.contains("好的")
            && DIALOGUE_SYSTEM_PROMPT.contains("收到"),
        "system prompt 应禁止只回复泛化确认词"
    );
    assert!(
        DIALOGUE_SYSTEM_PROMPT.contains("直接回答") && DIALOGUE_SYSTEM_PROMPT.contains("简短中文"),
        "system prompt 应要求简短、直接且有实质内容"
    );
    assert!(
        DIALOGUE_SYSTEM_PROMPT.contains("表情符号") && DIALOGUE_SYSTEM_PROMPT.contains("Markdown"),
        "system prompt 应排除 Matcha 难以播报的 emoji 与 Markdown"
    );
    assert_ne!(
        DIALOGUE_SYSTEM_PROMPT, "你是 Norma 语音助手。",
        "生产 prompt 不得退化为空泛身份声明"
    );
}

/// DLT-2: 注入 RecordingRunner 时 Create→SendText 应按顺序执行并返回最终回复。
///
/// 数据构造：
///   Mock 三步：Create → SendText(node_id:7) → 文本「好的，已经发到终端」；
///   RecordingRunner：Create→Created{7}，SendText→Sent{7}。
///
/// 执行过程：
///   1. run_dialogue_turn_with_mock
///
/// 预期结果：
///   - 正断言：reply_text ==「好的，已经发到终端」
///   - 正断言：executed_commands 顺序为 Create(Terminal) 再 SendText{7,"ls"}
///   - 正断言：template_reports 含创建与发送的中文模板
///   - 负断言：不应返回 Err
#[test]
fn test_run_dialogue_turn_create_then_send_text_records_command_order() {
    let mut runner = RecordingCardCommandRunner::new(|command| match command {
        CardCommand::Create { .. } => Ok(CardCommandResult::Created { node_id: 7 }),
        CardCommand::SendText { node_id, .. } => Ok(CardCommandResult::Sent { node_id }),
        other => Err(format!("未实现的命令: {other:?}")),
    });
    let mock_script = vec![
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
    ];

    let artifacts = run_dialogue_turn_with_mock(
        "创建终端并发送 ls",
        mock_script,
        &mut runner,
        DialoguePhase::Thinking,
        false,
    )
    .expect("Create→SendText→文本 三轮应成功完成");

    assert_eq!(
        artifacts.reply_text, "好的，已经发到终端",
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
        "RecordingRunner 应按 Create→SendText 顺序记录命令"
    );
    assert_eq!(
        artifacts.template_reports,
        vec![
            TemplateReportItem {
                text: "已创建卡片 7".into(),
                decision: ReportPlayDecision::Queue,
            },
            TemplateReportItem {
                text: "已发送到卡片 7".into(),
                decision: ReportPlayDecision::Queue,
            },
        ],
        "Thinking 阶段工具结果应生成排队的中文模板报告"
    );
    assert_eq!(
        artifacts.queued_template_reports,
        vec!["已创建卡片 7".to_string(), "已发送到卡片 7".to_string()],
        "Thinking 阶段模板报告应进入 queued 列表"
    );
}

/// DLT-3: Thinking 阶段模板报告应通过 decide_template_report 排队而非立即播放。
///
/// 数据构造：
///   command_results = [Created{3}]
///   phase = Thinking，user_speech_active = false
///
/// 执行过程：
///   1. build_template_reports
///
/// 预期结果：
///   - 正断言：decision == Queue
///   - 负断言：不应为 PlayNow
#[test]
fn test_build_template_reports_queues_during_thinking() {
    let results = vec![CardCommandResult::Created { node_id: 3 }];
    let (reports, queued) = build_template_reports(&results, DialoguePhase::Thinking, false);

    assert_eq!(
        reports,
        vec![TemplateReportItem {
            text: "已创建卡片 3".into(),
            decision: ReportPlayDecision::Queue,
        }],
        "Thinking 阶段应生成创建成功模板"
    );
    assert_eq!(
        decide_template_report(DialoguePhase::Thinking, false),
        ReportPlayDecision::Queue,
        "Thinking 阶段 decide_template_report 应返回 Queue"
    );
    assert_eq!(
        queued,
        vec!["已创建卡片 3".to_string()],
        "排队列表应包含模板文案"
    );
    assert_ne!(
        reports[0].decision,
        ReportPlayDecision::PlayNow,
        "Thinking 阶段不应立即播放模板"
    );
}

/// DLT-4: 后台 voice turn 必须通过请求通道逐轮执行 Create→SendText。
///
/// 数据构造：
///   Mock 三轮：Create → SendText(node_id:7) → 最终文本；
///   tool bridge：Create 返回 Created{7}，SendText 返回 Sent{7}。
///
/// 执行过程：
///   1. 在线程中运行 run_voice_turn_job，模拟 GPUI background executor；
///   2. 当前线程接收 VoiceToolRequest，模拟 GPUI 前台执行画布副作用；
///   3. 将每步结果回传给后台 LLM turn，直到通道关闭。
///
/// 预期结果：
///   - 正断言：请求顺序为 Create、SendText；
///   - 正断言：SendText 使用 Create 返回的 node_id=7；
///   - 正断言：最终 reply 与模板报告完整；
///   - 负断言：后台 worker 不得因等待 tool result 死锁。
#[test]
fn test_background_voice_turn_bridges_tools_to_foreground_in_order() {
    let job = VoiceTurnJob {
        transcript: "创建终端并发送 ls".to_string(),
        llm: VoiceTurnLlm::Mock(vec![
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
            MockStep::Text("已经发送".into()),
        ]),
    };
    let (request_tx, request_rx) = async_channel::bounded(1);
    let worker = std::thread::spawn(move || run_voice_turn_job(job, request_tx));
    let mut commands = Vec::new();

    while let Ok(request) = request_rx.recv_blocking() {
        let command = request.command().clone();
        let result = match &command {
            CardCommand::Create { .. } => CardCommandResult::Created { node_id: 7 },
            CardCommand::SendText { node_id, .. } => CardCommandResult::Sent { node_id: *node_id },
            other => panic!("测试 bridge 收到未预期命令: {other:?}"),
        };
        commands.push(command);
        request.complete(Ok(result));
    }

    let artifacts = worker
        .join()
        .expect("后台 voice turn 线程不应 panic")
        .expect("Create→SendText 后应返回最终回复");

    assert_eq!(
        commands,
        vec![
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
        "后台 worker 应按 Create→SendText 顺序请求前台副作用"
    );
    assert_eq!(
        artifacts.reply_text, "已经发送",
        "tool bridge 完成后应保留最终 LLM 文本"
    );
    assert_eq!(
        artifacts.queued_template_reports,
        vec!["已创建卡片 7".to_string(), "已发送到卡片 7".to_string()],
        "后台 turn 的工具结果应生成完整模板播报队列"
    );
    assert_ne!(
        artifacts.executed_commands.len(),
        0,
        "后台 turn 不得在 tool bridge 存在时退化为零命令"
    );
}

/// DLT-L3-1: 生产 VoiceTurnJob 对不同普通问题不能都退化为「好的」。
///
/// 数据构造（含关键数值的推导过程）：
///   transcripts =「你是谁，请简单介绍自己」「请用一句话解释什么是无限画布」；
///   Ollama endpoint/model 使用生产默认值；每轮使用生产 system prompt 与 card tools。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐轮运行真实 VoiceTurnJob；
///   2. tool bridge 对意外工具调用返回明确错误，避免 worker 死锁；
///   3. 收集最终 reply_text。
///
/// 预期结果：
///   - 正断言：两轮均返回至少 4 个字符的可播报文本，且内容不同；
///   - 负断言：任一回复不得精确退化为「好的」。
#[test]
#[ignore = "requires running Ollama and qwen3.5:0.8b"]
fn test_live_voice_turn_answers_are_not_generic_okay() {
    let replies =
        ["你是谁，请简单介绍自己", "请用一句话解释什么是无限画布"].map(run_live_voice_turn);

    for reply in &replies {
        assert!(
            reply.chars().count() >= 4,
            "真实对话回复应包含实质内容，不能只有短确认词，实际: {reply:?}"
        );
        assert_ne!(reply.trim(), "好的", "真实对话回复不得退化为「好的」");
    }
    assert_ne!(
        replies[0], replies[1],
        "不同用户问题应产生不同回复，实际: {replies:?}"
    );
    eprintln!("live production voice replies: {replies:?}");
}

fn run_live_voice_turn(transcript: &str) -> String {
    let endpoint = std::env::var("MULTISOUL_OLLAMA_ENDPOINT")
        .unwrap_or_else(|_| "http://127.0.0.1:11434".to_string());
    let model =
        std::env::var("MULTISOUL_DIALOGUE_MODEL").unwrap_or_else(|_| "qwen3.5:0.8b".to_string());
    let job = VoiceTurnJob {
        transcript: transcript.to_string(),
        llm: VoiceTurnLlm::Ollama { endpoint, model },
    };
    let (request_tx, request_rx) = async_channel::bounded(1);
    let worker = std::thread::spawn(move || run_voice_turn_job(job, request_tx));

    while let Ok(request) = request_rx.recv_blocking() {
        request.complete(Err(
            "普通问答 L3 不应执行画布工具；请直接回答用户问题".to_string()
        ));
    }

    worker
        .join()
        .expect("真实 VoiceTurnJob worker 不应 panic")
        .expect("真实 VoiceTurnJob 应返回最终文本")
        .reply_text
}
