//! E2E-05：Agent task 状态驱动 terminal cursor 动画（L1 headless GPUI）。
//!
//! Agent 卡片使用持久的交互式 PTY，因此任务失败或手动中断后，PTY 仍存活并不表示
//! Agent 仍在执行。本用例验证公开 action、任务状态更新与聚焦 cursor 动画意图之间的
//! 用户可见闭环。

use gpui::TestAppContext;
use runtime::{AgentTaskState, AgentTaskUpdate, NodeRunState};

use crate::harness::{create_codex_agent, echo_factory, enter_canvas, make_app, read_runtime};

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
