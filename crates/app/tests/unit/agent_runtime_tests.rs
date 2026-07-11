use super::{artifacts_from_agent_events, execute_agent_card_command, terminal_task_report};
use agent::AgentEvent;
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};
use terminal::{PtyBackend, TestPtyBackend};

fn echo_factory() -> runtime::PtyFactory {
    Box::new(|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))
}

/// ART-1: 对话模型把内部推理包在 `<think>` 标签中时，TTS 只能收到标签外的正文。
///
/// 数据构造：
///   流式事件按真实网络分块拆开：第一个分块含 `<think>` 和英文内部推理，第二个分块闭合
///   标签，第三个分块为用户可听到的英文答复。
///
/// 执行过程：
///   1. 将三个 DialogueTextDelta 交给 Agent 运行时的 reply 提取边界；
///   2. 读取产出的 VoiceTurnArtifacts.reply_text。
///
/// 预期结果：
///   - 正断言：标签外的英文正文逐字保留，不能因为是英文而被过滤；
///   - 负断言：内部 `<think>` 推理及其标签不得进入中文 TTS，避免产生 OOV 警告。
#[test]
fn test_agent_reply_strips_thinking_block_without_dropping_english_answer() {
    let artifacts = artifacts_from_agent_events(vec![
        AgentEvent::DialogueTextDelta {
            text: "<think>The user asked for an English answer.".into(),
        },
        AgentEvent::DialogueTextDelta {
            text: " I should answer directly.</think>".into(),
        },
        AgentEvent::DialogueTextDelta {
            text: "Please open README.md and follow the setup steps.".into(),
        },
    ]);

    assert_eq!(
        artifacts.reply_text, "Please open README.md and follow the setup steps.",
        "清洗只能移除 think 块，标签外英文答复必须完整保留"
    );
    assert!(
        !artifacts.reply_text.contains("<think>")
            && !artifacts.reply_text.contains("The user asked"),
        "内部推理不得作为 TTS 输入；实际 reply: {:?}",
        artifacts.reply_text
    );
}

/// ART-2：没有任何 AgentEvent 时不得伪造“任务已派发”。
///
/// 数据构造：事件列表为空，模拟 Provider 返回空内容且没有 `TaskQueued` 的异常回合。
/// 执行过程：调用 AgentEvent → VoiceTurnArtifacts 的生产转换边界。
/// 预期结果：回复明确表示没有得到有效响应；不得出现“已派发”或“已完成”。
#[test]
fn test_empty_agent_events_never_claim_task_was_dispatched() {
    let artifacts = artifacts_from_agent_events(Vec::new());

    assert_eq!(
        artifacts.reply_text, "我没有收到有效回复，请再试一次。",
        "空事件必须使用诚实的失败兜底，不能伪造任务状态"
    );
    assert!(
        !artifacts.reply_text.contains("已派发") && !artifacts.reply_text.contains("已完成"),
        "没有 TaskQueued/TaskSucceeded 时不得出现成功语义"
    );
}

/// ART-3：只有真实 `TaskQueued` 才能生成“任务已派发”语义。
///
/// 数据构造：单个 task_id=7 的 TaskQueued，summary 为明确创建意图。
/// 执行过程：将事件转换为当前语音回合产物。
/// 预期结果：回复同时包含已派发状态和真实意图；不得声称任务已经完成。
#[test]
fn test_task_queued_event_generates_dispatch_receipt() {
    let artifacts = artifacts_from_agent_events(vec![AgentEvent::TaskQueued {
        task_id: 7,
        summary: "创建一个终端卡片".to_string(),
    }]);

    assert_eq!(
        artifacts.reply_text, "任务已派发：创建一个终端卡片",
        "TaskQueued 应转换为明确、可核对的派发回执"
    );
    assert!(
        !artifacts.reply_text.contains("已完成"),
        "queued 状态不得被提升为完成状态"
    );
}

/// ART-4：项目中断事件必须如实回报，不能落入派发兜底。
///
/// 数据构造：task_id=9 的 TaskInterrupted，没有任何文本或 queued 事件。
/// 执行过程：将中断事件转换为语音回合产物。
/// 预期结果：明确报告任务中断；不得出现“已派发”。
#[test]
fn test_task_interrupted_event_reports_interruption() {
    let artifacts = artifacts_from_agent_events(vec![AgentEvent::TaskInterrupted { task_id: 9 }]);

    assert_eq!(
        artifacts.reply_text, "任务已中断",
        "Interrupted 终态必须有独立、真实的用户回报"
    );
    assert!(
        !artifacts.reply_text.contains("已派发"),
        "中断任务不得落入派发兜底"
    );
}

