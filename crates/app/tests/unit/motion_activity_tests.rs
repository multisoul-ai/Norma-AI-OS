use super::*;
use canvas::CanvasRect;
use std::time::Duration;

/// MA-1：MotionStore 必须把静止、scalar 过渡和初始化边界汇总为纯活动摘要。
///
/// 数据构造（含关键数值的推导过程）：
///   settled node count = 1
///   hover target transition = 0.0 → 1.0
///   settle tick = 1s（远大于 hover token 的百毫秒级持续时间）
///   initialization first boundary > 0ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建一个完全静止的节点并读取活动摘要。
///   2. 将 hover 从 false 改为 true，读取 scalar 过渡摘要。
///   3. tick 1s 完成过渡，再启动 Terminal 初始化并读取下一视觉边界。
///
/// 预期结果：
///   - 负断言：静止节点不得被误报为活动动画。
///   - 正断言：hover 过渡必须报告 canvas_active。
///   - 正断言：初始化必须报告 canvas_active 和正数视觉边界。
#[test]
fn test_motion_activity_summary_tracks_scalar_and_initialization_activity() {
    let node_id = 401;
    let rect = CanvasRect::new(0.0, 0.0, 320.0, 180.0);
    let mut store = MotionStore::new(false);
    store.ensure_node(node_id, rect);

    let settled = store.activity_summary();
    store.set_hover(node_id, true);
    let hovering = store.activity_summary();
    store.tick(Duration::from_secs(1));
    store.start_initialization(node_id, InitializationAnimationKind::Terminal);
    let initializing = store.activity_summary();

    assert!(
        !settled.canvas_active,
        "只有一个 settled 节点时不得报告活动 Canvas 动画"
    );
    assert!(
        !settled.frame_synced_active,
        "settled 节点不得报告 frame-synced 活动"
    );
    assert!(
        !settled.ambient_active,
        "settled 节点不得报告稳定 ambient 活动"
    );
    assert!(
        hovering.canvas_active,
        "hover scalar 的 value 与 target 不一致时必须报告活动动画"
    );
    assert!(
        hovering.frame_synced_active,
        "hover scalar 过渡必须留在逐显示帧通道"
    );
    assert!(
        !hovering.ambient_active,
        "纯 hover 过渡不得误报为长期 ambient 循环"
    );
    assert!(
        initializing.canvas_active,
        "Terminal 初始化时间线存在时必须报告活动 Canvas 动画"
    );
    assert!(
        initializing.frame_synced_active,
        "Terminal 初始化必须保持 frame-synced，不能被降到 67ms"
    );
    assert!(
        initializing
            .next_visual_boundary
            .is_some_and(|boundary| boundary > Duration::ZERO),
        "初始化活动摘要必须提供正数下一视觉边界"
    );
}

/// MA-2：Working Snake 淡入使用显示帧，稳定循环降为 ambient，Reduce Motion 不启动循环。
///
/// 数据构造（含关键数值的推导过程）：
///   chase fade token = 150ms
///   before tick: chase_fade.value = 0，target = 1 → frame-synced scalar active
///   settle tick = 150ms → chase_fade.value = 1，target = 1
///   reduced-motion store = true → chase 使用静态反馈，不推进循环相位
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 普通模式节点开启 chase，立即读取淡入期活动摘要。
///   2. tick(150ms) 完成淡入，再读取稳定期活动摘要。
///   3. Reduce Motion 节点开启同一 chase 意图，读取静态状态摘要。
///
/// 预期结果：
///   - 正断言：淡入期同时报告 canvas、frame-synced 与 ambient 活动，调度器优先显示帧。
///   - 正断言：稳定期仍报告 canvas/ambient，但 frame-synced 已关闭。
///   - 负断言：Reduce Motion 不得报告 ambient 或 frame-synced 循环。
#[test]
fn test_motion_activity_summary_separates_working_snake_fade_from_stable_ambient() {
    let node_id = 402;
    let rect = CanvasRect::new(0.0, 0.0, 320.0, 180.0);
    let mut store = MotionStore::new(false);
    store.ensure_node(node_id, rect);
    store.set_chase_intent(node_id, true);
    let fading = store.activity_summary();

    let fade_changed = store.tick(Duration::from_millis(150));
    let stable = store.activity_summary();

    let reduced_node_id = 403;
    let mut reduced_store = MotionStore::new(true);
    reduced_store.ensure_node(reduced_node_id, rect);
    reduced_store.set_chase_intent(reduced_node_id, true);
    let reduced = reduced_store.activity_summary();

    assert!(
        fading.canvas_active,
        "Working Snake 淡入期必须报告 Canvas 活动"
    );
    assert!(
        fading.frame_synced_active,
        "0→1 的 150ms chase fade 必须由显示帧即时推进"
    );
    assert!(
        fading.ambient_active,
        "淡入期 chase 循环已激活，应保留 ambient 分类供淡入结束后无缝接管"
    );
    assert!(
        fade_changed,
        "tick(150ms) 必须完成 chase fade 并产生可见变化"
    );
    assert!(
        stable.canvas_active,
        "淡入完成后 Working Snake 仍是活动 Canvas 状态"
    );
    assert!(
        !stable.frame_synced_active,
        "chase fade 在 150ms 收束后不得继续占用逐显示帧通道"
    );
    assert!(
        stable.ambient_active,
        "淡入完成后的 Working Snake 必须转为稳定 ambient 循环"
    );
    assert!(
        !reduced.frame_synced_active,
        "Reduce Motion 的静态 chase 反馈不得启动 frame-synced 过渡"
    );
    assert!(
        !reduced.ambient_active,
        "Reduce Motion 下不得启动 Working Snake ambient timer"
    );
    assert!(
        !reduced.canvas_active,
        "仅有 Reduce Motion 静态 chase 状态时不应报告需要持续 tick 的 Canvas 活动"
    );
}
