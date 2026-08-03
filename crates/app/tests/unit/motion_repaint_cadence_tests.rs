use super::*;

fn cadence_rect() -> CanvasRect {
    CanvasRect::new(0.0, 0.0, 190.0, 254.0)
}

/// MRC-1：稳定 working snake 跨过 67ms 边界后必须保留余量，且不超过 15fps。
///
/// 数据构造（含关键数值的推导过程）：
///   fade-in tick = 150ms（完成 chase_fade，节流累计清零）
///   steady ticks = 50ms × 4 = 200ms
///   repaint interval = ceil(1000ms / 15fps) = 67ms
///   expected repaint count = floor(200 / 67) = 2
///   expected remainder = 200 - 67 × 2 = 66ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建节点并进入 working snake 状态。
///   2. tick(150ms) 完成淡入，使后续只经过稳定期节流分支。
///   3. 连续输入四个 50ms tick，逐次记录是否请求 repaint。
///   4. 读取 chase_repaint_elapsed，验证两个边界后的 66ms 余量仍被保存。
///
/// 预期结果：
///   - 正断言：第 2、3 个 tick 分别跨过 67ms 边界并请求 repaint。
///   - 负断言：第 1、4 个 tick 尚未跨过下一边界，不应请求 repaint。
///   - 正断言：200ms 内总 repaint 次数精确为 2，不得超过 15fps 预算。
///   - 正断言：最终余量为 66ms，不得被重置为 0ms。
#[test]
fn test_working_snake_repaint_cadence_preserves_overshoot() {
    let node_id = 61;
    let mut store = MotionStore::new(false);
    store.ensure_node(node_id, cadence_rect());
    store.set_chase_intent(node_id, true);
    let fade_in_changed = store.tick(Duration::from_millis(150));

    let first = store.tick(Duration::from_millis(50));
    let second = store.tick(Duration::from_millis(50));
    let third = store.tick(Duration::from_millis(50));
    let fourth = store.tick(Duration::from_millis(50));
    let remainder = store
        .nodes
        .get(&node_id)
        .expect("working snake 节点在 cadence 测试期间必须存在")
        .chase_repaint_elapsed;
    let repaint_count = [first, second, third, fourth]
        .into_iter()
        .filter(|changed| *changed)
        .count();

    assert!(fade_in_changed, "150ms tick 必须完成 snake 淡入并触发首帧");
    assert!(!first, "第 1 个 50ms tick 小于 67ms，不应 repaint");
    assert!(second, "累计 100ms 跨过首个 67ms 边界，应 repaint");
    assert!(
        third,
        "保留 33ms 后再加 50ms 跨过第二个 67ms 边界，应 repaint"
    );
    assert!(!fourth, "保留 16ms 后再加 50ms 仅为 66ms，不应 repaint");
    assert_eq!(
        repaint_count, 2,
        "floor(200ms / 67ms) = 2，稳定 snake 在 200ms 内不得产生第三次 repaint"
    );
    assert_eq!(
        remainder,
        Duration::from_millis(66),
        "跨过两个 67ms 边界后必须保留 200 - 134 = 66ms 余量"
    );
}

/// MRC-2：稳定 breathing 跨过 67ms 边界后必须保留余量，且不超过 15fps。
///
/// 数据构造（含关键数值的推导过程）：
///   fade-in tick = 150ms（完成 breathing_fade，节流累计清零）
///   steady ticks = 50ms × 4 = 200ms
///   repaint interval = ceil(1000ms / 15fps) = 67ms
///   expected repaint count = floor(200 / 67) = 2
///   expected remainder = 200 - 67 × 2 = 66ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建节点并进入 Cool breathing 状态。
///   2. tick(150ms) 完成淡入，使后续只经过稳定期节流分支。
///   3. 连续输入四个 50ms tick，逐次记录是否请求 repaint。
///   4. 读取 breathing_repaint_elapsed，验证 66ms 余量被保留。
///
/// 预期结果：
///   - 正断言：第 2、3 个 tick 请求 repaint。
///   - 负断言：第 1、4 个 tick 不请求 repaint。
///   - 正断言：总 repaint 次数精确为 2。
///   - 正断言：最终余量为 66ms，长期 cadence 不会因清零而漂移。
#[test]
fn test_breathing_repaint_cadence_preserves_overshoot() {
    let node_id = 62;
    let mut store = MotionStore::new(false);
    store.ensure_node(node_id, cadence_rect());
    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    let fade_in_changed = store.tick(Duration::from_millis(150));

    let first = store.tick(Duration::from_millis(50));
    let second = store.tick(Duration::from_millis(50));
    let third = store.tick(Duration::from_millis(50));
    let fourth = store.tick(Duration::from_millis(50));
    let remainder = store
        .nodes
        .get(&node_id)
        .expect("breathing 节点在 cadence 测试期间必须存在")
        .breathing_repaint_elapsed;
    let repaint_count = [first, second, third, fourth]
        .into_iter()
        .filter(|changed| *changed)
        .count();

    assert!(
        fade_in_changed,
        "150ms tick 必须完成 breathing 淡入并触发首帧"
    );
    assert!(!first, "第 1 个 50ms tick 小于 67ms，不应 repaint");
    assert!(second, "累计 100ms 跨过首个 67ms 边界，应 repaint");
    assert!(
        third,
        "保留 33ms 后再加 50ms 跨过第二个 67ms 边界，应 repaint"
    );
    assert!(!fourth, "保留 16ms 后再加 50ms 仅为 66ms，不应 repaint");
    assert_eq!(
        repaint_count, 2,
        "floor(200ms / 67ms) = 2，稳定 breathing 在 200ms 内不得产生第三次 repaint"
    );
    assert_eq!(
        remainder,
        Duration::from_millis(66),
        "跨过两个 67ms 边界后必须保留 200 - 134 = 66ms 余量"
    );
}
