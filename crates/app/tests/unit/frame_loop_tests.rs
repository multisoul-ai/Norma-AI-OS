use super::*;

/// FL-1: runtime frame 和 animation frame 的 notify 原因必须分离。
///
/// 数据构造（含关键数值的推导过程）：
///   runtime report:
///     runtime_changed = true
///     runtime_event_count = 0
///     agent_hook_changed = false
///     agent_hook_pending = false
///     elapsed = 1ms（只用于诊断，不参与 should_notify 计算）
///   animation report:
///     motion_changed = true
///     cursor_changed = false
///     visible_working_snake = true
///     elapsed = 1ms（只用于诊断，不参与 should_notify 计算）
///   persistence_changed 不出现在两类 hot-path report 中，避免 12ms loop 同步写库。
///
/// 执行过程：
///   1. 构造 RuntimeFrameReport，模拟 terminal/runtime 有输出。
///   2. 构造 AnimationFrameReport，模拟只有 running snake 需要 repaint。
///   3. 分别调用 should_notify。
///   4. 读取 reason labels，确认两类 report 不互相污染。
///
/// 预期结果：
///   - 正断言：runtime report 应 notify，因为 runtime_changed=true。
///   - 正断言：runtime reason 只包含 runtime_changed。
///   - 正断言：animation report 应 notify，因为 motion_changed=true 且 snake 可见。
///   - 正断言：animation reason 包含 motion_changed/visible_working_snake。
///   - 负断言：runtime reason 不应包含 motion_changed。
///   - 负断言：animation reason 不应包含 runtime_changed 或 persistence_changed。
#[test]
fn test_frame_loop_reports_keep_runtime_animation_and_persistence_separate() {
    let runtime_report = RuntimeFrameReport {
        runtime_changed: true,
        runtime_event_count: 0,
        agent_hook_changed: false,
        agent_hook_pending: false,
        elapsed: std::time::Duration::from_millis(1),
    };
    let animation_report = AnimationFrameReport {
        motion_changed: true,
        cursor_changed: false,
        visible_working_snake: true,
        voice_window_inset_changed: false,
        elapsed: std::time::Duration::from_millis(1),
    };

    assert!(
        runtime_report.should_notify(),
        "runtime_changed=true 时 runtime frame 应触发 notify"
    );
    assert_eq!(
        runtime_report.notify_reason_labels(),
        "runtime_changed",
        "runtime frame reason 应只包含 runtime_changed，不能混入 animation/persistence"
    );
    assert!(
        animation_report.should_notify(),
        "motion_changed=true 时 animation frame 应触发 notify"
    );
    assert_eq!(
        animation_report.notify_reason_labels(),
        "motion_changed|visible_working_snake",
        "animation frame reason 应明确标记 motion 和 visible snake"
    );
    assert!(
        !runtime_report
            .notify_reason_labels()
            .contains("motion_changed"),
        "runtime frame 不应包含 motion_changed，否则 animation 仍耦合到 runtime loop"
    );
    assert!(
        !animation_report
            .notify_reason_labels()
            .contains("runtime_changed"),
        "animation frame 不应包含 runtime_changed，否则 running snake 仍会驱动 runtime drain"
    );
    assert!(
        !animation_report
            .notify_reason_labels()
            .contains("persistence_changed"),
        "animation frame 不应包含 persistence_changed，否则 animation hot path 仍可能同步写库"
    );
}

/// FL-2：animation frame clock 必须使用两次实际唤醒时刻的差值。
///
/// 数据构造（含关键数值的推导过程）：
///   scheduled interval = 12ms（生产 animation loop 的常规间隔）
///   foreground stall = 900ms
///   actual wake time = started_at + 900ms
///   actual delta = 900ms，而不是 12ms
///
/// 执行过程：
///   1. 用 started_at 创建 AnimationFrameClock。
///   2. 模拟 GPUI 前台阻塞 900ms 后才执行下一次 tick。
///   3. 读取 clock 计算的 delta。
///
/// 预期结果：
///   - 正断言：delta 等于真实经过的 900ms。
///   - 负断言：delta 不等于调度器原计划的 12ms。
#[test]
fn test_animation_frame_clock_uses_actual_elapsed_time_after_foreground_stall() {
    let started_at = std::time::Instant::now();
    let mut clock = AnimationFrameClock::new(started_at);

    let delta = clock.elapsed_at(started_at + std::time::Duration::from_millis(900));

    assert_eq!(
        delta,
        std::time::Duration::from_millis(900),
        "前台阻塞 900ms 后动画应按真实 900ms 推进"
    );
    assert_ne!(
        delta,
        std::time::Duration::from_millis(12),
        "动画不得把实际 900ms 卡顿误认为计划的 12ms tick"
    );
}

