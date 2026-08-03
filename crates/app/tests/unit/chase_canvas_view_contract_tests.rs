/// Working Snake 不应挂在 CanvasSurface 子树内，且仍使用圆角路径 segment。
///
/// 数据构造（含关键数值推导）：
///   GPUI overflow_hidden 只生成矩形 ContentMask，不会按 rounded radius 裁剪子元素。
///   因此任何 top/left/right/bottom 直线 chase layer 都会在圆角区域画出直角线。
///   正确路径是通过 PathBuilder::stroke 画圆角矩形路径上的短 segment。
///
/// 执行过程：
///   1. 读取 canvas/view.rs 与 canvas/working_snakes.rs 源码
///   2. 检查不再导入/调用 chase_border_layer
///   3. 检查 canvas/view.rs 委托 working_snakes 模块同步 WorkingSnakeView，且不把它挂进 node_div。
///   4. 检查 app/render.rs 把 working_snake_views 挂为 CanvasSurface 的兄弟叠层。
///   5. 检查 tooltip、queue panel 与 dispatch effect 位于 Snake 之后，避免被边框穿透覆盖。
///   6. 检查 cards/snake/view.rs 由局部 Entity 调用 render_working_snake_border。
///   7. 读取 cards/snake.rs，检查使用 PathBuilder::stroke 和 arc_to 画圆角路径。
///
/// 预期结果：
///   - 正断言：node_div 仍保留 rounded + overflow_hidden 的自身圆角结构
///   - 负断言：不应导入/调用 chase_border_layer
///   - 负断言：不应继续引用 cards::chase 模块
///   - 正断言：Canvas 只同步 WorkingSnakeView，不直接构建蛇路径。
///   - 正断言：根视图应把 working_snake_views 挂在 cached CanvasSurface 之后。
///   - 正断言：Canvas 交互叠层与 dispatch effect 应挂在 Snake 之后。
///   - 负断言：node_div 不得直接挂载 snake_view，否则局部 notify 会失效 CanvasSurface。
///   - 正断言：WorkingSnakeView 应调用 render_working_snake_border 渲染路径 segment。
///   - 负断言：canvas view 不应再用 .bg(linear_gradient(...)) 画整圈 ring
#[test]
fn test_working_state_does_not_mount_chase_border_layer() {
    let source = include_str!("../../src/canvas/view.rs");
    let working_snakes_source = include_str!("../../src/canvas/working_snakes.rs");
    let overlays_source = include_str!("../../src/canvas/overlays.rs");
    let app_render_source = include_str!("../../src/app/render.rs");
    let snake_source = include_str!("../../src/cards/snake.rs");
    let snake_view_source = include_str!("../../src/cards/snake/view.rs");

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
        source.contains("working_snakes::synchronize_view"),
        "canvas view 应把局部 WorkingSnakeView 的业务状态同步委托给 working_snakes 模块"
    );
    assert!(
        working_snakes_source.contains("WorkingSnakeView::new"),
        "working_snakes 模块应只在业务状态变化时创建局部 WorkingSnakeView"
    );
    assert!(
        app_render_source.contains(".children(ordered_snake_views)"),
        "根视图应按 Canvas 节点顺序把 Working Snake Entity 挂为 cached CanvasSurface 的兄弟叠层"
    );
    let snake_index = app_render_source
        .find(".children(ordered_snake_views)")
        .expect("根视图必须挂载排序后的 Working Snake Entity");
    let overlays_index = app_render_source
        .find(".child(render_canvas_overlays(self, window, cx))")
        .expect("根视图必须在 Snake 后挂载 Canvas 交互叠层");
    let dispatch_index = app_render_source
        .find("crate::effects::dispatch_receipt::render(self, window)")
        .expect("根视图必须在 Snake 后挂载 dispatch receipt effect");
    assert!(
        snake_index < overlays_index && overlays_index < dispatch_index,
        "绘制顺序必须是 Snake → tooltip/queue overlays → dispatch effect，避免 Snake 穿透上层反馈"
    );
    assert!(
        overlays_source.contains("render_terminal_link_tooltip")
            && overlays_source.contains("render_agent_queue_panel"),
        "Canvas overlays 必须同时承载 terminal tooltip 与 Agent queue panel"
    );
    assert!(
        !source.contains("node_div = node_div.child(snake_view)"),
        "Working Snake 不得挂入 node_div/CanvasSurface 子树，否则局部 notify 会重建 Canvas"
    );
    assert!(
        !source.contains("render_working_snake_border"),
        "canvas view 不应逐帧直接构建 Snake path；该职责必须属于局部 Entity"
    );
    assert!(
        snake_view_source.contains("render_working_snake_border"),
        "WorkingSnakeView 应调用圆角路径 renderer 绘制局部 segment"
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

/// 卡片壁纸毛玻璃必须位于圆角外壳之后，不能作为外壳的矩形子元素覆盖圆角。
///
/// 数据构造（含关键数值推导）：
///   card radius = node_card_radius_px(1.0) = 22px。
///   GPUI 0.2.2 的 ContentMask 仅支持矩形；当 100% 不透明的壁纸采样成为
///   node_div 子元素时，会覆盖 22px 圆角范围内本应透明的像素。
///
/// 执行过程：
///   1. 读取 canvas/view.rs 的节点元素构造顺序。
///   2. 查找独立的 render_card_frosted_backdrop 元素入栈位置。
///   3. 查找 node_div 入栈位置，并检查毛玻璃不再作为 node_div.child(...)。
///
/// 预期结果：
///   - 正断言：毛玻璃先入栈、卡片外壳后入栈，外壳可用自身圆角决定可见边界。
///   - 负断言：node_div 子树不应直接挂载毛玻璃，避免矩形采样遮住四个圆角。
#[test]
fn test_frosted_backdrop_is_behind_the_rounded_card_shell() {
    let source = include_str!("../../src/canvas/view.rs");
    let backdrop_push = "node_elems.push(\n            app.render_card_frosted_backdrop";
    let shell_push = "node_elems.push(node_div.into_any_element());";
    let child_backdrop = ".child(app.render_card_frosted_backdrop";

    let backdrop_index = source
        .find(backdrop_push)
        .expect("毛玻璃必须作为独立画布元素入栈，供圆角外壳覆盖");
    let shell_index = source
        .find(shell_push)
        .expect("卡片外壳必须作为独立元素入栈，供圆角样式生效");

    assert!(
        backdrop_index < shell_index,
        "毛玻璃应先于圆角卡片外壳绘制，否则不透明采样会遮住四角"
    );
    assert!(
        !source.contains(child_backdrop),
        "毛玻璃不得继续作为 node_div 子元素；GPUI 的矩形裁剪会使四角变直"
    );
}

/// 卡片顶部不应再绘制独立的白色高光线，避免玻璃标题栏上方出现突兀横线。
///
/// 数据构造（含关键数值推导）：
///   原高光为 height = 1px，left/right = node_card_radius_px(1.0) = 22px。
///   它横跨标题栏顶部的中间区域，在浅色壁纸和 Ended 状态下会呈现为明显白线。
///
/// 执行过程：
///   1. 读取 Canvas 卡片的生产渲染源码。
///   2. 检查不再构造 edge_highlight 子元素。
///   3. 检查卡片子树不再挂载该独立高光层。
///
/// 预期结果：
///   - 正断言：卡片仍保留圆角外壳、标题分界和状态边框。
///   - 负断言：不得存在 1px 的 edge_highlight 横线或其 child 挂载点。
#[test]
fn test_card_shell_does_not_render_a_top_edge_highlight_line() {
    let source = include_str!("../../src/canvas/view.rs");

    assert!(
        source.contains(".rounded(px(node_radius))")
            && source.contains(".border_color(rgba(stroke))"),
        "移除顶部高光后，卡片仍必须保留圆角外壳和状态边框"
    );
    assert!(
        !source.contains("let edge_highlight = div()"),
        "卡片不得继续构造 1px edge_highlight 横线，否则标题顶部会出现白线"
    );
    assert!(
        !source.contains(".child(edge_highlight)"),
        "卡片不得继续挂载 edge_highlight 子元素，否则白线仍会被绘制"
    );
}
