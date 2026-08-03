//! E2E-05：Agent task 状态驱动 terminal cursor 动画（L1 headless GPUI）。
//!
//! Agent 卡片使用持久的交互式 PTY，因此任务失败或手动中断后，PTY 仍存活并不表示
//! Agent 仍在执行。本用例验证公开 action、任务状态更新与聚焦 cursor 动画意图之间的
//! 用户可见闭环。

use gpui::TestAppContext;
use runtime::{AgentTaskState, AgentTaskUpdate, NodeRunState};
use std::time::Duration;

use crate::harness::{
    create_codex_agent, create_terminal, echo_factory, enter_canvas, make_app, read_runtime,
};

/// E2E-05：失败或手动中断后的 Agent 不得因仍存活的 PTY 保持运行中 cursor 动画。
///
/// 数据构造（含关键数值的推导过程）：
///   echo_factory() 生成确定性 TestPtyBackend，创建后 session.status=Running
///   NewCodexAgent 自动聚焦新 Agent 卡片，满足 cursor 动画的 focused 前置条件
///   Thinking 表示 Agent 正在执行，应该启用动画
///   Failed("tool failed") 表示执行失败；Done 表示 Stop hook（手动中断后的结束态）
///   两个结束态都发生在同一仍存活的 PTY 上，NodeRunState 预期保持 Running
///
/// 执行过程（逐步说明系统如何处理）：
///   1. headless 启动 app，进入 Canvas 并通过公开 NewCodexAgent action 创建卡片
///   2. 注入 Thinking task update，读取公开 e2e 动画状态
///   3. 注入 Failed task update，不退出 PTY，再读取动画状态与 NodeRunState
///   4. 注入 Done task update，模拟 Stop hook，再读取动画状态
///
/// 预期结果：
///   - 正断言：Thinking 时 focused Agent cursor 动画启用
///   - 正断言：Failed 与 Done 时 focused Agent cursor 动画关闭
///   - 正断言：Failed 后 NodeRunState 仍为 Running，证明不是靠杀死 PTY 达成
///   - 负断言：Failed Agent 动画状态不得与 Thinking 相同
///   - 负断言：Done Agent 动画状态不得因 PTY 存活而恢复为运行中
#[test]
fn e2e_05_agent_cursor_stops_when_failed_or_interrupted_while_pty_survives() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, _| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to: AgentTaskState::Thinking,
            task_title: None,
            session_id: None,
        });
        assert!(changed, "Thinking task update 应写入新建 Codex Agent");
    });
    let thinking_active = app.read_with(cx, |app, _| {
        app.focused_terminal_cursor_animation_active_e2e()
    });

    app.update(cx, |app, _| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "StopFailure".to_string(),
            to: AgentTaskState::Failed {
                message: "tool failed".to_string(),
            },
            task_title: None,
            session_id: None,
        });
        assert!(changed, "Failed task update 应覆盖 Thinking 状态");
    });
    let failed_active = app.read_with(cx, |app, _| {
        app.focused_terminal_cursor_animation_active_e2e()
    });
    let failed_run_state = read_runtime(&app, cx, |runtime| runtime.node_run_state(node_id));

    app.update(cx, |app, _| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "Stop".to_string(),
            to: AgentTaskState::Done,
            task_title: None,
            session_id: None,
        });
        assert!(
            changed,
            "Done task update 应覆盖 Failed 状态，模拟手动中断后的 Stop hook"
        );
    });
    let done_active = app.read_with(cx, |app, _| {
        app.focused_terminal_cursor_animation_active_e2e()
    });

    assert!(thinking_active, "Thinking Agent 应展示运行中的 cursor 动画");
    assert!(
        !failed_active,
        "Failed Agent 必须切换到结束后的静态 cursor 效果"
    );
    assert_eq!(
        failed_run_state,
        Some(NodeRunState::Running),
        "Failed 后 PTY 应仍存活，测试必须证明动画不是依赖进程退出才停止"
    );
    assert!(!done_active, "Stop/Done 后 Agent cursor 不得继续运行中动画");
    assert_ne!(
        failed_active, thinking_active,
        "Failed Agent 的 cursor 动画状态不得与运行中的 Thinking 相同"
    );
    assert_ne!(
        done_active, thinking_active,
        "Stop/Done 后 Agent 不得因 PTY 存活恢复为运行中 cursor 动画"
    );

    app.update(cx, |app, _| app.runtime.kill_all());
}

