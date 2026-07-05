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
///   source = app/browser.rs；create_webview 创建 wry::WebView；
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
    let source = include_str!("../../src/app/browser.rs");

    assert!(
        source.contains("round_native_webview_corners(&webview"),
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
        !source.contains("GPUI overflow_hidden clips native WebView"),
        "browser implementation must not rely on GPUI clipping for a native child WebView"
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