/// ART-4b：用户主动查询任务状态时必须得到结构化状态回报，不能落入空回复兜底。
///
/// 数据构造：task_id=12、state=Running、已完成两个工具调用的 TaskStatus。
/// 执行过程：将状态事件转换为当前语音回合产物。
/// 预期结果：回复包含任务 id、running 状态和完成工具数；不得声称已派发或已完成。
#[test]
fn test_task_status_event_generates_status_reply() {
    let artifacts = artifacts_from_agent_events(vec![AgentEvent::TaskStatus {
        task_id: 12,
        state: agent::TaskState::Running,
        completed_tool_calls: 2,
    }]);

    assert_eq!(
        artifacts.reply_text, "任务 #12 状态：running；已完成 2 次工具调用",
        "TaskStatus 必须转换为可听懂、可核对的主动查询结果"
    );
    assert!(
        !artifacts.reply_text.contains("已派发") && !artifacts.reply_text.contains("已完成："),
        "Running 状态不得被误报为 queued 或 succeeded"
    );
}

/// ART-5：后台事件泵只把真实终态转换为待播报文本，并区分部分副作用取消。
///
/// 数据构造：成功、失败、无副作用取消、部分副作用取消、中断和非终态 Running 事件。
/// 执行过程：逐个调用事件泵使用的终态报告映射。
/// 预期结果：五种终态得到准确文本；Running 不生成主动语音报告。
#[test]
fn test_terminal_task_reports_cover_all_terminal_states_only() {
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskSucceeded {
            task_id: 1,
            summary: "已完成：创建卡片 #3".to_string(),
        }),
        Some("已完成：创建卡片 #3".to_string()),
        "成功报告必须直接采用真实工具回执摘要"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskFailed {
            task_id: 2,
            reason: "Provider timeout".to_string(),
        }),
        Some("任务失败：Provider timeout".to_string()),
        "失败报告必须包含真实失败原因"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskCancelled {
            task_id: 3,
            partial_effects: false,
            completed_tool_calls: 0,
        }),
        Some("任务已取消".to_string()),
        "零副作用取消不应虚构已执行操作"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskCancelled {
            task_id: 4,
            partial_effects: true,
            completed_tool_calls: 1,
        }),
        Some("任务已取消；已发生的操作不会回滚".to_string()),
        "部分副作用取消必须提醒用户已发生操作不会回滚"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskInterrupted { task_id: 5 }),
        Some("任务已中断".to_string()),
        "中断终态必须得到独立回报"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskRunning { task_id: 6 }),
        None,
        "Running 中间态不得主动打断语音对话"
    );
}

/// ART-6：Agent tool bridge 的生产 adapter 必须把 Terminal Create 落到真实 app Runtime。
///
/// 数据构造：headless MultisoulApp 注入 echo PTY 并进入临时画布；构造生产 CardCommand::Create。
/// 执行过程：直接调用 tool pump 使用的私有 `execute_agent_card_command` adapter。
/// 预期结果：返回 Created；Runtime live/document 各只有一个 Terminal，且没有 Agent 节点。
#[test]
fn test_agent_card_command_adapter_creates_terminal_in_real_runtime() {
    let mut gpui = TestAppContext::single();
    let workspace =
        tempfile::TempDir::new().expect("app adapter temp workspace should be creatable");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let result = cx.update(|window, app_cx| {
        app.update(app_cx, |app, _cx| {
            execute_agent_card_command(
                app,
                window,
                CardCommand::Create {
                    kind: CardKind::Terminal { shell: None },
                    placement: Placement::VisibleArea,
                },
            )
        })
    });

    assert!(
        matches!(result, Ok(CardCommandResult::Created { .. })),
        "生产 Agent adapter 必须返回 Runtime 的 Created receipt；实际结果：{result:?}"
    );
    let (live_len, nodes) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.live_len(),
            app.runtime.document.nodes().to_vec(),
        )
    });
    assert_eq!(live_len, 1, "生产 adapter 必须注册一个 live terminal");
    assert_eq!(nodes.len(), 1, "生产 adapter 必须只创建一个画布节点");
    assert!(
        matches!(nodes[0].kind, CanvasNodeKind::Term(_)),
        "新增节点必须是 Terminal"
    );
    assert!(
        !nodes
            .iter()
            .any(|node| matches!(node.kind, CanvasNodeKind::Agent(_))),
        "Terminal 创建不得误建 Agent 节点"
    );
}
