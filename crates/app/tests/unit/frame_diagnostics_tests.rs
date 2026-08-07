use super::*;

/// FD-1: frame 诊断日志默认关闭，显式打开后应保留触发 repaint 的关键原因。
///
/// 数据构造（含关键数值的推导过程）：
///   env unset        → 默认关闭，避免诊断日志本身在每帧制造额外 IO。
///   env "1"          → 显式打开，供用户复现掉帧时采集关键路径。
///   runtime_events=2 → 两条 runtime event 应直接进入 report，便于定位 hook/runtime 抖动。
///   elapsed=7ms      → 单 tick 的前台处理耗时，用于判断是否接近 12ms 帧循环预算。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用 None / Some("1") 调用诊断开关解析函数。
///   2. 构造 FrameTickReport，runtime_changed=false 但 runtime_event_count=2。
///   3. 读取 should_notify 与 elapsed_ms。
///
/// 预期结果：
///   - 负断言：未设置环境变量时不应启用 frame diagnostics。
///   - 正断言："1" 应启用 frame diagnostics。
///   - 正断言：runtime_event_count=2 应触发 should_notify。
///   - 负断言：report 不应把 elapsed=7ms 误报为 0ms。
#[test]
fn test_frame_diagnostics_env_gate_and_notify_reason_are_explicit() {
    assert!(
        !crate::frame_diagnostics::enabled_from_value(None),
        "未设置 MULTISOUL_FRAME_DIAGNOSTICS 时不应输出每帧诊断日志"
    );
    assert!(
        crate::frame_diagnostics::enabled_from_value(Some("1")),
        "MULTISOUL_FRAME_DIAGNOSTICS=1 应启用每帧诊断日志"
    );

    let report = crate::frame_diagnostics::FrameTickReport {
        runtime_changed: false,
        runtime_event_count: 2,
        agent_hook_changed: false,
        motion_changed: false,
        cursor_changed: false,
        persistence_changed: false,
        norma_pending: false,
        agent_hook_pending: false,
        elapsed: Duration::from_millis(7),
    };

    assert!(
        report.should_notify(),
        "runtime_event_count=2 应触发 repaint，否则 runtime 事件会被吞掉"
    );
    assert_eq!(
        report.elapsed_ms(),
        7.0,
        "elapsed=7ms 应按毫秒输出，便于和 12ms tick 预算比较"
    );
    assert_ne!(
        report.elapsed_ms(),
        0.0,
        "diagnostics 不应把非零 tick 耗时误报为 0ms"
    );
}

/// FD-2: frame_tick 诊断应输出可机器聚合的 notify reason bits 和可读 labels。
///
/// 数据构造（含关键数值的推导过程）：
///   runtime_changed = true      → bit0 = 1
///   runtime_event_count = 3     → bit1 = 2
///   cursor_changed = true       → bit4 = 16
///   其余 reason = false         → 对应 bit 不应置位
///   notify_reason_bits = 1 + 2 + 16 = 19
///   notify_reason_labels 应包含 runtime_changed/runtime_events/cursor_changed 三项。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造一个同时包含 runtime、runtime events、cursor 三类触发源的 FrameTickReport。
///   2. 调用 notify_reason_bits，验证 bitmask 可直接按日志聚合。
///   3. 调用 notify_reason_labels，验证人读日志时能快速识别 repaint 来源。
///
/// 预期结果：
///   - 正断言：notify_reason_bits 精确等于 19。
///   - 正断言：labels 精确列出三类触发源。
///   - 负断言：bits 不应包含 motion_changed 的 bit3=8。
///   - 负断言：labels 不应包含 motion_changed，避免误判 motion 动画触发 repaint。
#[test]
fn test_frame_tick_report_exposes_notify_reason_bits_and_labels() {
    let report = crate::frame_diagnostics::FrameTickReport {
        runtime_changed: true,
        runtime_event_count: 3,
        agent_hook_changed: false,
        motion_changed: false,
        cursor_changed: true,
        persistence_changed: false,
        norma_pending: false,
        agent_hook_pending: false,
        elapsed: Duration::from_millis(9),
    };

    assert_eq!(
        report.notify_reason_bits(),
        19,
        "runtime_changed(bit0) + runtime_events(bit1) + cursor_changed(bit4) 应得到 bitmask=19"
    );
    assert_eq!(
        report.notify_reason_labels(),
        "runtime_changed|runtime_events|cursor_changed",
        "notify reason labels 应按固定顺序列出触发 repaint 的来源"
    );
    assert_eq!(
        report.notify_reason_bits() & 8,
        0,
        "motion_changed=false 时 bit3 不应置位，否则会误判 motion 是 repaint 来源"
    );
    assert!(
        !report.notify_reason_labels().contains("motion_changed"),
        "motion_changed=false 时 labels 不应包含 motion_changed"
    );
}

/// FD-3：repaint 诊断必须区分 requested、coalesced 与 emitted，不能把合并当丢帧。
///
/// 数据构造（含关键数值的推导过程）：
///   requested = 3
///   first request coalesced = false
///   second/third request coalesced = true + true = 2
///   emitted = 1
///   pending age = 16ms
///   selected budget = 16ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建启用状态的纯诊断计数器。
///   2. 记录三次 request，其中后两次标记为被 gate 合并。
///   3. 记录一次 16ms 边界 emission 并读取 snapshot。
///
/// 预期结果：
///   - 正断言：requested/coalesced/emitted 分别精确为 3/2/1。
///   - 正断言：pending age 与 selected budget 都为 16ms。
///   - 负断言：coalesced 不得等于 requested 或被记作 dropped。
#[test]
fn test_frame_diagnostics_distinguishes_requested_coalesced_and_emitted_repaints() {
    let started_at = std::time::Instant::now();
    let mut diagnostics = crate::frame_diagnostics::RepaintDiagnostics::new(true, started_at);

    diagnostics.record_request(false);
    diagnostics.record_request(true);
    diagnostics.record_request(true);
    diagnostics.record_emission(Duration::from_millis(16), Duration::from_millis(16));
    let snapshot = diagnostics.snapshot();

    assert_eq!(
        snapshot.requested, 3,
        "三次 Runtime dirty 都必须计入 requested，不能只统计实际绘制"
    );
    assert_eq!(
        snapshot.coalesced, 2,
        "第二、第三次预算内 request 应精确计为两次 coalesced"
    );
    assert_eq!(
        snapshot.emitted, 1,
        "三次 request 合并后应只有一次 emitted repaint"
    );
    assert_eq!(
        snapshot.last_pending_age,
        Duration::from_millis(16),
        "实际放行时 pending age 应精确记录为 16ms"
    );
    assert_eq!(
        snapshot.last_selected_budget,
        Duration::from_millis(16),
        "诊断应记录本次选择的前台 16ms budget"
    );
    assert_ne!(
        snapshot.coalesced, snapshot.requested,
        "首次 request 不是合并，coalesced 不得被误报成全部 requested"
    );
}
