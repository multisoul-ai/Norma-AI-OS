use super::*;

/// 可聚焦卡片正文单击应进入节点聚焦，避免用户选中节点后无法输入。
///
/// 数据构造（含关键数值推导）：
///   node id = 7
///   click_count = 1（单击，不是双击）
///   is_focusable = true（命中可接收键盘输入的 live node）
///   is_title_bar = false（点击正文区，标题栏高度不参与拖拽）
///
/// 执行过程：
///   1. click_event_for_node(Some(7), true, false, 1) 生成输入事件
///   2. route(CanvasSelect, event) 按 runtime 纯路由解析
///
/// 预期结果：
///   - 正断言：正文单击被转换为 FocusNode(7)
///   - 负断言：正文单击不应只是 SelectNode(7)，否则后续按键会被画布忽略
#[test]
fn test_focusable_body_single_click_focuses_node() {
    let event = click_event_for_node(Some(7), true, false, 1);
    let route = route(&InputMode::CanvasSelect, &event);

    assert_eq!(
        route,
        Route::FocusNode(7),
        "可聚焦正文单击应直接聚焦节点 7，让键盘输入有接收者"
    );
    assert_ne!(
        route,
        Route::SelectNode(7),
        "可聚焦正文单击不应只停留在选中态，否则用户会看到橙色边框但无法输入"
    );
}

/// 可聚焦卡片标题栏单击仍只选中节点并保留拖拽语义。
///
/// 数据构造（含关键数值推导）：
///   node id = 7
///   click_count = 1（单击）
///   is_focusable = true（命中可接收键盘输入的 live node）
///   is_title_bar = true（screen_y 落在 node_top + TITLE_BAR_H * zoom 内）
///
/// 执行过程：
///   1. click_event_for_node(Some(7), true, true, 1) 生成输入事件
///   2. route(CanvasSelect, event) 按 runtime 纯路由解析
///
/// 预期结果：
///   - 正断言：标题栏单击保持 SelectNode(7)，供拖拽逻辑继续执行
///   - 负断言：标题栏单击不应进入 FocusNode(7)，否则点击标题栏会吞掉拖拽入口
#[test]
fn test_terminal_title_single_click_selects_for_drag() {
    let event = click_event_for_node(Some(7), true, true, 1);
    let route = route(&InputMode::CanvasSelect, &event);

    assert_eq!(
        route,
        Route::SelectNode(7),
        "终端标题栏单击应保持选中节点，标题栏拖拽才能继续工作"
    );
    assert_ne!(
        route,
        Route::FocusNode(7),
        "可聚焦卡片标题栏单击不应直接聚焦节点，否则拖拽标题栏会变成输入聚焦"
    );
}

/// 标题栏命中计算随 zoom 缩放，避免高缩放下正文被误判成标题栏。
///
/// 数据构造（含关键数值推导）：
///   node_top_screen_y = 100
///   TITLE_BAR_H = 26
///   zoom = 2
///   title_bottom = 100 + 26 * 2 = 152
///   y=151.5 在标题栏内，y=153 已进入正文
///
/// 执行过程：
///   1. is_title_bar_hit(151.5, 100, 2) → true
///   2. is_title_bar_hit(153, 100, 2) → false
///
/// 预期结果：
///   - 正断言：151.5 被识别为标题栏
///   - 正断言：153 被识别为正文
///   - 负断言：正文 y=153 不应仍被当作标题栏
#[test]
fn test_title_bar_hit_scales_with_zoom() {
    let in_title = is_title_bar_hit(151.5, 100.0, 2.0);
    let in_body = is_title_bar_hit(153.0, 100.0, 2.0);

    assert!(in_title, "y=151.5 <= 100 + 26 * 2，应被识别为标题栏命中");
    assert!(!in_body, "y=153 > 100 + 26 * 2，应被识别为正文命中");
    assert_ne!(
        in_body, true,
        "正文区域不应被误判为标题栏，否则单击正文不会聚焦节点"
    );
}

/// 卡片圆角 token 应统一用于外壳、标题栏和顶部高亮线。
///
/// 数据构造（含关键数值推导）：
///   base radius = 16px
///   zoom=0.25 → raw radius = 16 * 0.25 = 4px，clamp 到最小 8px
///   zoom=1.00 → raw radius = 16 * 1.00 = 16px，保持 16px
///   zoom=2.00 → raw radius = 16 * 2.00 = 32px，clamp 到最大 22px
///   top highlight inset = card radius，避免 1px 高亮线画进圆角区域
///
/// 执行过程：
///   1. 分别计算 zoom=0.25、1.0、2.0 的卡片圆角
///   2. 计算 zoom=1.0 的标题栏圆角与顶部高亮线缩进
///
/// 预期结果：
///   - 正断言：卡片圆角随 zoom 缩放并保持在 8..=22px
///   - 正断言：标题栏圆角复用卡片圆角，避免顶部背景填平圆角
///   - 正断言：顶部高亮线从 radius 内侧开始，避开左右顶角
///   - 负断言：高亮线缩进不应为 0，否则会再次画出直角顶边
#[test]
fn test_node_card_radius_tokens_keep_children_inside_rounded_shell() {
    let zoomed_out = node_card_radius_px(0.25);
    let normal = node_card_radius_px(1.0);
    let zoomed_in = node_card_radius_px(2.0);
    let title_radius = node_title_radius_px(1.0);
    let highlight_inset = node_edge_highlight_inset_px(1.0);

    assert_eq!(
        zoomed_out, 8.0,
        "zoom=0.25 时 raw=4px，应 clamp 到 8px 最小卡片圆角"
    );
    assert_eq!(normal, 16.0, "zoom=1.0 时 raw=16px，应保持默认卡片圆角");
    assert_eq!(
        zoomed_in, 22.0,
        "zoom=2.0 时 raw=32px，应 clamp 到 22px 最大卡片圆角"
    );
    assert_eq!(
        title_radius, normal,
        "标题栏顶部圆角必须复用卡片圆角，否则矩形标题栏会填平顶角"
    );
    assert_eq!(
        highlight_inset, normal,
        "顶部高亮线应从卡片圆角内侧开始，避免覆盖圆角弧线"
    );
    assert_ne!(
        highlight_inset, 0.0,
        "顶部高亮线缩进不能为 0，否则 w_full 的 1px 线会重新制造直角"
    );
}
