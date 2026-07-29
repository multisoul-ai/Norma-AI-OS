use super::*;

fn cadence_rect() -> CanvasRect {
    CanvasRect::new(0.0, 0.0, 190.0, 254.0)
}

/// MRC-1：稳定 working snake 跨过 33ms 边界后必须保留余量，避免不规则帧间隔降速。
///
/// 数据构造（含关键数值的推导过程）：
///   fade-in tick = 150ms（完成 chase_fade，节流累计清零）
///   steady ticks = 20ms × 5 = 100ms
///   repaint interval = 33ms
///   expected repaint count = floor(100 / 33) = 3
///   expected remainder = 100 - 33 × 3 = 1ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建节点并进入 working snake 状态。
///   2. tick(150ms) 完成淡入，使后续只经过稳定期节流分支。
///   3. 连续输入五个 20ms tick，逐次记录是否请求 repaint。
///   4. 读取 chase_repaint_elapsed，验证第三次边界后的 1ms 余量仍被保存。
///
/// 预期结果：
///   - 正断言：第 2、4、5 个 tick 分别跨过 33ms 边界并请求 repaint。
///   - 负断言：第 1、3 个 tick 尚未跨过边界，不应请求 repaint。
///   - 正断言：总 repaint 次数为 3，而不是清零策略产生的 2 次。
///   - 正断言：最终余量为 1ms，不得被重置为 0ms。
#[test]
fn test_working_snake_repaint_cadence_preserves_overshoot() {
    let node_id = 61;
    let mut store = MotionStore::new(false);
    store.ensure_node(node_id, cadence_rect());
    store.set_chase_intent(node_id, true);
    let fade_in_changed = store.tick(Duration::from_millis(150));

    let first = store.tick(Duration::from_millis(20));
    let second = store.tick(Duration::from_millis(20));
    let third = store.tick(Duration::from_millis(20));
    let fourth = store.tick(Duration::from_millis(20));
    let fifth = store.tick(Duration::from_millis(20));
    let remainder = store
        .nodes
        .get(&node_id)
        .expect("working snake 节点在 cadence 测试期间必须存在")
        .chase_repaint_elapsed;
    let repaint_count = [first, second, third, fourth, fifth]
        .into_iter()
        .filter(|changed| *changed)
        .count();

    assert!(fade_in_changed, "150ms tick 必须完成 snake 淡入并触发首帧");
    assert!(!first, "第 1 个 20ms tick 小于 33ms，不应 repaint");
    assert!(second, "累计 40ms 跨过首个 33ms 边界，应 repaint");
    assert!(!third, "保留 7ms 余量后再加 20ms 仅为 27ms，不应 repaint");
    assert!(fourth, "累计 47ms 跨过第二个 33ms 边界，应 repaint");
    assert!(
        fifth,
        "保留 14ms 余量后再加 20ms 跨过第三个 33ms 边界，应 repaint"
    );
    assert_eq!(
        repaint_count, 3,
        "100ms / 33ms 应产生 3 次稳定期 repaint，清零余量会错误地只产生 2 次"
    );
    assert_eq!(
        remainder,
        Duration::from_millis(1),
        "跨过三个 33ms 边界后必须保留 100 - 99 = 1ms 余量"
    );
}

/// MRC-2：稳定 breathing 跨过 33ms 边界后必须保留余量，保持约 30fps 的长期节奏。
///
/// 数据构造（含关键数值的推导过程）：
///   fade-in tick = 150ms（完成 breathing_fade，节流累计清零）
///   steady ticks = 20ms × 5 = 100ms
///   repaint interval = 33ms
///   expected repaint count = floor(100 / 33) = 3
///   expected remainder = 100 - 33 × 3 = 1ms
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建节点并进入 Cool breathing 状态。
///   2. tick(150ms) 完成淡入，使后续只经过稳定期节流分支。
///   3. 连续输入五个 20ms tick，逐次记录是否请求 repaint。
///   4. 读取 breathing_repaint_elapsed，验证 1ms 余量被保留。
///
/// 预期结果：
///   - 正断言：第 2、4、5 个 tick 请求 repaint。
///   - 负断言：第 1、3 个 tick 不请求 repaint。
///   - 正断言：总 repaint 次数为 3。
///   - 正断言：最终余量为 1ms，长期 cadence 不会降到约 25fps。
#[test]
fn test_breathing_repaint_cadence_preserves_overshoot() {
    let node_id = 62;
    let mut store = MotionStore::new(false);
    store.ensure_node(node_id, cadence_rect());
    store.set_breathing_intent(node_id, BreathingIntent::Cool);
    let fade_in_changed = store.tick(Duration::from_millis(150));

    let first = store.tick(Duration::from_millis(20));
    let second = store.tick(Duration::from_millis(20));
    let third = store.tick(Duration::from_millis(20));
    let fourth = store.tick(Duration::from_millis(20));
    let fifth = store.tick(Duration::from_millis(20));
    let remainder = store
        .nodes
        .get(&node_id)
        .expect("breathing 节点在 cadence 测试期间必须存在")
        .breathing_repaint_elapsed;
    let repaint_count = [first, second, third, fourth, fifth]
        .into_iter()
        .filter(|changed| *changed)
        .count();

    assert!(
        fade_in_changed,
        "150ms tick 必须完成 breathing 淡入并触发首帧"
    );
    assert!(!first, "第 1 个 20ms tick 小于 33ms，不应 repaint");
    assert!(second, "累计 40ms 跨过首个 33ms 边界，应 repaint");
    assert!(!third, "保留 7ms 余量后再加 20ms 仅为 27ms，不应 repaint");
    assert!(fourth, "累计 47ms 跨过第二个 33ms 边界，应 repaint");
    assert!(
        fifth,
        "保留 14ms 余量后再加 20ms 跨过第三个 33ms 边界，应 repaint"
    );
    assert_eq!(
        repaint_count, 3,
        "100ms / 33ms 应产生 3 次 breathing repaint，清零余量会错误地只产生 2 次"
    );
    assert_eq!(
        remainder,
        Duration::from_millis(1),
        "跨过三个 33ms 边界后必须保留 100 - 99 = 1ms 余量"
    );
}
