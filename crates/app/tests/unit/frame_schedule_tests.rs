use super::*;
use crate::motion::{DispatchReceiptVisualStatus, MotionStore};
use std::time::Duration;

/// FS-1：前台 Canvas 和 Voice 都完全空闲时，动画驱动器必须降到 250ms 定时巡检。
///
/// 数据构造（含关键数值的推导过程）：
///   voice active = false
///   canvas motion active = false
///   cursor active = false
///   screen is Canvas = true
///   window active = true
///   settings open = false
///   expected wakeup = Timer(250ms)
///   active negative control = NextFrame(16ms)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造没有任何动画活动源的前台 Canvas 状态。
///   2. 调用纯 AnimationSchedule seam 计算下一次唤醒方式。
///   3. 分别比较 250ms 定时巡检与 GPUI 下一帧驱动。
///
/// 预期结果：
///   - 正断言：空闲唤醒必须精确为 Timer(250ms)。
///   - 负断言：空闲状态不得请求 NextFrame，否则空闲 CPU 会持续占用。
#[test]
fn test_animation_schedule_uses_250ms_when_canvas_and_voice_are_idle() {
    let activity = AnimationActivity {
        voice_active: false,
        canvas_motion_active: false,
        cursor_active: false,
        screen_is_canvas: true,
        window_active: true,
        settings_open: false,
        next_visual_boundary: None,
    };

    let wakeup = AnimationSchedule::next_wakeup(activity);

    assert_eq!(
        wakeup,
        AnimationWakeup::Timer(Duration::from_millis(250)),
        "Canvas 与 Voice 完全空闲时 animation driver 应以 250ms 低频巡检"
    );
    assert_ne!(
        wakeup,
        AnimationWakeup::NextFrame(Duration::from_millis(16)),
        "空闲 animation driver 不得持续请求 GPUI 下一帧"
    );
}

/// FS-2：可见 Canvas 有活动动画时必须交给 GPUI 下一帧回调驱动。
///
/// 数据构造（含关键数值的推导过程）：
///   initialization next boundary = 7ms
///   display callback = NextFrame(16ms)（显示帧对齐 + 约 60Hz cadence）
///   fixed timer negative control = Timer(16ms)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造可见、前台且 motion active 的 Canvas，未提供更近边界。
///   2. 计算基础动画唤醒方式。
///   3. 在同一活动状态加入 7ms 初始化视觉边界并再次计算。
///
/// 预期结果：
///   - 正断言：普通活动 Canvas 返回 NextFrame(16ms)。
///   - 正断言：存在 7ms 视觉边界时返回 NextFrame(7ms)，避免错过边界。
///   - 负断言：活动 Canvas 不得退回 Timer(16ms) 的 sleep + dispatch 路径。
#[test]
fn test_animation_schedule_uses_next_frame_for_visible_active_canvas() {
    let active_canvas = AnimationActivity {
        voice_active: false,
        canvas_motion_active: true,
        cursor_active: false,
        screen_is_canvas: true,
        window_active: true,
        settings_open: false,
        next_visual_boundary: None,
    };
    let boundary_canvas = AnimationActivity {
        next_visual_boundary: Some(Duration::from_millis(7)),
        ..active_canvas
    };

    let active_wakeup = AnimationSchedule::next_wakeup(active_canvas);
    let boundary_wakeup = AnimationSchedule::next_wakeup(boundary_canvas);

    assert_eq!(
        active_wakeup,
        AnimationWakeup::NextFrame(Duration::from_millis(16)),
        "可见活动 Canvas 应由 GPUI 下一帧回调按 16ms cadence 驱动"
    );
    assert_eq!(
        boundary_wakeup,
        AnimationWakeup::NextFrame(Duration::from_millis(7)),
        "初始化视觉边界仅 7ms 时应在 GPUI 下一显示帧消费该边界"
    );
    assert_ne!(
        active_wakeup,
        AnimationWakeup::Timer(Duration::from_millis(16)),
        "活动 Canvas 不得继续采用固定 16ms timer，否则会累加 dispatch 与绘制耗时"
    );
}

