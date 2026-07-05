/// Chase 直线边框层不应再由 canvas 挂载，工作态应使用圆角路径上的贪吃蛇 segment。
///
/// 数据构造（含关键数值推导）：
///   GPUI overflow_hidden 只生成矩形 ContentMask，不会按 rounded radius 裁剪子元素。
///   因此任何 top/left/right/bottom 直线 chase layer 都会在圆角区域画出直角线。
///   正确路径是通过 PathBuilder::stroke 画圆角矩形路径上的短 segment。
///
/// 执行过程：
///   1. 读取 canvas/view.rs 源码
///   2. 检查不再导入/调用 chase_border_layer
///   3. 检查不再通过 node_div.child(...) 挂载 chase 子层
///   4. 检查 canvas/view.rs 使用 visual.working_snake 和 render_working_snake_border
///   5. 读取 motion/mod.rs 源码，检查不再向 visual 暴露 chase_glow
///   6. 读取 cards/snake.rs 源码，检查使用 PathBuilder::stroke 和 arc_to 画圆角路径
///
/// 预期结果：
///   - 正断言：node_div 仍保留 rounded + overflow_hidden 的自身圆角结构
///   - 负断言：不应导入/调用 chase_border_layer
///   - 负断言：不应继续引用 cards::chase 模块
///   - 负断言：ResolvedNodeVisual 不应继续暴露 chase_glow overlay 出口
///   - 正断言：应使用 visual.working_snake 驱动贪吃蛇 segment
///   - 正断言：应使用 render_working_snake_border 渲染路径 segment
///   - 负断言：canvas view 不应再用 .bg(linear_gradient(...)) 画整圈 ring
#[test]
fn test_working_state_does_not_mount_chase_border_layer() {
    let source = include_str!("../../src/canvas/view.rs");
    let motion_source = include_str!("../../src/motion/mod.rs");
    let snake_source = include_str!("../../src/cards/snake.rs");

    assert!(
        source.contains(".rounded(px(node_radius))"),
        "node card 应继续使用自身 rounded radius 绘制真实卡片圆角"
    );
    assert!(
        source.contains(".overflow_hidden()"),
        "node card 应继续裁剪自身内容，保持原有卡片圆角结构"
    );
    assert!(
        !source.contains("chase_border_layer"),
        "canvas view 不应再导入或调用 chase_border_layer；直线子层会破坏圆角"
    );
    assert!(
        !source.contains("cards::chase"),
        "canvas view 不应再依赖 cards::chase 模块渲染工作态边框"
    );
    assert!(
        !motion_source.contains("chase_glow"),
        "ResolvedNodeVisual 不应再暴露 chase_glow；工作态反馈应停留在自身 chrome visual"
    );
    assert!(
        source.contains("visual.working_snake"),
        "canvas view 应读取 visual.working_snake 来决定是否绘制贪吃蛇 segment"
    );
    assert!(
        source.contains("render_working_snake_border"),
        "canvas view 应调用 render_working_snake_border，而不是直接拼直线 overlay"
    );
    assert!(
        !source.contains(".bg(linear_gradient("),
        "canvas view 不应再用整张卡片背景 linear_gradient 画 full ring"
    );
    assert!(
        snake_source.contains("PathBuilder::stroke"),
        "cards/snake.rs 应使用 PathBuilder::stroke 画路径 segment"
    );
    assert!(
        snake_source.contains(".arc_to("),
        "cards/snake.rs 应使用 arc_to 让 segment 沿圆角转弯，而不是直角折线"
    );
}
