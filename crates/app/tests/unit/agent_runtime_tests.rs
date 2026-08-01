use super::{
    agent_completion_report, artifacts_from_agent_events, execute_agent_card_command,
    execute_norma_adapter_command, terminal_task_report,
};
use agent::AgentEvent;
use canvas::CanvasNodeKind;
use gpui::TestAppContext;
use norma::protocol::{CardCommand, CardCommandResult, CardKind, Placement};
use terminal::{PtyBackend, TestPtyBackend};

fn echo_factory() -> runtime::PtyFactory {
    Box::new(|_cmd, _cols, _rows, tx| Ok(Box::new(TestPtyBackend::echo(tx)) as Box<dyn PtyBackend>))
}

/// ART-1: 对话模型把内部推理包在 `<think>` 标签中时，可见回复只移除推理并完整保留长正文。
///
/// 数据构造（含关键数值的推导过程）：
///   标签外英文正文 = 49 个 Unicode 字符
///   历史截断边界   = 15 个 Unicode 字符
///   超出历史边界   = 49 - 15 = 34 个 Unicode 字符
///   流式事件按真实网络分为 3 块：开始 think、结束 think、英文正文
///
/// 执行过程：
///   1. 将三个 DialogueTextDelta 交给 Agent 运行时的 reply 提取边界；
///   2. 移除完整 `<think>` 区段；
///   3. 将剩余正文完整交给 UI 与 TTS。
///
/// 预期结果：
///   - 正断言：标签外 49 字符英文正文完整保留；
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
        "清洗后的英文正文必须完整进入 UI 与 TTS，不能在十五字符后截断"
    );
    assert!(
        artifacts.reply_text.ends_with("setup steps."),
        "历史十五字符边界之后的英文正文必须继续保留"
    );
    assert!(
        !artifacts.reply_text.contains("<think>")
            && !artifacts.reply_text.contains("The user asked"),
        "内部推理不得作为 TTS 输入；实际 reply: {:?}",
        artifacts.reply_text
    );
    assert!(
        artifacts.play_reply,
        "普通 Dialogue 正文必须继续进入现有流式/静态 TTS 路径"
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

/// ART-3：内部 `TaskQueued` 只能表达处理中，不能冒充目标 Agent 派发回执。
///
/// 数据构造：单个 task_id=7 的 TaskQueued，summary 为明确创建意图。
/// 执行过程：将事件转换为当前语音回合产物。
/// 预期结果：回复同时包含处理中状态和真实意图；不得声称任务已派发或已完成。
#[test]
fn test_task_queued_event_only_reports_processing() {
    let artifacts = artifacts_from_agent_events(vec![AgentEvent::TaskQueued {
        task_id: 7,
        summary: "创建一个终端卡片".to_string(),
    }]);

    assert_eq!(
        artifacts.reply_text, "任务处理中：创建一个终端卡片",
        "TaskQueued 只能转换为内部处理中状态"
    );
    assert!(
        !artifacts.reply_text.contains("已派发") && !artifacts.reply_text.contains("已完成"),
        "queued 状态不得被提升为派发或完成状态"
    );
    assert!(
        !artifacts.play_reply,
        "TaskQueued 的处理中占位只更新文字，不得抢在真实派发回执前播报"
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

/// ART-4b：用户主动查询任务状态时必须得到完整状态回报，不能截断或落入空回复兜底。
///
/// 数据构造（含关键数值的推导过程）：
///   task_id = 12，state = Running，completed_tool_calls = 2
///   完整状态文本 = 29 个 Unicode 字符
///   超出历史边界 = 29 - 15 = 14 个 Unicode 字符
///
/// 执行过程：
///   1. 将状态事件转换为完整内部状态文本；
///   2. 将面向用户的状态文本完整交给 UI 与 TTS。
///
/// 预期结果：
///   - 正断言：回复保留任务 id、running 状态和工具调用次数；
///   - 负断言：Running 状态不得声称已派发或已成功完成。
#[test]
fn test_task_status_event_generates_status_reply() {
    let artifacts = artifacts_from_agent_events(vec![AgentEvent::TaskStatus {
        task_id: 12,
        state: agent::TaskState::Running,
        completed_tool_calls: 2,
    }]);

    assert_eq!(
        artifacts.reply_text, "任务 #12 状态：running；已完成 2 次工具调用",
        "TaskStatus 必须完整保留可核对的任务、状态和工具调用次数"
    );
    assert!(
        !artifacts.reply_text.ends_with('…'),
        "TaskStatus 不得在历史十五字符边界改写为省略号结尾"
    );
    assert!(
        !artifacts.reply_text.contains("已派发") && !artifacts.reply_text.contains("已完成："),
        "Running 状态不得被误报为 queued 或 succeeded"
    );
}

/// ART-5：后台事件泵只把真实终态转换为待播报文本，并保留部分派发与取消事实。
///
/// 数据构造（含关键数值的推导过程）：
///   dispatch terminal states = 3：成功 + 部分派发 + 失败
///   lifecycle terminal states = 3：无副作用取消 + 部分副作用取消 + 中断
///   non-terminal states       = 1：Running
///   total cases               = 3 + 3 + 1 = 7
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐个调用后台事件泵使用的终态报告映射
///   2. 核对成功、混合回执、失败、取消与中断的用户可听文本
///   3. 用 Running 验证中间态不会主动进入待播报队列
///
/// 预期结果：
///   - 正断言：六种终态得到准确文本，部分派发同时保留成功与失败 Agent
///   - 负断言：Running 不生成主动语音报告
#[test]
fn test_terminal_task_reports_cover_all_terminal_states_only() {
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskSucceeded {
            task_id: 1,
            summary: "已创建卡片".to_string(),
        }),
        Some("已创建卡片".to_string()),
        "成功报告必须直接采用真实工具回执摘要"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskPartiallyDispatched {
            task_id: 2,
            summary: "Bella 已派发；Alan 派发失败".to_string(),
        }),
        Some("Bella 已派发；Alan 派发失败".to_string()),
        "部分派发报告必须同时保留已派发与失败的逐 Agent 事实"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskFailed {
            task_id: 3,
            reason: "Provider timeout".to_string(),
        }),
        Some("任务失败：Provider timeout".to_string()),
        "失败报告必须包含真实失败原因"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskCancelled {
            task_id: 4,
            partial_effects: false,
            completed_tool_calls: 0,
        }),
        Some("任务已取消".to_string()),
        "零副作用取消不应虚构已执行操作"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskCancelled {
            task_id: 5,
            partial_effects: true,
            completed_tool_calls: 1,
        }),
        Some("任务已取消；已发生的操作不会回滚".to_string()),
        "部分副作用取消必须提醒用户已发生操作不会回滚"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskInterrupted { task_id: 6 }),
        Some("任务已中断".to_string()),
        "中断终态必须得到独立回报"
    );
    assert_eq!(
        terminal_task_report(&AgentEvent::TaskRunning { task_id: 7 }),
        None,
        "Running 中间态不得主动打断语音对话"
    );
}