/// FL-3：Settings 遮住 Canvas 时，runtime 更新不得继续请求整窗重绘。
///
/// 数据构造（含关键数值的推导过程）：
///   runtime_changed = true（模拟可见 Terminal 持续输出）
///   runtime_event_count = 0（没有需要 Settings 立即展示的独立事件）
///   settings states = 2（closed 作为正对照，open 验证遮罩降载）
///   elapsed = 1ms（只用于诊断，不参与 repaint 策略）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一次由 Terminal 输出触发的 RuntimeFrameReport。
///   2. 在 Settings 关闭时解析 repaint 决策。
///   3. 保持同一 report，仅将 Settings 切换为打开，再次解析 repaint 决策。
///
/// 预期结果：
///   - 正断言：Settings 关闭时 runtime 更新必须刷新可见 Canvas。
///   - 负断言：Settings 打开时同一更新不得刷新被遮住的 Canvas。
///   - 负断言：打开与关闭状态不得产生相同结果，否则遮罩状态未参与资源决策。
#[test]
fn test_runtime_frame_repaint_pauses_while_settings_occludes_canvas() {
    let report = RuntimeFrameReport {
        runtime_changed: true,
        runtime_event_count: 0,
        agent_hook_changed: false,
        agent_hook_pending: false,
        elapsed: std::time::Duration::from_millis(1),
    };

    let settings_closed = report.should_notify_surface(false);
    let settings_open = report.should_notify_surface(true);

    assert!(
        settings_closed,
        "Settings 关闭时 Terminal runtime 更新必须刷新可见 Canvas"
    );
    assert!(
        !settings_open,
        "Settings 打开时 Terminal runtime 更新不得刷新被遮住的 Canvas"
    );
    assert_ne!(
        settings_closed, settings_open,
        "Settings 遮罩状态必须改变 runtime frame 的 repaint 决策"
    );
}

/// FL-4：Settings 遮住 Canvas 时只暂停 Canvas 动画，Voice Preview 反馈仍可重绘。
///
/// 数据构造（含关键数值的推导过程）：
///   canvas report = motion_changed(true) + cursor_changed(true) + visible_working_snake(true)
///   voice report = voice_window_inset_changed(true)，其余动画状态均为 false
///   settings_open = true
///   elapsed = 12ms（生产 animation loop 的常规间隔）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一帧仅包含 Agent Working snake 与 Terminal cursor 的 Canvas 动画。
///   2. 在 Settings 打开时解析该帧的 repaint 决策。
///   3. 构造一帧 Voice Preview PCM 驱动的窗口内缘变化并解析 repaint 决策。
///
/// 预期结果：
///   - 负断言：被 Settings 遮住的 Canvas 动画不得请求整窗重绘。
///   - 正断言：Voice Preview 的窗口内缘反馈必须继续请求重绘。
///   - 负断言：Canvas-only 与 Voice Preview 结果不得相同，避免误伤设置内预览反馈。
#[test]
fn test_animation_frame_repaint_keeps_voice_preview_while_settings_occludes_canvas() {
    let canvas_only = AnimationFrameReport {
        motion_changed: true,
        cursor_changed: true,
        visible_working_snake: true,
        voice_window_inset_changed: false,
        elapsed: std::time::Duration::from_millis(12),
    };
    let voice_preview = AnimationFrameReport {
        motion_changed: false,
        cursor_changed: false,
        visible_working_snake: false,
        voice_window_inset_changed: true,
        elapsed: std::time::Duration::from_millis(12),
    };

    let canvas_repaint = canvas_only.should_notify_surface(true);
    let voice_repaint = voice_preview.should_notify_surface(true);

    assert!(
        !canvas_repaint,
        "Settings 打开时 Agent snake 与 Terminal cursor 不得刷新被遮住的 Canvas"
    );
    assert!(
        voice_repaint,
        "Settings 打开时 Voice Preview 的窗口内缘反馈仍必须刷新"
    );
    assert_ne!(
        canvas_repaint, voice_repaint,
        "Canvas-only 动画与 Voice Preview 在 Settings 内必须采用不同 repaint 决策"
    );
}

/// FL-5：稳定显示的 Working Snake 只能作为诊断状态，不能绕过 MotionStore 的 33ms 节流。
///
/// 数据构造（含关键数值的推导过程）：
///   animation loop interval = 12ms
///   stable snake repaint budget = 33ms
///   current tick = 12ms < 33ms，因此 motion_changed = false
///   visible_working_snake = true（节点仍处于 Working 状态）
///   cursor/voice changes = false
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一帧可见 Working Snake，但 MotionStore 尚未达到 33ms repaint 预算。
///   2. 调用 should_notify，确认可见状态不会自行触发整窗重绘。
///   3. 读取 reason labels，确认诊断信息仍能解释当前存在可见 snake。
///
/// 预期结果：
///   - 负断言：仅 visible_working_snake=true 不得请求重绘，否则会退回约 83 FPS。
///   - 正断言：诊断标签仍应保留 visible_working_snake，便于定位 CPU 热点。
///   - 负断言：诊断标签不得伪造 motion_changed，避免误判实际绘制变化。
#[test]
fn test_visible_working_snake_does_not_bypass_motion_repaint_throttle() {
    let report = AnimationFrameReport {
        motion_changed: false,
        cursor_changed: false,
        visible_working_snake: true,
        voice_window_inset_changed: false,
        elapsed: std::time::Duration::from_millis(12),
    };

    assert!(
        !report.should_notify(),
        "仅 Working Snake 可见但 motion 未变化时不得请求重绘，否则 33ms 节流会失效"
    );
    assert!(
        report
            .notify_reason_labels()
            .contains("visible_working_snake"),
        "可见 Working Snake 应保留在诊断标签中，便于排查动画 CPU 占用"
    );
    assert!(
        !report.notify_reason_labels().contains("motion_changed"),
        "motion_changed=false 时诊断标签不得报告 motion_changed"
    );
}