/// E2E-05：用户在聚焦且运行中的 Codex Agent 卡片按 Ctrl+C 时，卡片必须立即停止运行特效，
/// 同时保留既有的终端输入转发路径。
///
/// 数据构造（含关键数值的推导过程）：
///   echo_factory() 生成确定性 TestPtyBackend，PTY 始终保持 Running，排除“进程退出”导致动画停止的干扰。
///   NewCodexAgent 自动聚焦新 Agent 卡片；Thinking 表示当前对话执行中，因此按键前特效应为 active。
///   Ctrl+C 经 terminal::encode 转为 ETX = 0x03；它是 Codex CLI 手动中断当前 turn 的标准终端输入。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. headless 启动 app，进入 Canvas 并创建、聚焦 Codex Agent 卡片。
///   2. 注入 UserPromptSubmit → Thinking，确认运行特效处于 active。
///   3. 通过公开 GPUI keyboard seam 发送 ctrl-c，模拟用户手动中断当前对话。
///   4. 读取公开 e2e 动画投影和 runtime task state。
///
/// 预期结果：
///   - 正断言：Ctrl+C 前 Thinking Agent 的运行特效处于 active。
///   - 正断言：Ctrl+C 后 task state 为 Ended，表示本地已知的手动终止立即结束活动态。
///   - 正断言：Ctrl+C 后运行特效关闭，即使 echo PTY 仍保持 Running。
///   - 负断言：Ctrl+C 后状态不得保留 Thinking，避免没有 Stop Hook 时动画永久运行。
///   - 负断言：Ctrl+C 后特效不得继续与中断前相同的 active 状态。
#[test]
fn e2e_05_ctrl_c_stops_focused_agent_cursor_without_waiting_for_stop_hook() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, _| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to: AgentTaskState::Thinking,
            task_title: None,
            session_id: None,
        });
        assert!(changed, "Thinking task update 应写入新建 Codex Agent");
    });
    let active_before_interrupt = app.read_with(cx, |app, _| {
        app.focused_terminal_cursor_animation_active_e2e()
    });

    cx.simulate_keystrokes("ctrl-c");
    cx.run_until_parked();

    let state_after_interrupt = read_runtime(&app, cx, |runtime| runtime.agent_task_state(node_id));
    let active_after_interrupt = app.read_with(cx, |app, _| {
        app.focused_terminal_cursor_animation_active_e2e()
    });

    assert!(
        active_before_interrupt,
        "Ctrl+C 前 Thinking Agent 应展示运行中的 cursor 动画"
    );
    assert_eq!(
        state_after_interrupt,
        Some(AgentTaskState::Ended),
        "Ctrl+C 必须立即把 Agent 标为 Ended，不能等待可能缺失的 Stop Hook"
    );
    assert!(
        !active_after_interrupt,
        "Ctrl+C 后 Agent cursor 特效必须停止，即使 PTY 仍存活"
    );
    assert_ne!(
        state_after_interrupt,
        Some(AgentTaskState::Thinking),
        "Ctrl+C 后 Agent 不得继续保留 Thinking，否则运行特效会永久卡住"
    );
    assert_ne!(
        active_after_interrupt, active_before_interrupt,
        "Ctrl+C 后特效状态必须与中断前不同，不能继续 active"
    );

    app.update(cx, |app, _| app.runtime.kill_all());
}

