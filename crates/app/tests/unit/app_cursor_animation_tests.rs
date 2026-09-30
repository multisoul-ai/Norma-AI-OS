use super::*;

/// 已完成任务的 Agent 仍可输入时，光标应继续呼吸；PTY 退出后停止。
///
/// 数据构造（含关键数值的推导过程）：
///   Agent task=Done，PTY=Running 表示任务已结束但交互式终端仍可输入。
///   同一节点改为 PTY=Finished 表示输入通道已关闭。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在前台窗口解析 Agent + Running 的光标动画意图。
///   2. 保持 Agent 类型，将 PTY 改为 Finished 后再次解析。
///
/// 预期结果：
///   - 正断言：Done + Running 的 Agent 光标保持动画。
///   - 负断言：PTY 已退出的 Agent 光标不再动画。
#[test]
fn test_finished_agent_cursor_tracks_live_terminal_instead_of_task_state() {
    let active = focused_cursor_animation_active(
        true,
        canvas::NodeType::Agent,
        Some(runtime::NodeRunState::Running),
    );
    let exited = focused_cursor_animation_active(
        true,
        canvas::NodeType::Agent,
        Some(runtime::NodeRunState::Finished { code: Some(0) }),
    );

    assert!(active, "Done Agent 的 PTY 仍可输入时，光标应持续呼吸");
    assert!(!exited, "Agent PTY 退出后光标不应继续请求重绘");
}

/// MEM-DISPLAY-1：应用窗口退到后台后必须暂停 Terminal cursor repaint。
///
/// 数据构造（含关键数值的推导过程）：
///   window states = 2 个（active=true、active=false）
///   node type     = Term
///   run state     = Running
///   repaint rate  = 前台约 30fps；后台预算必须为 0fps
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 active=true 解析运行中 Terminal 的 cursor 动画意图。
///   2. 保持相同节点和运行状态，仅把 window active 改为 false。
///   3. 比较两种窗口状态的动画意图。
///
/// 预期结果：
///   - 正断言：前台运行中 Terminal 仍保留 cursor 动画业务效果。
///   - 负断言：后台窗口不得继续发起 cursor repaint。
///   - 负断言：前后台结果不得相同，否则窗口状态没有参与资源决策。
#[test]
fn test_terminal_cursor_animation_pauses_when_window_is_inactive() {
    let foreground = focused_cursor_animation_active(
        true,
        canvas::NodeType::Term,
        Some(runtime::NodeRunState::Running),
    );
    let background = focused_cursor_animation_active(
        false,
        canvas::NodeType::Term,
        Some(runtime::NodeRunState::Running),
    );

    assert!(
        foreground,
        "前台运行中 Terminal 必须继续显示 cursor 动画，不能为了省内存删除业务效果"
    );
    assert!(
        !background,
        "后台窗口必须暂停 cursor repaint，不能继续放大 GPUI DisplayLink 泄漏"
    );
    assert_ne!(
        foreground, background,
        "相同 Terminal 在前后台必须产生不同动画意图"
    );
}
