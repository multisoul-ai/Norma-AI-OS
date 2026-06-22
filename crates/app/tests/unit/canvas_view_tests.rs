use super::*;
use canvas::{CanvasNodeKind, Document, PreviewSpec, PreviewType, RectNode, Selection, Viewport};

fn preview_kind() -> CanvasNodeKind {
    CanvasNodeKind::Preview(PreviewSpec {
        path: "/tmp/report.md".to_string(),
        explicit_type: Some(PreviewType::Markdown),
    })
}

fn selected_selection(id: NodeId) -> Selection {
    let mut selection = Selection::new();
    selection.select_only(id);
    selection
}

fn screen_point(viewport: Viewport, canvas_x: f32, canvas_y: f32) -> Vec2 {
    viewport.canvas_to_screen(Vec2::new(canvas_x, canvas_y))
}

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

/// 已选 Preview 正文区命中时，滚轮应归 Preview 内部滚动而不是缩放画布。
///
/// 数据构造（含关键数值推导）：
///   preview bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   node_top_screen_y = 50*2 + 10 = 110
///   title_bottom_screen_y = 110 + TITLE_BAR_H(26)*2 = 162
///   正文命中点 canvas=(130,80) → screen=(130*2+20, 80*2+10)=(280,170)
///   170 > 162，因此命中正文区而非标题栏
///
/// 执行过程：
///   1. add Preview 节点并 select_only(preview)
///   2. wheel_scroll_owner_for_point 用 screen=(280,170) 做命中归属判断
///
/// 预期结果：
///   - 正断言：归属为 Card(preview)，Preview 内部滚动可消费 wheel
///   - 负断言：不应归属 Canvas，否则滚轮会继续缩放画布
#[test]
fn test_selected_preview_body_scroll_belongs_to_card() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let preview = doc.add_node(preview_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = selected_selection(preview);
    let point = screen_point(doc.viewport, 130.0, 80.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::Card(preview),
        "已选 Preview 正文区滚轮应归 Preview card，避免画布抢走向下滚动"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::Canvas,
        "已选 Preview 正文区滚轮不应归 Canvas，否则会触发 zoom 而不是内容滚动"
    );
}

/// 已选 Preview 标题栏命中时，滚轮仍应归画布而不是 Preview 正文滚动。
///
/// 数据构造（含关键数值推导）：
///   preview bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   node_top_screen_y = 50*2 + 10 = 110
///   title_bottom_screen_y = 110 + 26*2 = 162
///   标题栏命中点 canvas=(130,60) → screen=(280,130)
///   130 <= 162，因此命中标题栏区域
///
/// 执行过程：
///   1. add Preview 节点并 select_only(preview)
///   2. 对标题栏 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 Canvas，标题栏保留画布/拖拽语义
///   - 负断言：不应归属 Card(preview)，否则标题栏也会吞掉画布滚轮
#[test]
fn test_selected_preview_title_bar_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let preview = doc.add_node(preview_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = selected_selection(preview);
    let point = screen_point(doc.viewport, 130.0, 60.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::Canvas,
        "已选 Preview 标题栏滚轮应归 Canvas，标题栏不属于内部滚动内容区"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::Card(preview),
        "已选 Preview 标题栏不应归 Preview card，否则标题栏会错误吞掉画布滚轮"
    );
}

/// 未选 Preview 正文区命中时，滚轮应保持画布 zoom 语义。
///
/// 数据构造（含关键数值推导）：
///   preview bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   正文命中点 canvas=(130,80) → screen=(280,170)，位于标题栏下方
///   selection 为空，因此 Preview 未被用户选中
///
/// 执行过程：
///   1. add Preview 节点但不 select
///   2. 对正文 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 Canvas，未选卡片不抢全局滚轮
///   - 负断言：不应归属 Card(preview)，否则 hover 到未选卡片也会改变滚轮语义
#[test]
fn test_unselected_preview_body_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let preview = doc.add_node(preview_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = Selection::new();
    let point = screen_point(doc.viewport, 130.0, 80.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::Canvas,
        "未选 Preview 正文区滚轮应继续归 Canvas，只有选中态才切换到内部滚动"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::Card(preview),
        "未选 Preview 不应接管滚轮，否则用户路过卡片时会失去画布 zoom"
    );
}

/// 已选非 Preview 卡片正文区命中时，滚轮应仍归画布。
///
/// 数据构造（含关键数值推导）：
///   rect bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   正文命中点 canvas=(130,80) → screen=(280,170)
///   Rect card 的 captures_wheel_scroll 默认为 false
///
/// 执行过程：
///   1. add Rect 节点并 select_only(rect)
///   2. 对正文 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 Canvas，非滚动卡片不改变画布滚轮
///   - 负断言：不应归属 Card(rect)，否则普通卡片会吞掉 zoom
#[test]
fn test_selected_non_preview_body_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let rect = doc.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(100.0, 50.0, 300.0, 220.0),
    );
    let selection = selected_selection(rect);
    let point = screen_point(doc.viewport, 130.0, 80.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::Canvas,
        "已选 Rect 正文区滚轮应归 Canvas，因为 Rect card 未声明内部滚动"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::Card(rect),
        "已选 Rect 不应接管滚轮，否则非滚动卡片会阻断画布 zoom"
    );
}

/// 已选 Preview 但指针在卡片外时，滚轮应继续缩放画布。
///
/// 数据构造（含关键数值推导）：
///   preview bounds = origin(100,50) size(300x220)
///   viewport.pan=(20,10), zoom=2
///   外部点 canvas=(10,10) → screen=(40,30)
///   preview 覆盖 x∈[100,400), y∈[50,270)，canvas=(10,10) 不在范围内
///
/// 执行过程：
///   1. add Preview 节点并 select_only(preview)
///   2. 对卡片外 screen point 做滚轮归属判断
///
/// 预期结果：
///   - 正断言：归属为 Canvas，卡片外保持全局画布滚轮
///   - 负断言：不应归属 Card(preview)，否则选中后会全局吞滚轮
#[test]
fn test_selected_preview_outside_scroll_belongs_to_canvas() {
    let mut doc = Document::new();
    doc.viewport = Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let preview = doc.add_node(preview_kind(), CanvasRect::new(100.0, 50.0, 300.0, 220.0));
    let selection = selected_selection(preview);
    let point = screen_point(doc.viewport, 10.0, 10.0);

    let owner = wheel_scroll_owner_for_point(doc.nodes(), &selection, point, doc.viewport);

    assert_eq!(
        owner,
        WheelScrollOwner::Canvas,
        "已选 Preview 外部滚轮应归 Canvas，方便用户继续缩放/导航画布"
    );
    assert_ne!(
        owner,
        WheelScrollOwner::Card(preview),
        "已选 Preview 不应全局接管滚轮，否则必须取消选择才能 zoom"
    );
}