/// E2E-05：Thinking 淡入收束后必须改用 67ms ambient timer，Done 后退出 ambient 且 PTY 存活。
///
/// 数据构造（含关键数值的推导过程）：
///   initialization settle = 1000ms > Terminal 初始化总时长 992ms
///   chase fade = 150ms
///   ambient cadence = ceil(1000ms / 15fps) = 67ms
///   focused Done card = mark_card_viewed(true)，不保留未读完成呼吸提示
///   completion settle budget = 600ms > completion token 520ms，也 > chase fade 150ms
///   echo_factory PTY run state = Running，任务 Done 不销毁 session
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建并聚焦 Codex Agent，推进 1000ms 排除卡片初始化动画干扰。
///   2. 注入 Thinking 并触发真实 Canvas render，同步 Working Snake 意图。
///   3. 推进 150ms 完成淡入，读取公开动画调度投影。
///   4. 注入 Done 并标记当前聚焦卡片已查看，推进 600ms 完成淡出/完成反馈。
///   5. 读取调度投影与 PTY 状态。
///
/// 预期结果：
///   - 正断言：Thinking 稳定期报告 ambient active，并使用 Timer(67ms)。
///   - 负断言：Thinking 稳定期不得继续请求逐显示帧。
///   - 正断言：Done 收束后 ambient 与 cursor 均关闭，且不再使用 Timer(67ms)。
///   - 正断言：Done 后 PTY 仍为 Running，证明调度停止不依赖销毁进程。
#[test]
fn e2e_05_thinking_settles_to_67ms_ambient_timer_then_done_exits_ambient() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("ambient 调度测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx);
    });
    cx.run_until_parked();

    app.update(cx, |app, cx| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to: AgentTaskState::Thinking,
            task_title: None,
            session_id: None,
        });
        assert!(changed, "Thinking task update 应写入待测 Codex Agent");
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(150), cx);
    });
    cx.run_until_parked();
    let thinking_schedule = app.read_with(cx, |app, _cx| app.animation_schedule_snapshot_for_e2e());

    app.update(cx, |app, cx| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "Stop".to_string(),
            to: AgentTaskState::Done,
            task_title: None,
            session_id: None,
        });
        assert!(changed, "Done task update 应覆盖 Thinking 状态");
        let viewed = app.runtime.mark_card_viewed(node_id);
        assert!(
            viewed,
            "当前聚焦 Agent 在 Done 后应标记为已查看，排除未读完成呼吸提示"
        );
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(600), cx);
    });
    cx.run_until_parked();
    let done_schedule = app.read_with(cx, |app, _cx| app.animation_schedule_snapshot_for_e2e());
    let done_run_state = read_runtime(&app, cx, |runtime| runtime.node_run_state(node_id));

    assert!(
        thinking_schedule.canvas_motion_active,
        "Thinking 稳定期仍必须报告总 Canvas 动效活动，供性能监控记录为 Animation"
    );
    assert!(
        thinking_schedule.canvas_ambient_active,
        "Thinking 淡入 150ms 收束后必须保留 Working Snake ambient 活动"
    );
    assert_eq!(
        thinking_schedule.timer_interval,
        Some(Duration::from_millis(67)),
        "Thinking 稳定期必须使用 ceil(1000ms / 15fps) = 67ms timer"
    );
    assert!(
        !thinking_schedule.next_frame_requested,
        "Thinking 稳定期不得继续占用逐显示帧回调"
    );
    assert!(
        !done_schedule.canvas_ambient_active,
        "Done 收束后必须退出 Working Snake ambient 活动"
    );
    assert!(
        !done_schedule.cursor_active,
        "Done 收束后 focused Agent cursor 动画必须关闭"
    );
    assert_ne!(
        done_schedule.timer_interval,
        Some(Duration::from_millis(67)),
        "Done 收束后不得继续保留 67ms ambient timer"
    );
    assert_eq!(
        done_run_state,
        Some(NodeRunState::Running),
        "Done 后 echo PTY 必须仍为 Running，动画退出不能依赖杀死 session"
    );

    app.update(cx, |app, _| app.runtime.kill_all());
}

