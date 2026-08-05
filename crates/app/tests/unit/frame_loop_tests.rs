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
        voice_caption_changed: false,
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

/// FL-3：120Hz GPUI 回调必须在进入 App 状态计算前按 16ms cadence 合并为约 60Hz 动画推进。
///
/// 数据构造（含关键数值的推导过程）：
///   display callback interval = 8ms（近似 1000 / 120Hz = 8.33ms）
///   Canvas cadence = 16ms（近似 1000 / 60Hz = 16.67ms）
///   callback timestamps = 8ms、16ms、24ms、32ms
///   expected due decisions = false、true、false、true
///   expected animation deltas = 16ms、16ms
///   expected state-update count = 32 / 16 = 2，而不是 120Hz 下的 4
///
/// 执行过程：
///   1. 用 started_at 创建 AnimationFrameClock。
///   2. 按四个 8ms 显示帧依次检查 callback 是否已到 state-update deadline。
///   3. 仅在两个 due callback 上消费实际 elapsed delta。
///   4. 统计 32ms 窗口中的实际状态推进次数。
///
/// 预期结果：
///   - 负断言：8ms 与 24ms 尚未达到 16ms，不应进入动画状态计算或 repaint。
///   - 正断言：16ms 与 32ms 各产生一个精确 16ms delta。
///   - 正断言：总状态更新次数为 2，避免 ProMotion 屏为每个 120Hz callback 执行完整活动状态计算。
#[test]
fn test_animation_frame_clock_caps_120hz_state_updates_at_60hz_cadence() {
    let started_at = std::time::Instant::now();
    let cadence = std::time::Duration::from_millis(16);
    let mut clock = AnimationFrameClock::new(started_at);

    let at_eight = clock.is_due_at(started_at + std::time::Duration::from_millis(8), cadence);
    let at_sixteen = clock.is_due_at(started_at + std::time::Duration::from_millis(16), cadence);
    let first_delta =
        at_sixteen.then(|| clock.elapsed_at(started_at + std::time::Duration::from_millis(16)));
    let at_twenty_four =
        clock.is_due_at(started_at + std::time::Duration::from_millis(24), cadence);
    let at_thirty_two = clock.is_due_at(started_at + std::time::Duration::from_millis(32), cadence);
    let second_delta =
        at_thirty_two.then(|| clock.elapsed_at(started_at + std::time::Duration::from_millis(32)));
    let update_count = [at_eight, at_sixteen, at_twenty_four, at_thirty_two]
        .into_iter()
        .filter(|due| *due)
        .count();

    assert!(
        !at_eight,
        "首个 8ms 显示帧尚未达到 16ms Canvas cadence，不应进入动画状态计算"
    );
    assert!(at_sixteen, "累计 16ms 时必须进入一次 Canvas 状态计算");
    assert_eq!(
        first_delta,
        Some(cadence),
        "首个 due callback 必须消费精确 16ms 动画步长"
    );
    assert!(
        !at_twenty_four,
        "消费首个 16ms 后仅新增 8ms，24ms 显示帧不应重复进入状态计算"
    );
    assert!(
        at_thirty_two,
        "累计到 32ms 时必须进入第二次 Canvas 状态计算"
    );
    assert_eq!(
        second_delta,
        Some(cadence),
        "第二个 due callback 必须消费第二个精确 16ms 动画步长"
    );
    assert_eq!(
        update_count, 2,
        "32ms 内应只有 2 次 Canvas 状态更新，不能按 120Hz 显示回调执行 4 次"
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

/// FL-4：半透明 Settings 覆盖层打开时，Canvas 动画与 Voice Preview 都须继续重绘。
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
///   - 正断言：Canvas 动画必须请求整窗重绘，避免半透明覆盖层外可见的贪吃蛇冻结。
///   - 正断言：Voice Preview 的窗口内缘反馈必须继续请求重绘。
///   - 负断言：两类动画均不允许在 Settings 打开时被错误抑制。
#[test]
fn test_animation_frame_repaint_keeps_canvas_and_voice_preview_while_settings_is_open() {
    let canvas_only = AnimationFrameReport {
        motion_changed: true,
        cursor_changed: true,
        voice_caption_changed: false,
        visible_working_snake: true,
        voice_window_inset_changed: false,
        elapsed: std::time::Duration::from_millis(12),
    };
    let voice_preview = AnimationFrameReport {
        motion_changed: false,
        cursor_changed: false,
        voice_caption_changed: false,
        visible_working_snake: false,
        voice_window_inset_changed: true,
        elapsed: std::time::Duration::from_millis(12),
    };

    let canvas_repaint = canvas_only.should_notify_surface(true);
    let voice_repaint = voice_preview.should_notify_surface(true);

    assert!(
        canvas_repaint,
        "Settings 半透明覆盖层打开时，Agent snake 与 Terminal cursor 必须刷新可见的 Canvas"
    );
    assert!(
        voice_repaint,
        "Settings 打开时 Voice Preview 的窗口内缘反馈仍必须刷新"
    );
    assert!(
        canvas_repaint && voice_repaint,
        "Settings 打开时 Canvas-only 与 Voice Preview 动画都不得被错误暂停"
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
        voice_caption_changed: false,
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
