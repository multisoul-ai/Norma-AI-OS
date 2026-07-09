use super::{
    build_template_reports, default_echo_mock_script, run_dialogue_turn_with_mock,
    TemplateReportItem,
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
