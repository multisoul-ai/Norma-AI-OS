use super::*;
use std::time::{Duration, Instant};

/// 画布交互状态：默认应关闭滚轮/触控板画布滚动。
///
/// 数据构造（含关键数值推导）：
///   InteractionState::default() 来自 app 启动默认状态
///   canvas_scroll_enabled = false（用户要求画布模块默认关闭滚动）
///   space_held = false，panning = false（未按空格、未拖拽）
///
/// 执行过程：
///   1. 构造 InteractionState::default()
///   2. 读取 canvas_scroll_enabled 与其他初始交互标记
///
/// 预期结果：
///   - 正断言：canvas_scroll_enabled 为 false
///   - 负断言：space_held/panning 不应随默认滚动关闭而变成 true
#[test]
fn test_interaction_state_disables_canvas_scroll_by_default() {
    let state = InteractionState::default();

    assert!(
        !state.canvas_scroll_enabled,
        "画布滚动开关默认应关闭，用户需要通过顶部 toggle 主动开启"
    );
    assert!(
        !state.canvas_scroll_enabled,
        "默认状态不应继续开启画布滚动，否则进入画布后仍可直接滚动平移"
    );
    assert!(!state.space_held, "默认关闭画布滚动不应伪造空格按下状态");
    assert!(
        !state.panning,
        "默认关闭画布滚动不应进入鼠标拖拽 panning 状态"
    );
}

/// 画布滚动开启：wheel pixel delta 应平移 viewport.pan，且不再改变 zoom。
///
/// 数据构造（含关键数值推导）：
///   viewport.pan = (20,10)，zoom = 2.0
///   wheel delta = (12,-24)
///   画布滚动采用 pan -= delta，因此 new pan = (20-12, 10-(-24)) = (8,34)
///   zoom 保持 2.0，因为滚轮默认行为已从缩放改为平移
///
/// 执行过程：
///   1. 构造 viewport(pan=(20,10), zoom=2)
///   2. apply_canvas_wheel_scroll(enabled=true, delta=(12,-24))
///   3. 检查返回值、pan、zoom
///
/// 预期结果：
///   - 正断言：返回 true 且 pan 更新为 (8,34)
///   - 正断言：zoom 仍为 2.0
///   - 负断言：pan 不应停在原值，zoom 不应被改成旧滚轮缩放结果
#[test]
fn test_canvas_wheel_scroll_pans_viewport_without_zooming_when_enabled() {
    let mut viewport = canvas::Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 2.0,
    };
    let original_pan = viewport.pan;
    let original_zoom = viewport.zoom;

    let changed = apply_canvas_wheel_scroll(&mut viewport, Vec2::new(12.0, -24.0), true);

    assert!(
        changed,
        "非零 wheel delta 且画布滚动开启时应返回 true，表示 viewport 已平移"
    );
    assert_eq!(
        viewport.pan,
        Vec2::new(8.0, 34.0),
        "pan 应按 pan -= delta 从 (20,10) 更新到 (8,34)"
    );
    assert_eq!(
        viewport.zoom, original_zoom,
        "画布默认滚动应保持 zoom=2.0 不变，不再执行旧的滚轮缩放"
    );
    assert_ne!(
        viewport.pan, original_pan,
        "非零 wheel delta 不应让 pan 停在原值，否则画布看起来不能滚动"
    );
    assert_ne!(
        viewport.zoom,
        2.0 * (1.0 + 24.0 * 0.0015),
        "wheel delta 不应继续套用旧缩放公式改变 zoom"
    );
}

/// 画布滚动关闭：画布层 wheel 应被忽略，不改变 pan 或 zoom。
///
/// 数据构造（含关键数值推导）：
///   viewport.pan = (20,10)，zoom = 1.5
///   wheel delta = (8,16)
///   canvas_scroll_enabled = false → 不应用 pan -= delta
///   因此 pan 仍为 (20,10)，zoom 仍为 1.5
///
/// 执行过程：
///   1. 构造 viewport(pan=(20,10), zoom=1.5)
///   2. apply_canvas_wheel_scroll(enabled=false, delta=(8,16))
///   3. 检查返回值、pan、zoom
///
/// 预期结果：
///   - 正断言：返回 false 且 pan/zoom 保持原值
///   - 负断言：pan 不应变成 (12,-6)，zoom 不应改变
#[test]
fn test_canvas_wheel_scroll_does_not_move_viewport_when_disabled() {
    let mut viewport = canvas::Viewport {
        pan: Vec2::new(20.0, 10.0),
        zoom: 1.5,
    };
    let original_pan = viewport.pan;
    let original_zoom = viewport.zoom;

    let changed = apply_canvas_wheel_scroll(&mut viewport, Vec2::new(8.0, 16.0), false);

    assert!(
        !changed,
        "画布滚动关闭时应返回 false，表示画布层 wheel 被忽略"
    );
    assert_eq!(
        viewport.pan, original_pan,
        "画布滚动关闭时 pan 应保持 (20,10)，不能继续平移"
    );
    assert_eq!(
        viewport.zoom, original_zoom,
        "画布滚动关闭时 zoom 应保持 1.5，不能回退到旧的 wheel zoom"
    );
    assert_ne!(
        viewport.pan,
        Vec2::new(12.0, -6.0),
        "关闭后不应应用 pan -= delta，否则 toggle 没有真正锁住画布滚动"
    );
    assert_ne!(
        viewport.zoom,
        1.5 * (1.0 + 16.0 * 0.0015),
        "关闭后不应继续套用旧缩放公式改变 zoom"
    );
}

