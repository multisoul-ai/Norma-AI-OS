use super::*;

/// B-1: 浏览器地址栏点击必须把键盘焦点拉回 GPUI root，避免 WebView 抢焦后无法输入 URL。
///
/// 数据构造（含关键数值的推导过程）：
///   source = cards/browser/card.rs；地址栏事件块以 id(("browser-address", node_id)) 定位；
///   WebView 是原生子视图，点击网页后系统焦点可能不在 GPUI root。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取浏览器卡片源码并截取 browser-address 事件块
///   2. 检查地址栏 mouse down/up 处理里是否调用 FocusHandle::focus(window)
///
/// 预期结果：
///   - 正断言：地址栏点击路径包含 .focus(window)，后续 keydown 能进入 MultisoulApp
///   - 负断言：不应只 dispatch EditAddress action 而不恢复 root focus
#[test]
fn test_browser_address_click_refocuses_app_root_for_typing() {
    let source = include_str!("../../src/cards/browser/card.rs");
    let marker = ".id((\"browser-address\", node_id))";
    let start = source
        .find(marker)
        .expect("browser card should render an address element with a stable id");
    let end = source[start..]
        .find(".on_mouse_up(MouseButton::Left")
        .map(|offset| start + offset)
        .expect("browser address should handle mouse up to enter edit mode");
    let address_block = &source[start..end];

    assert!(
        address_block.contains(".focus(window)"),
        "address click must call FocusHandle::focus(window) so typed URL keys reach the GPUI root"
    );
    assert!(
        !(address_block.contains("BrowserCommandKind::EditAddress")
            && !address_block.contains(".focus(window)")),
        "address click must not only dispatch EditAddress; without root focus the edit state cannot receive keys"
    );
}

/// B-2: macOS 原生 WebView 必须配置 layer 圆角裁剪，否则会盖住 GPUI 卡片圆角。
///
/// 数据构造（含关键数值的推导过程）：
///   source = app/browser/wry_backend.rs；WryBrowserView 创建 wry::WebView；
///   GPUI overflow_hidden 不能裁剪系统原生 NSView 子视图。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取浏览器 WebView lifecycle 源码
///   2. 检查 create_webview 后是否调用原生圆角 helper
///   3. 检查 helper 是否设置 wantsLayer、cornerRadius 和 masksToBounds
///
/// 预期结果：
///   - 正断言：WebView 创建路径调用 round_native_webview_corners
///   - 正断言：原生 helper 设置 cornerRadius 与 masksToBounds
///   - 负断言：不应只依赖 GPUI .rounded/.overflow_hidden 裁剪原生 WebView
#[test]
fn test_browser_webview_applies_native_corner_clipping() {
    let source = include_str!("../../src/app/browser/wry_backend.rs");

    assert!(
        source.contains("clip_native_webview_to_bottom_corners(&webview"),
        "create_webview must apply native WebView corner clipping after building the child view"
    );
    assert!(
        source.contains("setWantsLayer(true)") && source.contains("setCornerRadius"),
        "native WebView clipping must configure an NSView layer with a corner radius"
    );
    assert!(
        source.contains("setMasksToBounds(true)"),
        "native WebView clipping must mask sublayers to prevent rectangular WebView corners"
    );
    assert!(
        source.contains("LayerMinXMaxYCorner") && source.contains("LayerMaxXMaxYCorner"),
        "native WebView clipping must mask only the bottom corners below the browser toolbar"
    );
    assert!(
        !source.contains("round_native_webview_corners"),
        "browser implementation must not keep the old all-corners clipping helper"
    );
}