/// E2E-05：六卡 Thinking 稳定期每秒必须实际重绘，但根 Canvas 重绘不得超过 15 次。
///
/// 数据构造（含关键数值的推导过程）：
///   cards = 5 Terminal + 1 Codex Agent = 6
///   initialization settle = 1000ms > 初始化总时长 992ms
///   chase fade settle = 150ms
///   scheduler warmup = 42 × 12ms = 504ms > idle 250ms，也 > persistence tick 120ms
///   measurement window = 1000ms
///   mock steps = 83 × 12ms + 4ms = 1000ms
///   ambient interval = 67ms → floor(1000 / 67) = 14 次 cadence 边界，验收上限 15
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建五张 Terminal，再创建并聚焦第六张 Codex Agent 卡片。
///   2. 推进 1000ms 排除初始化，再注入 Thinking 并推进 150ms 完成淡入。
///   3. 分步预热 504ms，让淡入切换与持久化尾帧退出测量窗口，再清零真实渲染计数。
///   4. 以 83 个 12ms 和最后 4ms 分步推进 mock clock，读取一秒实际渲染次数。
///
/// 预期结果：
///   - 正断言：场景必须精确包含六张卡片，且 Thinking 总活动/ambient 均为 true。
///   - 正断言：根 Canvas 至少重绘 1 次，证明动画没有通过冻结来达标。
///   - 正断言：根 Canvas 至多重绘 15 次，满足稳定期不超过 15fps 的预算。
///   - 负断言：稳定期不得继续请求逐显示帧调度。
#[test]
fn e2e_05_six_card_thinking_canvas_repaints_between_one_and_fifteen_times_per_second() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("六卡 repaint 测试应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());
    for _ in 0..5 {
        create_terminal(&app, cx);
    }
    let node_id = create_codex_agent(&app, cx);

    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(1_000), cx)
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        let changed = app.runtime.apply_agent_task_update(AgentTaskUpdate {
            node_id,
            profile: canvas::AgentProfile::Codex,
            provider_event: "UserPromptSubmit".to_string(),
            to: AgentTaskState::Thinking,
            task_title: None,
            session_id: None,
        });
        assert!(
            changed,
            "六卡场景的 Thinking task update 应写入 Codex Agent"
        );
        cx.notify();
    });
    cx.run_until_parked();
    app.update(cx, |app, cx| {
        app.advance_animations_for_e2e(Duration::from_millis(150), cx)
    });
    cx.run_until_parked();

    let card_count = read_runtime(&app, cx, |runtime| runtime.document.nodes().len());
    let stable_schedule = app.read_with(cx, |app, _cx| app.animation_schedule_snapshot_for_e2e());

    // warmup = 42 × 12ms = 504ms，覆盖 250ms idle 旧 timer 和 120ms persistence 尾帧。
    for _ in 0..42 {
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(12));
        cx.run_until_parked();
    }
    app.update(cx, |app, _cx| app.reset_canvas_render_count_for_e2e());

    // measurement = 83 × 12ms + 4ms = 1000ms；分步推进才能逐次触发 mock timer。
    let mut repaint_steps = Vec::new();
    for step in 1..=83 {
        let before = app.read_with(cx, |app, _cx| app.canvas_render_count_for_e2e());
        cx.cx
            .background_executor
            .advance_clock(Duration::from_millis(12));
        cx.run_until_parked();
        let after = app.read_with(cx, |app, _cx| app.canvas_render_count_for_e2e());
        if after > before {
            repaint_steps.push((step, after - before));
        }
    }
    cx.cx
        .background_executor
        .advance_clock(Duration::from_millis(4));
    cx.run_until_parked();
    let render_count = app.read_with(cx, |app, _cx| app.canvas_render_count_for_e2e());

    assert_eq!(
        card_count, 6,
        "性能验收场景必须精确包含 5 Terminal + 1 Agent = 6 张卡片"
    );
    assert!(
        stable_schedule.canvas_motion_active,
        "稳定 Thinking snake 必须保持总 Canvas 活动，不能被性能监控误记为 Idle"
    );
    assert!(
        stable_schedule.canvas_ambient_active,
        "淡入完成后的 Thinking snake 必须处于 ambient 调度分类"
    );
    assert!(
        !stable_schedule.next_frame_requested,
        "稳定 Thinking snake 不得继续请求逐显示帧调度"
    );
    assert!(
        render_count >= 1,
        "1000ms 稳定期必须至少有 1 次根 Canvas 动画重绘，不能冻结"
    );
    assert!(
        render_count <= 15,
        "1000ms 稳定期根 Canvas 重绘必须 <= 15 次，实际为 {render_count}，steps={repaint_steps:?}"
    );

    app.update(cx, |app, _cx| app.runtime.kill_all());
}
