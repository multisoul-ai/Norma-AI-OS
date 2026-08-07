use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> CanvasRect {
    CanvasRect::new(x, y, w, h)
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}

/// MWS-5: running snake phase 应由渲染时间决定，而不是由 12ms tick 次数决定。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 64（任意稳定 id，与其它 working snake 用例区分）。
///   start_at = Instant::now()。
///   完整绕行周期 = 1200ms。
///   render_at = start_at + 480ms，因此 expected_phase = 480 / 1200 = 0.4。
///   motion tick 只执行一次 12ms，旧 fixed-step 方案会得到 12 / 1200 = 0.01。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ensure_node 创建空闲节点。
///   2. set_chase_intent_at(..., start_at) 记录 working snake 起始时间。
///   3. tick(12ms) 只推进 fade/repaint 状态，模拟后台 tick 被系统调度延迟前只跑了一次。
///   4. resolve_node_visual_at(..., render_at) 使用真实渲染时间解析 working_snake。
///
/// 预期结果：
///   - 正断言：working_snake 存在，Agent running 仍显示贪吃蛇 overlay。
///   - 正断言：head_phase 等于 0.4，证明 sparse tick 后相位追上真实时间。
///   - 负断言：head_phase 不应等于 0.01，否则仍绑定 fixed 12ms tick。
#[test]
fn test_working_snake_phase_uses_render_time_after_sparse_ticks() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 64;
    let node_rect = rect(0.0, 0.0, 190.0, 254.0);
    let start_at = std::time::Instant::now();
    let render_at = start_at + Duration::from_millis(480);
    store.ensure_node(node_id, node_rect);

    store.set_chase_intent_at(node_id, true, start_at);
    store.tick(Duration::from_millis(12));
    let visual = store.resolve_node_visual_at(node_id, node_rect, theme, render_at);
    let snake = visual
        .working_snake
        .expect("Agent running 状态应提供 working_snake overlay");

    assert!(
        approx(snake.head_phase, 0.4),
        "render_at=start+480ms 且周期 1200ms 时 head_phase 应为 0.4，实际 {}",
        snake.head_phase
    );
    assert!(
        !approx(snake.head_phase, 0.01),
        "head_phase 不应等于单次 12ms tick 的 0.01，否则动画仍会被调度抖动拖慢"
    );
}
