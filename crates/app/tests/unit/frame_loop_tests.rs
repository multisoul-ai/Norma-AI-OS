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