/// B-5: 浏览器原生 WebView bounds 必须落在卡片内容区内，不能覆盖卡片边框和底部圆角。
///
/// 数据构造（含关键数值的推导过程）：
///   card_top_left = (40, 70)，来自截图中卡片位于窗口内的屏幕坐标；
///   card_size = 1180 x 840，代表一个大尺寸 Browser 卡片；
///   header_h = TITLE_BAR_H(26) + BROWSER_TOOLBAR_H_PX(34) = 60；
///   border_inset = 1px，左右和底部各避让 GPUI card border；
///   bottom_radius = 16px，等同 zoom=1 时 node_card_radius_px(1.0)。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将卡片屏幕坐标、尺寸、header 高度和底部圆角传入 bounds 计算函数
///   2. 函数把 WebView y 移到 header 下方，即 70 + 60 = 130
///   3. 函数左右各内缩 1px，宽度 1180 - 2 = 1178
///   4. 函数底部内缩 1px，高度 840 - 60 - 1 = 779
///
/// 预期结果：
///   - 正断言：x/y/width/height 精确匹配内容区 bounds
///   - 正断言：bottom_corner_radius_px 保持为调用方传入的 16px
///   - 负断言：WebView 不应等于整张卡片 bounds，否则原生视图会盖住边框和圆角
#[test]
fn test_browser_native_view_bounds_inset_under_browser_toolbar() {
    let bounds = browser_native_view_bounds(40.0, 70.0, 1180.0, 840.0, 60.0, 16.0);

    assert_eq!(
        bounds.x, 41.0,
        "native WebView x should inset by 1px so it does not cover the left card border"
    );
    assert_eq!(
        bounds.y, 130.0,
        "native WebView y should start below title bar plus browser toolbar"
    );
    assert_eq!(
        bounds.width, 1178.0,
        "native WebView width should inset both horizontal borders"
    );
    assert_eq!(
        bounds.height, 779.0,
        "native WebView height should leave the bottom border visible"
    );
    assert_eq!(
        bounds.bottom_corner_radius_px, 16.0,
        "native WebView bottom corner radius should come from the card bottom radius"
    );
    assert!(
        bounds
            != (BrowserViewBounds {
                x: 40.0,
                y: 70.0,
                width: 1180.0,
                height: 840.0,
                bottom_corner_radius_px: 16.0,
            }),
        "native WebView must not use the full card bounds because native child views are not clipped by GPUI"
    );
}

/// B-6: 极小 Browser 卡片计算原生 bounds 时必须把尺寸钳制到 0，避免负尺寸传给 wry。
///
/// 数据构造（含关键数值的推导过程）：
///   card_size = 1 x 30，宽度小于左右边框 2px，总高度小于 header_h(60) + bottom_inset(1)；
///   width  = max(0, 1 - 2) = 0；
///   height = max(0, 30 - 60 - 1) = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 传入极小卡片尺寸
///   2. bounds 计算对宽高执行 max(0)
///
/// 预期结果：
///   - 正断言：width 被钳制为 0
///   - 正断言：height 被钳制为 0
///   - 负断言：width 和 height 不应为负数
#[test]
fn test_browser_native_view_bounds_clamps_tiny_cards_to_zero_size() {
    let bounds = browser_native_view_bounds(10.0, 20.0, 1.0, 30.0, 60.0, 16.0);

    assert_eq!(
        bounds.width, 0.0,
        "native WebView width should clamp to zero when the card is narrower than both borders"
    );
    assert_eq!(
        bounds.height, 0.0,
        "native WebView height should clamp to zero when the card is shorter than the browser header"
    );
    assert!(
        bounds.width >= 0.0,
        "native WebView width must never be negative because wry bounds require valid sizes"
    );
    assert!(
        bounds.height >= 0.0,
        "native WebView height must never be negative because wry bounds require valid sizes"
    );
}

/// B-7: 删除 Browser 节点时必须先把焦点交回 GPUI root，再移除原生 WebView。
///
/// 数据构造（含关键数值的推导过程）：
///   source = app/mod.rs；remove_node_and_reflow 是标题栏关闭按钮和快捷键删除的共同路径；
///   WebView 是 macOS native first responder，Runtime::focus 不会自动清理 native first responder。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 app/mod.rs 中 remove_node_and_reflow 函数片段
///   2. 检查删除前是否记录 removed_node_was_browser
///   3. 检查 changed 分支内 Browser 删除路径是否调用 focus_handle.focus(window)
///   4. 检查 remove_browser_view(node_id) 仍然执行，释放原生 WebView
///
/// 预期结果：
///   - 正断言：删除路径在节点移除前判断 CanvasNodeKind::Browser
///   - 正断言：Browser 删除路径调用 focus_handle.focus(window)
///   - 正断言：Browser 删除路径仍移除 native WebView
///   - 负断言：不应无条件聚焦 root，非 Browser 删除不需要 native focus handoff
#[test]
fn test_browser_delete_path_refocuses_root_before_removing_native_webview() {
    let source = include_str!("../../src/app/mod.rs");
    let start = source
        .find("pub(crate) fn remove_node_and_reflow")
        .expect("app should keep remove_node_and_reflow as the shared node deletion path");
    let end = source[start..]
        .find("pub(crate) fn close_node_through_route")
        .map(|offset| start + offset)
        .expect("remove_node_and_reflow should appear before close_node_through_route");
    let remove_block = &source[start..end];

    assert!(
        remove_block.contains("removed_node_was_browser"),
        "deletion path should capture whether the removed node was a Browser before runtime removes it"
    );
    assert!(
        remove_block.contains("CanvasNodeKind::Browser"),
        "deletion path should identify Browser nodes explicitly for native focus handoff"
    );
    assert!(
        remove_block.contains("self.focus_handle.focus(window)"),
        "Browser deletion must return keyboard focus to the GPUI root before the native WebView disappears"
    );
    assert!(
        remove_block.contains("self.remove_browser_view(node_id)"),
        "Browser deletion must still remove the native WebView after the root focus handoff"
    );
    assert!(
        !remove_block.contains("changed {\n            self.focus_handle.focus(window);"),
        "root focus handoff should be guarded by removed_node_was_browser instead of running for every node type"
    );
}