/// FS-3：半透明 Settings 覆盖层打开时，可见 Canvas 动画仍须按显示帧推进。
///
/// 数据构造（含关键数值的推导过程）：
///   voice active wakeup = NextFrame(12ms)
///   settings-open Canvas-only wakeup = NextFrame(16ms)
///   background Canvas-only wakeup = Timer(250ms)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 Voice active + Settings open + window background 的组合状态。
///   2. 分别构造 Settings 打开和窗口后台时的 Canvas-only 活动。
///   3. 比较 Voice、Settings Canvas 与后台 Canvas 的调度结果。
///
/// 预期结果：
///   - 正断言：Voice 在遮挡和后台组合下仍返回 NextFrame(12ms)。
///   - 正断言：Settings 打开但窗口前台时，Canvas-only 返回 NextFrame(16ms)，让透过覆盖层可见的贪吃蛇保持平滑。
///   - 正断言：窗口后台的 Canvas-only 仍降到 Timer(250ms)。
///   - 负断言：Voice 不得被错误降到 Timer(250ms)，避免 PCM meter 卡顿。
#[test]
fn test_animation_schedule_keeps_exposed_canvas_motion_on_next_frame_while_settings_is_open() {
    let voice_with_occluded_canvas = AnimationActivity {
        voice_active: true,
        canvas_motion_active: true,
        cursor_active: true,
        screen_is_canvas: true,
        window_active: false,
        settings_open: true,
        next_visual_boundary: None,
    };
    let settings_canvas_only = AnimationActivity {
        voice_active: false,
        window_active: true,
        ..voice_with_occluded_canvas
    };
    let background_canvas_only = AnimationActivity {
        voice_active: false,
        settings_open: false,
        ..voice_with_occluded_canvas
    };

    let voice_wakeup = AnimationSchedule::next_wakeup(voice_with_occluded_canvas);
    let settings_canvas_wakeup = AnimationSchedule::next_wakeup(settings_canvas_only);
    let background_canvas_wakeup = AnimationSchedule::next_wakeup(background_canvas_only);

    assert_eq!(
        voice_wakeup,
        AnimationWakeup::NextFrame(Duration::from_millis(12)),
        "Voice PCM 活动时必须保持 GPUI 下一帧 + 12ms cadence，即使 Canvas 被遮挡且窗口在后台"
    );
    assert_eq!(
        settings_canvas_wakeup,
        AnimationWakeup::NextFrame(Duration::from_millis(16)),
        "Settings 半透明覆盖层打开时，仍可见的 Canvas-only 动画应按 16ms 显示帧推进"
    );
    assert_eq!(
        background_canvas_wakeup,
        AnimationWakeup::Timer(Duration::from_millis(250)),
        "窗口后台时 Canvas-only 动画应降到 250ms"
    );
    assert_ne!(
        voice_wakeup,
        AnimationWakeup::Timer(Duration::from_millis(250)),
        "Voice 活动不得错误继承 Canvas 的后台降频预算"
    );
    assert_ne!(
        settings_canvas_wakeup,
        AnimationWakeup::Timer(Duration::from_millis(250)),
        "Settings 打开时仍可见的贪吃蛇不得退回 250ms 巡检，否则会出现明显卡顿"
    );
}

/// FS-DR-1：dispatch effect 存活时必须使用 16ms 显示帧，清理后恢复 250ms idle。
///
/// 数据构造（含关键数值的推导过程）：
///   dispatch effects = 1
///   flight = 480ms = 9 × 50ms + 30ms
///   impact = 430ms = 8 × 50ms + 30ms
///   active cadence = 16ms；idle cadence = 250ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 MotionStore 创建一个 Submitted dispatch effect。
///   2. 把 Motion activity 投影进 AnimationSchedule，读取存活期唤醒方式。
///   3. 推进完整 480ms + 430ms 生命周期，再次投影并读取空闲唤醒方式。
///
/// 预期结果：
///   - 正断言：effect 存活时返回 NextFrame(16ms)。
///   - 正断言：effect 清理后返回 Timer(250ms)。
///   - 负断言：清理后不得继续返回 NextFrame，避免隐藏的连续帧占用。
#[test]
fn test_dispatch_effect_activity_drives_next_frame_then_returns_idle() {
    let mut motion = MotionStore::new(false);
    motion.spawn_dispatch_receipt(
        "frame-schedule:1",
        971,
        DispatchReceiptVisualStatus::Submitted,
    );
    let visible_activity = |canvas_motion_active| AnimationActivity {
        voice_active: false,
        canvas_motion_active,
        cursor_active: false,
        screen_is_canvas: true,
        window_active: true,
        settings_open: false,
        next_visual_boundary: None,
    };
    let active_wakeup =
        AnimationSchedule::next_wakeup(visible_activity(motion.activity_summary().canvas_active));

    // flight = 480ms = 9 × 50ms + 30ms；单次推进不超过 50ms clamp。
    for _ in 0..9 {
        motion.tick(Duration::from_millis(50));
    }
    motion.tick(Duration::from_millis(30));
    // impact = 430ms = 8 × 50ms + 30ms。
    for _ in 0..8 {
        motion.tick(Duration::from_millis(50));
    }
    motion.tick(Duration::from_millis(30));
    let idle_wakeup =
        AnimationSchedule::next_wakeup(visible_activity(motion.activity_summary().canvas_active));

    assert_eq!(
        active_wakeup,
        AnimationWakeup::NextFrame(Duration::from_millis(16)),
        "dispatch effect 存活时必须按 16ms GPUI 显示帧推进"
    );
    assert_eq!(
        idle_wakeup,
        AnimationWakeup::Timer(Duration::from_millis(250)),
        "最后一个 dispatch effect 清理后必须恢复 250ms idle 巡检"
    );
    assert_ne!(
        idle_wakeup,
        AnimationWakeup::NextFrame(Duration::from_millis(16)),
        "dispatch effect 清理后不得继续请求 16ms 连续帧"
    );
}