/// ART-6：Agent tool bridge 的生产 adapter 必须把连续的 Terminal Create 落到真实 app Runtime。
///
/// 数据构造：headless MultisoulApp 注入 echo PTY 并进入临时画布；构造两条生产 CardCommand::Create。
/// 执行过程：按顺序两次调用 tool pump 使用的私有 `execute_agent_card_command` adapter。
/// 预期结果：两次均返回 Created；Runtime live/document 各有两个 Terminal，且没有 Agent 节点。
#[test]
fn test_agent_card_command_adapter_creates_multiple_terminals_in_real_runtime() {
    let mut gpui = TestAppContext::single();
    let workspace =
        tempfile::TempDir::new().expect("app adapter temp workspace should be creatable");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let results = (0..2)
        .map(|_| {
            cx.update(|window, app_cx| {
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
            })
        })
        .collect::<Vec<_>>();

    assert!(
        results
            .iter()
            .all(|result| matches!(result, Ok(CardCommandResult::Created { .. }))),
        "两次生产 Agent adapter 调用都必须返回 Runtime Created receipt；实际结果：{results:?}"
    );
    let (live_len, nodes) = app.read_with(cx, |app, _cx| {
        (
            app.runtime.live_len(),
            app.runtime.document.nodes().to_vec(),
        )
    });
    assert_eq!(live_len, 2, "生产 adapter 必须注册两个 live terminal");
    assert_eq!(nodes.len(), 2, "生产 adapter 必须创建两个画布节点");
    assert!(
        nodes
            .iter()
            .all(|node| matches!(node.kind, CanvasNodeKind::Term(_))),
        "两个新增节点都必须是 Terminal"
    );
    assert!(
        !nodes
            .iter()
            .any(|node| matches!(node.kind, CanvasNodeKind::Agent(_))),
        "Terminal 创建不得误建 Agent 节点"
    );
}