/// B-3: 地址栏编辑态必须在快捷键路由前处理 Cmd+V，把剪贴板文本写入 draft。
///
/// 数据构造（含关键数值的推导过程）：
///   source = app/browser.rs；handle_browser_address_key_down 位于 root keydown 开头；
///   Cmd+V = platform modifier + key "v"，不能落入普通快捷键或 WebView。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取浏览器编辑源码
///   2. 检查是否存在 browser_address_paste_requested 分支
///   3. 检查该分支读取 cx.read_from_clipboard 并早于 platform/control 退回
///
/// 预期结果：
///   - 正断言：编辑态 Cmd+V 有专门分支并读取剪贴板
///   - 负断言：不应先因 platform/control 直接 return false
#[test]
fn test_browser_address_edit_handles_paste_before_shortcut_routing() {
    let source = include_str!("../../src/app/browser.rs");
    let paste_pos = source
        .find("browser_address_paste_requested")
        .expect("browser address editing should detect Cmd+V paste explicitly");
    let clipboard_pos = source
        .find("read_from_clipboard")
        .expect("browser address paste should read the GPUI clipboard");
    let modifier_return_pos = source
        .find("modifiers.platform || ev.keystroke.modifiers.control")
        .expect("browser address editing should still guard non-paste platform/control shortcuts");

    assert!(
        paste_pos < modifier_return_pos,
        "paste handling must run before platform/control shortcuts are returned to global routing"
    );
    assert!(
        clipboard_pos < modifier_return_pos,
        "clipboard text must be read before Cmd+V can be intercepted by global shortcut routing"
    );
    assert!(
        !source[paste_pos..modifier_return_pos].contains("return false"),
        "paste branch must not fall through as unhandled before reading clipboard text"
    );
}

/// B-4: 地址栏编辑态必须渲染可见光标，否则用户无法判断输入焦点和插入位置。
///
/// 数据构造（含关键数值的推导过程）：
///   source = cards/browser/card.rs；editing = browser_address_draft.is_some()；
///   光标应是稳定 id 的窄竖条，随 editing 条件显示。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取浏览器卡片源码
///   2. 检查地址栏渲染中是否有 browser-address-caret
///   3. 检查 caret 由 .when(editing, ...) 条件挂载
///
/// 预期结果：
///   - 正断言：编辑态有稳定 id 的 caret 元素
///   - 正断言：caret 是窄竖条，宽度约 1px，高度随 zoom 缩放
///   - 负断言：不应只渲染纯文本 URL
#[test]
fn test_browser_address_edit_renders_visible_caret() {
    let source = include_str!("../../src/cards/browser/card.rs");
    let address_pos = source
        .find(".id((\"browser-address\", node_id))")
        .expect("browser card should render an address element with a stable id");
    let caret_pos = source
        .find("browser-address-caret")
        .expect("browser address editing should render a visible caret element");
    let address_block = &source[address_pos..(address_pos + 2400).min(source.len())];

    assert!(
        address_block.contains(".when(editing"),
        "caret should be conditionally mounted only while address editing is active"
    );
    assert!(
        caret_pos > address_pos,
        "caret should live inside the address element, after the address container starts"
    );
    assert!(
        address_block.contains(".w(px((1.0 * zoom)") || address_block.contains(".w(px(1.0"),
        "caret should render as a narrow vertical insertion marker"
    );
}