/// terminal 正文局部坐标：screen 坐标应扣除节点位置、标题栏和正文 padding。
///
/// 数据构造（含关键数值推导）：
///   node_top_left = (80,40)
///   screen = (100,120)
///   zoom = 2，因此标题栏高度 = TITLE_BAR_H(26) * 2 = 52
///   TERM_PADDING_PX = 8
///   local_x = 100 - 80 - 8 = 12
///   local_y = 120 - 40 - 52 - 8 = 20
///
/// 执行过程：
///   1. terminal_content_local_position(screen, node_top_left, zoom=2)
///   2. 读取返回的 Point<Pixels>
///
/// 预期结果：
///   - 正断言：local=(12px,20px)
///   - 负断言：local_y 不应忘记扣标题栏而变成 72px
#[test]
fn test_terminal_content_local_position_offsets_title_bar_and_padding() {
    let local =
        terminal_content_local_position(Vec2::new(100.0, 120.0), Vec2::new(80.0, 40.0), 2.0);

    assert_eq!(
        local.x,
        px(12.0),
        "terminal local x 应扣除 node.x 和 TERM_PADDING_PX，即 100-80-8=12"
    );
    assert_eq!(
        local.y,
        px(20.0),
        "terminal local y 应扣除 node.y、标题栏和 padding，即 120-40-52-8=20"
    );
    assert_ne!(
        local.y,
        px(72.0),
        "terminal local y 不应忘记扣 TITLE_BAR_H*zoom，否则选区会整体下移"
    );
}

/// 标题栏文本：长 task_title 应保持单行截断，不能换行撑高标题栏。
///
/// 数据构造（含关键数值推导）：
///   TITLE_BAR_H = 26px；标题栏高度固定为 TITLE_BAR_H * zoom
///   长 task_title 来源于用户输入，可能远超卡片宽度
///   标题文本应在可用宽度内 flex_1 + min_w(0)，再 truncate 成单行
///
/// 执行过程：
///   1. 读取 canvas/view.rs 的生产渲染源码
///   2. 定位 node-title 文本元素
///   3. 检查该元素声明单行截断所需的 GPUI 样式
///
/// 预期结果：
///   - 正断言：标题文本元素有 node-title id，便于防回归定位
///   - 正断言：标题文本元素有 flex_1、min_w(0)、overflow_hidden、truncate
///   - 负断言：标题不应继续以裸 .child(title) 直接挂在标题行 flex 内
#[test]
fn test_canvas_title_text_truncates_in_single_line() {
    let source = include_str!("../../src/canvas/view.rs");
    let title_marker = ".id((\"node-title\", id))";
    let bare_title_sequence = ".child(title)\n                        .when_some(task_badge";

    let title_start = source
        .find(title_marker)
        .expect("标题文本元素应声明 node-title id，便于锁定单行截断样式");
    let title_block_end = (title_start + 500).min(source.len());
    let title_block = &source[title_start..title_block_end];

    assert!(
        title_block.contains(".flex_1()"),
        "标题文本应 flex_1 占用剩余空间，避免和 badge/actions 争宽后换行"
    );
    assert!(
        title_block.contains(".min_w(px(0.0))"),
        "标题文本应 min_w(0)，否则 flex 子项可能拒绝收缩并挤压标题栏"
    );
    assert!(
        title_block.contains(".overflow_hidden()"),
        "标题文本应隐藏溢出内容，不能让长 task_title 画到第二行"
    );
    assert!(
        title_block.contains(".truncate()"),
        "标题文本应用 GPUI truncate 单行截断，长用户输入不应换行"
    );
    assert!(
        !source.contains(bare_title_sequence),
        "标题不应继续以裸 .child(title) 挂在标题行 flex 内，否则长文本会按默认文本布局换行"
    );
}