/// ART-NORMA-1：生产 AgentCardAdapter 用一次命令创建内置 Agent 并派发任务
///
/// 数据构造（含关键数值的推导过程）：
///   Initial Agent definitions = 0
///   `create_agent(codex, task)` calls = 1 → definitions=1、AgentCard=1、active tasks=1
///   Additional `assign_agent_task` calls = 0
///   Expected task_id = initial next id 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 headless GPUI 的真实 MultisoulApp 上执行一次带 task 的 Norma `CreateAgent`。
///   2. Adapter 创建并激活系统命名的 AgentCard。
///   3. Adapter 在同一命令内把完整任务交给生产 FIFO。
///
/// 预期结果：
///   - 正断言：同一回执为 accepted，并同时包含唯一 node_id 与 task_id=1。
///   - 正断言：Coordinator 中存在同一 Agent 的运行任务
///   - 负断言：画布中不出现名为 Norma 的 Agent，也不产生第二张 AgentCard 或第二个任务。
#[test]
fn test_norma_adapter_creates_builtin_agent_and_dispatches_without_confirmation() {
    let mut gpui = TestAppContext::single();
    let workspace =
        tempfile::TempDir::new().expect("Norma Adapter temp workspace should be creatable");
    let (app, cx) = gpui.add_window_view(|_window, cx| {
        crate::app::MultisoulApp::new_with_pty_factory(cx, echo_factory())
    });
    app.update(cx, |app, cx| {
        app.enter_canvas_for_test(workspace.path(), cx)
    });

    let created = cx.update(|window, app_cx| {
        app.update(app_cx, |app, cx| {
            execute_norma_adapter_command(
                app,
                window,
                cx,
                agent::AdapterCommand::CreateAgent {
                    profile: norma::protocol::AgentProfile::Codex,
                    task: Some("检查当前项目有哪些技术栈。".to_string()),
                },
            )
        })
    });
    let created = created.expect("create_agent should execute on the production App Adapter");
    assert_eq!(
        created.status, "accepted",
        "create_agent with a task should report FIFO acceptance, not task completion"
    );
    assert!(
        created.node_id.is_some(),
        "create_agent should return the activated AgentCard node id"
    );
    assert_eq!(
        created.task_id,
        Some(1),
        "the combined command's first production FIFO task should return task_id=1"
    );
    let (agent_nodes, tasks, has_norma) = app.read_with(cx, |app, _cx| {
        let agent_nodes = app
            .runtime
            .document
            .nodes()
            .iter()
            .filter(|node| matches!(node.kind, CanvasNodeKind::Agent(_)))
            .count();
        let tasks = app
            .agent_task_coordinator
            .as_ref()
            .map(agent::AgentTaskCoordinator::tasks)
            .unwrap_or_default();
        let has_norma = app
            .agent_definitions
            .definitions()
            .iter()
            .any(|definition| definition.name.eq_ignore_ascii_case("Norma"));
        (agent_nodes, tasks, has_norma)
    });
    assert_eq!(
        agent_nodes, 1,
        "create plus assignment should reuse one unique AgentCard"
    );
    assert_eq!(
        tasks.len(),
        1,
        "the direct assignment should create exactly one tracked FIFO task"
    );
    assert_eq!(
        tasks[0].task, "检查当前项目有哪些技术栈。",
        "the production coordinator should retain the complete delegated task"
    );
    assert!(
        !has_norma,
        "the Master identity Norma must never be persisted as a Canvas Agent"
    );
}

/// ART-8：Hook 终态通知必须区分完成、失败和中断，运行中状态不能提前播报。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent name = Alan
///   terminal states = 3：Done、Failed("tests failed")、Ended
///   non-terminal states = 1：Thinking
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将 Alan 与三个终态交给完成通知格式化边界
///   2. 将 Thinking 交给相同边界
///   3. 比较可播报文本与 None
///
/// 预期结果：
///   - 正断言：Done/Failed/Ended 分别返回完成、失败、中断文本
///   - 负断言：Thinking 不得产生完成通知
#[test]
fn test_agent_completion_report_only_formats_real_hook_terminal_states() {
    assert_eq!(
        agent_completion_report("Alan", &runtime::AgentTaskState::Done),
        Some("Alan 已完成任务".to_string()),
        "Done 应生成明确的 Alan 完成通知"
    );
    assert_eq!(
        agent_completion_report(
            "Alan",
            &runtime::AgentTaskState::Failed {
                message: "tests failed".to_string(),
            },
        ),
        Some("Alan 任务失败：tests failed".to_string()),
        "Failed 应保留 Hook 的真实失败原因"
    );
    assert_eq!(
        agent_completion_report("Alan", &runtime::AgentTaskState::Ended),
        Some("Alan 任务已中断".to_string()),
        "Ended 应生成中断通知，不能误报完成"
    );
    assert_eq!(
        agent_completion_report("Alan", &runtime::AgentTaskState::Thinking),
        None,
        "Thinking 仍是运行中状态，不得提前生成终态通知"
    );
}
