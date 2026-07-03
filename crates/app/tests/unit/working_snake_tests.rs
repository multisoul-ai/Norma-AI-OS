use super::*;

fn rect(x: f32, y: f32, w: f32, h: f32) -> CanvasRect {
    CanvasRect::new(x, y, w, h)
}

fn approx(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.0001
}

/// MWS-1: Agent 工作态应输出一段沿圆角边框移动的贪吃蛇 segment，而不是整圈渐变 ring。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 41（任意稳定 id）
///   参考视觉：只有一段高亮沿边框跑，剩余边框保持卡片正常描边
///   本主题内等价色：accent_cyan = 0x27d7eaff，accent_violet = 0x9b55ffff
///   snake 可见 alpha 固定为 0xe6，因此 head=0x27d7eae6、tail=0x9b55ffe6
///   为了保持连续流动，完整绕行周期 = 1.2s：
///     tick(300ms) 后 head_phase = 300 / 1200 = 0.25
///   snake_tail_fraction = 0.24，表示蛇身占整条圆角边框路径的 24%
///
/// 执行过程（逐步说明系统如何处理）：
///   1. ensure_node 创建空闲节点
///   2. set_chase_intent(true) 模拟 Agent Thinking/RunningTool 工作态
///   3. tick(300ms) 让 fade 完成并把蛇头推进到 25% 路径处
///   4. resolve_node_visual 读取 working_snake 数据
///
/// 预期结果：
///   - 正断言：working_snake 存在，canvas 才能画移动 segment
///   - 正断言：head_phase 精确推进到 0.25
///   - 正断言：tail_fraction 精确等于 0.24，避免退化为整圈 ring
///   - 正断言：head_color / tail_color 使用青紫双色
///   - 负断言：tail_fraction 不应大于 0.5，否则视觉上接近整圈发光
///   - 负断言：priority 不应伪装成 Focused，工作态不是输入焦点
#[test]
fn test_agent_working_snake_uses_moving_segment_not_full_ring() {
    let mut store = MotionStore::new(false);
    let theme = Theme::glass_dark();
    let node_id = 41;
    let node_rect = rect(0.0, 0.0, 190.0, 254.0);
    store.ensure_node(node_id, node_rect);

    store.set_chase_intent(node_id, true);
    assert!(
        store.tick(Duration::from_millis(300)),
        "工作态 snake 淡入和 1.2s 路径推进期间应报告视觉变化"
    );
    let visual = store.resolve_node_visual(node_id, node_rect, theme);
    let snake = visual
        .working_snake
        .expect("Agent 工作态淡入后必须提供 working_snake 数据");

    assert!(
        approx(snake.head_phase, 0.25),
        "1.2s 绕行 tick(300ms) 后 head_phase 应为 0.25，实际 {}",
        snake.head_phase
    );
    assert!(
        approx(snake.tail_fraction, 0.24),
        "snake_tail_fraction 应为 0.24，实际 {}",
        snake.tail_fraction
    );
    assert_eq!(
        snake.head_color, 0x27d7eae6,
        "working_snake.head_color 应为 theme.accent_cyan 且 alpha=0xe6"
    );
    assert_eq!(
        snake.tail_color, 0x9b55ffe6,
        "working_snake.tail_color 应为 theme.accent_violet 且 alpha=0xe6"
    );
    assert!(
        snake.tail_fraction < 0.5,
        "snake_tail_fraction 必须小于半圈，否则会退化成整圈发光"
    );
    assert_ne!(
        visual.priority,
        VisualPriority::Focused,
        "Agent 工作态不应伪装成 Focused；真实输入焦点仍应由 selection/focus 控制"
    );
}