/// 拖拽/平移渲染节流：高频 mouse move 不应每个事件都触发整画布 repaint。
///
/// 数据构造（含关键数值推导）：
///   frame interval = 16ms（约 60fps）
///   first frame last=None → 应通知，保证拖拽立即有反馈
///   too_soon = first + 4ms，4 < 16 → 不应通知
///   next_frame = first + 16ms，16 >= 16 → 应通知
///
/// 执行过程：
///   1. should_notify_motion_frame(None, first)
///   2. should_notify_motion_frame(Some(first), first+4ms)
///   3. should_notify_motion_frame(Some(first), first+16ms)
///
/// 预期结果：
///   - 正断言：首帧和满 16ms 后返回 true
///   - 负断言：4ms 内返回 false，避免 mousemove 频率直接驱动整画布重绘
#[test]
fn test_motion_frame_notify_is_throttled_to_frame_interval() {
    let first = Instant::now();
    let too_soon = first + Duration::from_millis(4);
    let next_frame = first + Duration::from_millis(16);

    let initial = should_notify_motion_frame(None, first);
    let suppressed = should_notify_motion_frame(Some(first), too_soon);
    let allowed = should_notify_motion_frame(Some(first), next_frame);

    assert!(initial, "拖拽首帧应立即 notify，避免拖动看起来没有响应");
    assert!(
        !suppressed,
        "4ms < 16ms 的高频 mouse move 不应触发整画布 repaint"
    );
    assert!(allowed, "达到 16ms frame interval 后应允许下一次 repaint");
}

/// render_canvas 应在 clone/chrome/body 前完成 viewport culling，避免离屏卡片参与重型渲染路径。
///
/// 数据构造（含关键数值推导）：
///   source = canvas/view.rs 当前源码
///   cull marker = "viewport culling"
///   heavy clone marker = "let node = app.runtime.document.node(id).unwrap().clone()"
///   目标顺序：cull marker 的位置必须小于 heavy clone marker
///
/// 执行过程：
///   1. 读取 canvas/view.rs
///   2. 找到 culling 代码与 node clone 代码位置
///   3. 比较两者顺序
///
/// 预期结果：
///   - 正断言：culling 在 clone/chrome/body 之前
///   - 负断言：clone 不应继续早于 culling，否则离屏节点仍会消耗拖拽帧预算
#[test]
fn test_canvas_render_culls_before_heavy_node_clone() {
    let source = include_str!("../../src/canvas/view.rs");
    let cull_pos = source
        .find("viewport culling")
        .expect("render_canvas 应保留 viewport culling 注释作为性能护栏");
    let clone_pos = source
        .find("let node = app.runtime.document.node(id).unwrap().clone()")
        .expect("render_canvas 应只在必要时 clone node");

    assert!(
        cull_pos < clone_pos,
        "viewport culling 应早于 heavy node clone/chrome/body，cull_pos={cull_pos}, clone_pos={clone_pos}"
    );
    let clone_before_cull = clone_pos < cull_pos;
    assert!(
        !clone_before_cull,
        "node clone 不应早于 culling，否则离屏卡片仍参与拖拽重绘热路径"
    );
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

/// 画布鼠标按下应把 GPUI root focus 拉回应用，保证 IME commit 能进入聚焦节点。
///
/// 数据构造（含关键数值推导）：
///   Left = 正文/标题栏点击，会改变 runtime focus 或 selection
///   Middle = 画布 pan，也会让用户继续在画布中操作
///   Right = 当前没有画布输入语义，不应抢焦点
///
/// 执行过程：
///   1. should_focus_app_root_for_mouse_down(Left)
///   2. should_focus_app_root_for_mouse_down(Middle)
///   3. should_focus_app_root_for_mouse_down(Right)
///
/// 预期结果：
///   - 正断言：Left/Middle 返回 true，点击恢复后卡片时会重新激活 root FocusHandle
///   - 负断言：Right 返回 false，避免无语义右键抢走系统上下文焦点
#[test]
fn test_canvas_mouse_down_requests_root_focus_for_primary_canvas_buttons() {
    let left = should_focus_app_root_for_mouse_down(MouseButton::Left);
    let middle = should_focus_app_root_for_mouse_down(MouseButton::Middle);
    let right = should_focus_app_root_for_mouse_down(MouseButton::Right);

    assert!(
        left,
        "左键点击卡片正文/标题栏应请求 GPUI root focus，否则 IME 文本不会提交到节点"
    );
    assert!(
        middle,
        "中键 pan 也属于画布交互，应请求 GPUI root focus 保持后续键盘事件在应用内"
    );
    assert!(
        !right,
        "右键当前没有画布输入语义，不应无条件抢占 GPUI root focus"
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