/// RG-1：16ms 预算内的三次 Runtime dirty 必须合并成一次 repaint。
///
/// 数据构造（含关键数值的推导过程）：
///   focused budget = 16ms
///   dirty timestamps = 0ms、4ms、8ms
///   4ms < 16ms，8ms < 16ms → 两次中间 poll 均不得放行
///   boundary timestamp = 16ms → 恰好放行 1 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 0ms 请求 runtime_changed，并立即 poll。
///   2. 在 4ms/8ms 追加 runtime_events/agent_hook 原因并分别 poll。
///   3. 在 16ms poll，再在 32ms 验证没有重复放行。
///
/// 预期结果：
///   - 负断言：0ms、4ms、8ms 都不得提前放行。
///   - 正断言：16ms 恰好放行一次，且合并三个原因。
///   - 负断言：32ms 不得重复放行，累计次数不能变成 3。
#[test]
fn test_repaint_gate_coalesces_three_runtime_changes_into_one_16ms_repaint() {
    let started_at = std::time::Instant::now();
    let budget = Duration::from_millis(16);
    let mut gate = RepaintGate::default();
    let mut emitted_count = 0;

    gate.request(started_at, RepaintReason::RuntimeChanged);
    let at_zero = gate.poll(started_at, budget);
    gate.request(
        started_at + Duration::from_millis(4),
        RepaintReason::RuntimeEvents,
    );
    let at_four = gate.poll(started_at + Duration::from_millis(4), budget);
    gate.request(
        started_at + Duration::from_millis(8),
        RepaintReason::AgentHook,
    );
    let at_eight = gate.poll(started_at + Duration::from_millis(8), budget);
    let at_sixteen = gate.poll(started_at + Duration::from_millis(16), budget);
    emitted_count += usize::from(at_sixteen.is_some());
    let at_thirty_two = gate.poll(started_at + Duration::from_millis(32), budget);
    emitted_count += usize::from(at_thirty_two.is_some());

    assert!(
        at_zero.is_none(),
        "0ms 首次 dirty 只能置 pending，不能绕过 16ms 合并预算"
    );
    assert!(
        at_four.is_none(),
        "4ms < 16ms，第二次 dirty 不得提前放行 repaint"
    );
    assert!(
        at_eight.is_none(),
        "8ms < 16ms，第三次 dirty 不得提前放行 repaint"
    );
    assert!(
        at_sixteen.is_some_and(|emission| {
            emission.reasons.contains(RepaintReason::RuntimeChanged)
                && emission.reasons.contains(RepaintReason::RuntimeEvents)
                && emission.reasons.contains(RepaintReason::AgentHook)
        }),
        "16ms 必须放行一次且保留 runtime_changed/runtime_events/agent_hook 三个原因"
    );
    assert!(
        at_thirty_two.is_none(),
        "pending 已在 16ms 清空，32ms 不得重复放行"
    );
    assert_eq!(
        emitted_count, 1,
        "三次预算内 dirty 必须合并为恰好一次 repaint，不能各自放行"
    );
}

