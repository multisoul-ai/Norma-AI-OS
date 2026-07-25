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
        hovering.canvas_active,
        "hover scalar 的 value 与 target 不一致时必须报告活动动画"
    );
    assert!(
        initializing.canvas_active,
        "Terminal 初始化时间线存在时必须报告活动 Canvas 动画"
    );
    assert!(
        initializing
            .next_visual_boundary
            .is_some_and(|boundary| boundary > Duration::ZERO),
        "初始化活动摘要必须提供正数下一视觉边界"
    );
}