/// RG-2：只有一次 dirty 时，也必须一直保留到预算边界并绘制最终帧。
///
/// 数据构造（含关键数值的推导过程）：
///   focused budget = 16ms
///   single dirty = 0ms
///   before boundary = 15ms < 16ms
///   boundary = 16ms
///   post boundary = 32ms（已无新的 dirty）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 仅在 0ms 请求一次 TerminalOutput。
///   2. 15ms poll，确认 pending 未被丢弃。
///   3. 16ms poll 放行，32ms 再 poll 检查不会重复。
///
/// 预期结果：
///   - 负断言：15ms 不得提前放行。
///   - 正断言：16ms 必须放行这一条最终输出。
///   - 负断言：32ms 不得因旧 pending 再次放行。
#[test]
fn test_repaint_gate_preserves_single_pending_change_until_deadline() {
    let started_at = std::time::Instant::now();
    let budget = Duration::from_millis(16);
    let mut gate = RepaintGate::default();

    gate.request(started_at, RepaintReason::TerminalOutput);
    let before_deadline = gate.poll(started_at + Duration::from_millis(15), budget);
    let at_deadline = gate.poll(started_at + Duration::from_millis(16), budget);
    let after_emission = gate.poll(started_at + Duration::from_millis(32), budget);

    assert!(
        before_deadline.is_none(),
        "15ms 尚未达到 16ms 预算，单次 dirty 应继续保持 pending"
    );
    assert!(
        at_deadline
            .is_some_and(|emission| { emission.reasons.contains(RepaintReason::TerminalOutput) }),
        "16ms 边界必须放行唯一的 TerminalOutput，不能等待下一次 dirty"
    );
    assert!(
        after_emission.is_none(),
        "单次 pending 放行后必须清空，32ms 不得重复 repaint"
    );
}

/// RB-1：前台聚焦且仍在运行的 Terminal 必须使用 16ms repaint 预算。
///
/// 数据构造（含关键数值的推导过程）：
///   display cadence ≈ 1000ms / 60Hz = 16.67ms
///   selected focused budget = 16ms
///   background negative control = 33ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 Canvas 可见、窗口前台、聚焦运行中 Terminal 的上下文。
///   2. 调用纯 repaint_budget 策略。
///   3. 与 16ms 正值和 33ms 后台对照比较。
///
/// 预期结果：
///   - 正断言：预算精确为 16ms。
///   - 负断言：不得错误采用 33ms，避免输入回显多等一帧。
#[test]
fn test_repaint_budget_is_16ms_for_focused_running_terminal() {
    let context = RepaintContext {
        screen_is_canvas: true,
        settings_open: false,
        window_active: true,
        focused_running_terminal: true,
        voice_active: false,
    };

    let budget = context.repaint_budget();

    assert_eq!(
        budget,
        Some(Duration::from_millis(16)),
        "前台聚焦运行中 Terminal 应使用 16ms repaint 预算"
    );
    assert_ne!(
        budget,
        Some(Duration::from_millis(33)),
        "聚焦输入路径不得错误采用 33ms 后台预算"
    );
}

/// RB-2：后台 Terminal 输出使用 33ms，Settings 遮挡时暂停 Canvas repaint。
///
/// 数据构造（含关键数值的推导过程）：
///   background cadence ≈ 1000ms / 30Hz = 33.33ms
///   selected background budget = 33ms
///   focused negative control = 16ms
///   settings open = true → no Canvas repaint budget
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造窗口后台但 Canvas 仍为当前 screen 的上下文。
///   2. 读取后台 repaint 预算。
///   3. 保持其余状态不变，仅打开 Settings 后再次读取预算。
///
/// 预期结果：
///   - 正断言：后台预算精确为 33ms。
///   - 负断言：后台不得继续采用 16ms。
///   - 正断言：Settings 遮挡时返回 None，但 Runtime drain 由调用方继续执行。
#[test]
fn test_repaint_budget_is_33ms_for_background_terminal_output() {
    let background = RepaintContext {
        screen_is_canvas: true,
        settings_open: false,
        window_active: false,
        focused_running_terminal: false,
        voice_active: false,
    };
    let settings_open = RepaintContext {
        settings_open: true,
        ..background
    };

    let background_budget = background.repaint_budget();
    let settings_budget = settings_open.repaint_budget();

    assert_eq!(
        background_budget,
        Some(Duration::from_millis(33)),
        "后台 Terminal 输出应以 33ms 最小间距合并 repaint"
    );
    assert_ne!(
        background_budget,
        Some(Duration::from_millis(16)),
        "后台 Terminal 不得维持前台 16ms repaint 频率"
    );
    assert_eq!(
        settings_budget, None,
        "Settings 完全遮挡 Canvas 时应暂停 Canvas repaint；Runtime drain 仍由 frame loop 继续"
    );
}
