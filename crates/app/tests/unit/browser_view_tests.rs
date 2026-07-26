use super::*;

/// B-1: 浏览器地址栏必须实现系统文本服务，不能手工拼接按键字符。
///
/// 数据构造（含关键数值的推导过程）：
///   source = cards/browser/card.rs 与 app/browser.rs；地址栏 id 为
///   ("browser-address", node_id)；`BrowserAddressInput` 直接实现
///   `EntityInputHandler`，能接收 IME、Delete、复制与剪贴板命令；
///   旧实现仅从 KeyDownEvent 提取 ASCII 图形字符，无法表达输入法组合文本。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取浏览器卡片与编辑状态源码
///   2. 检查编辑态是否把 `BrowserAddressInput` 实体挂到地址栏
///   3. 检查旧的全局 KeyDown 手工字符解析是否已移除
///
/// 预期结果：
///   - 正断言：编辑态渲染 `BrowserAddressInput`，让系统文本服务接管输入
///   - 正断言：地址栏仍保留稳定 id，供点击与 e2e 定位
///   - 负断言：不能保留 `browser_input_char` 这类 ASCII-only 手工解析
#[test]
fn test_browser_address_uses_native_text_input_for_ime_clipboard_and_deletion() {
    let card_source = include_str!("../../src/cards/browser/card.rs");
    let state_source = include_str!("../../src/app/browser.rs");
    let input_source = include_str!("../../src/app/browser/address_input.rs");
    let marker = ".id((\"browser-address\", node_id))";
    let start = card_source
        .find(marker)
        .expect("browser card should render an address element with a stable id");
    let address_block = &card_source[start..(start + 2600).min(card_source.len())];

    assert!(
        address_block.contains(".child(browser_address_input)"),
        "地址栏编辑态必须挂载 BrowserAddressInput 实体，才能接入 macOS IME、选择、复制和删除"
    );
    assert!(
        input_source.contains("impl EntityInputHandler for BrowserAddressInput"),
        "BrowserAddressInput 必须实现 EntityInputHandler，才能把输入法组合文本交给 GPUI 平台文本服务"
    );
    assert!(
        input_source.contains("replace_and_mark_text_in_range"),
        "BrowserAddressInput 必须处理标记文本，才能正确显示输入法候选组合"
    );
    assert!(
        address_block.contains(marker),
        "浏览器地址栏必须保留稳定 id，点击与 e2e 才能定位到真实输入区域"
    );
    assert!(
        !state_source.contains("fn browser_input_char"),
        "地址栏不能继续从 KeyDownEvent 手工提取 ASCII 字符，否则中文输入法和编辑命令会被绕过"
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
///   header_h = TITLE_BAR_H(32) + BROWSER_TOOLBAR_H_PX(34) = 66；
///   border_inset = 2px，四边均避让 selected 状态的 GPUI `border_2`；
///   bottom_radius = 22px，等同 zoom=1 时 node_card_radius_px(1.0)。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将卡片屏幕坐标、尺寸、header 高度和底部圆角传入 bounds 计算函数
///   2. 函数把 WebView y 移到 header 和顶部边框下方，即 70 + 66 + 2 = 138
///   3. 函数左右各内缩 2px，宽度 1180 - 4 = 1176
///   4. 函数上下各避让 2px，高度 840 - 66 - 2 - 2 = 770
///
/// 预期结果：
///   - 正断言：x/y/width/height 精确匹配内容区 bounds
///   - 正断言：bottom_corner_radius_px 保持为调用方传入的 22px
///   - 负断言：WebView 不应等于整张卡片 bounds，否则原生视图会盖住边框和圆角
#[test]
fn test_browser_native_view_bounds_inset_under_browser_toolbar() {
    let bounds = browser_native_view_bounds(40.0, 70.0, 1180.0, 840.0, 66.0, 22.0);

    assert_eq!(
        bounds.x, 42.0,
        "native WebView x should inset by 2px so it does not cover the selected card's 2px left border"
    );
    assert_eq!(
        bounds.y, 138.0,
        "native WebView y should start below the browser header and selected card's 2px top border"
    );
    assert_eq!(
        bounds.width, 1176.0,
        "native WebView width should preserve both 2px horizontal selected borders"
    );
    assert_eq!(
        bounds.height, 770.0,
        "native WebView height should preserve both 2px vertical selected borders"
    );
    assert_eq!(
        bounds.bottom_corner_radius_px, 22.0,
        "native WebView bottom corner radius should come from the card bottom radius"
    );
    assert!(
        bounds
            != (BrowserViewBounds {
                x: 40.0,
                y: 70.0,
                width: 1180.0,
                height: 840.0,
                bottom_corner_radius_px: 22.0,
            }),
        "native WebView must not use the full card bounds because native child views are not clipped by GPUI"
    );
}

/// B-6: 极小 Browser 卡片计算原生 bounds 时必须把尺寸钳制到 0，避免负尺寸传给 wry。
///
/// 数据构造（含关键数值的推导过程）：
///   card_size = 1 x 30，宽度小于左右边框 2px，总高度小于 header_h(66) + bottom_inset(1)；
///   width  = max(0, 1 - 2) = 0；
///   height = max(0, 30 - 66 - 1) = 0。
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
    let bounds = browser_native_view_bounds(10.0, 20.0, 1.0, 30.0, 66.0, 22.0);

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

/// B-7: 修改地址或删除 Browser 节点时必须先释放原生 WebView 焦点，再交回 GPUI。
///
/// 数据构造（含关键数值的推导过程）：
///   source = app/node_removal.rs；remove_node_and_reflow 是标题栏关闭按钮和快捷键删除的共同路径；
///   WebView 是 macOS native first responder，Runtime::focus 不会自动清理 native first responder。
///   Wry 通过 `focus_parent()` 把 first responder 还给宿主 NSView；CEF 通过
///   `set_focus(0)` 取消 BrowserHost 焦点；两条后端路径都必须覆盖。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Browser Store、Wry、CEF 与节点删除源码
///   2. 检查 Store 对外暴露原生焦点释放入口
///   3. 检查 Wry/CEF 分别调用其原生后端的焦点释放 API
///   4. 检查删除路径在移除视图前调用释放入口和 GPUI root focus
///
/// 预期结果：
///   - 正断言：Wry、CEF 和 Store 都有对应的原生焦点释放链路
///   - 正断言：删除路径先释放原生焦点，再聚焦 GPUI root，最后释放 WebView
///   - 负断言：不能只调用 GPUI FocusHandle；那不会移除 native first responder
#[test]
fn test_browser_focus_handoff_releases_native_responder_before_view_removal() {
    let removal_source = include_str!("../../src/app/node_removal.rs");
    let store_source = include_str!("../../src/app/browser/store.rs");
    let wry_source = include_str!("../../src/app/browser/wry_backend.rs");
    let cef_source = include_str!("../../src/app/browser/cef_backend.rs");
    let start = removal_source
        .find("pub(crate) fn remove_node_and_reflow")
        .expect("app should keep remove_node_and_reflow as the shared node deletion path");
    let remove_block = &removal_source[start..];

    let release_pos = remove_block
        .find("self.release_browser_input(node_id)")
        .expect("Browser deletion must explicitly release the native WebView responder");
    let root_focus_pos = remove_block
        .find("self.focus_handle.focus(window)")
        .expect("Browser deletion must return keyboard focus to the GPUI root");
    let remove_view_pos = remove_block
        .find("self.remove_browser_view(node_id)")
        .expect("Browser deletion must still release the native WebView");

    assert!(
        store_source.contains("pub(crate) fn release_input"),
        "BrowserStore 必须公开原生输入焦点释放入口，供地址变更和删除复用"
    );
    assert!(
        wry_source.contains("focus_parent()"),
        "Wry/WebKit 后端必须将 first responder 交回宿主 NSView"
    );
    assert!(
        cef_source.contains("host.set_focus(0)"),
        "CEF 后端必须取消 BrowserHost 焦点，避免隐藏或销毁后继续吞掉键盘输入"
    );
    assert!(
        release_pos < root_focus_pos && root_focus_pos < remove_view_pos,
        "删除 Browser 的顺序必须是释放 native responder → 聚焦 GPUI root → 移除 WebView"
    );
    assert!(
        !remove_block
            .contains("self.focus_handle.focus(window);\n            self.remove_browser_view"),
        "不能只聚焦 GPUI root 后就删除 WebView；native first responder 必须先被显式释放"
    );
}

/// B-3: 地址栏编辑态必须由 BrowserAddressInput 托管 Enter 提交，不应再在全局 keydown 抢按键。
///
/// 数据构造（含关键数值的推导过程）：
///   source = app/browser.rs；BrowserAddressInput 会通过 `BrowserAddressInputEvent::Submit`
///   上报提交；
///   全局 on_key_down 无法接收输入法 composition，也会与网页/其他卡片抢焦点。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取浏览器编辑源码
///   2. 检查 BrowserAddressInput 订阅 Submit 事件
///   3. 检查根键盘路径不再调用旧地址栏 keydown 拦截器
///
/// 预期结果：
///   - 正断言：Enter 事件由原生输入实体发出并提交 URL
///   - 负断言：不应保留全局 `handle_browser_address_key_down` 拦截器
#[test]
fn test_browser_address_submits_from_native_input_instead_of_global_keydown() {
    let browser_source = include_str!("../../src/app/browser.rs");
    let keyboard_source = include_str!("../../src/app/keyboard.rs");

    assert!(
        browser_source.contains("BrowserAddressInputEvent::Submit"),
        "浏览器地址栏必须订阅 BrowserAddressInput 的 Enter 事件，才能保留 IME 与标准编辑命令"
    );
    assert!(
        browser_source.contains("BrowserAddressInput::new"),
        "浏览器地址栏必须创建 BrowserAddressInput，而不是只保存手工 draft 字符串"
    );
    assert!(
        !keyboard_source.contains("handle_browser_address_key_down"),
        "根键盘路径不得抢走地址栏输入；否则 IME、Delete、Copy 等标准命令无法交给 BrowserAddressInput"
    );
}

/// B-4: 地址栏编辑态必须让原生输入实体负责光标与选择绘制，不能保留伪光标。
///
/// 数据构造（含关键数值的推导过程）：
///   source = cards/browser/card.rs；编辑态由 `browser_address_input` 是否存在决定；
///   `BrowserAddressInput` 已绘制真实光标、选区与输入法组合下划线。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取浏览器卡片源码
///   2. 检查编辑态把 Input 实体渲染进地址栏
///   3. 检查旧的手绘 `browser-address-caret` 未残留
///
/// 预期结果：
///   - 正断言：编辑态挂载真正的 BrowserAddressInput 元素
///   - 正断言：非编辑态仍渲染 URL 纯文本
///   - 负断言：不应继续手绘脱离选区的固定 caret
#[test]
fn test_browser_address_edit_uses_native_cursor_and_selection() {
    let source = include_str!("../../src/cards/browser/card.rs");
    let input_source = include_str!("../../src/app/browser/address_input/render.rs");
    let address_pos = source
        .find(".id((\"browser-address\", node_id))")
        .expect("browser card should render an address element with a stable id");
    let address_block = &source[address_pos..(address_pos + 2400).min(source.len())];

    assert!(
        address_block.contains(".when_some(browser_address_input"),
        "编辑态必须把真实 BrowserAddressInput 实体放入地址栏，而不是渲染静态 URL 文本"
    );
    assert!(
        input_source.contains("ElementInputHandler::new")
            && input_source.contains("window.paint_quad"),
        "BrowserAddressInput 应通过 ElementInputHandler 接入 IME，并绘制真实光标与选区"
    );
    assert!(
        address_block.contains(".when(!editing"),
        "非编辑态应继续显示已提交 URL，不能让 Input 永远遮住地址文本"
    );
    assert!(
        !address_block.contains("browser-address-caret"),
        "旧的固定伪光标会与真实选择和 IME 下划线冲突，必须移除"
    );
}

/// B-11: 网页内的指针/输入焦点必须反馈到 Runtime 选择状态，使 Browser 卡片显示蓝色选中边框。
///
/// 数据构造（含关键数值的推导过程）：
///   - Browser 原生内容区 = Wry `build_as_child` 创建的 macOS WebView；该子视图不经过
///     GPUI `canvas` 的 mouse-down 冒泡。
///   - Wry 初始化脚本在每次导航前注入主 frame 与子 frame，并通过
///     `window.ipc.postMessage` 发出一个固定的 `BrowserEvent::PageInteraction`；固定事件名
///     只有 1 种，避免把网页文本当作命令解析。
///   - Runtime selection 只需保存 1 个 node_id；`select_only(node_id)` 会清除旧选择，
///     因而收到 Browser 交互后蓝框唯一归属当前 Browser 卡片。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 网页 pointerdown 或 focusin 触发 Wry 初始化脚本。
///   2. 脚本通过 Wry IPC 向 `BrowserStore` 的跨线程事件队列发送 PageInteraction。
///   3. app 帧循环 drain 该事件，并执行 Runtime::selection.select_only(node_id)。
///   4. canvas 渲染读取 selection，motion 层据此绘制 selected_stroke 蓝色边框。
///
/// 预期结果：
///   - 正断言：Wry 同时向主/子 frame 注册初始化脚本和 IPC handler，跨导航仍能收到网页交互。
///   - 正断言：BrowserEvent 有 PageInteraction，app drain 后调用 select_only(node_id)。
///   - 负断言：不能只依赖 GPUI canvas mouse-down；原生 WebView 不会把该事件冒泡回来。
///   - 负断言：IPC handler 不能把任意网页消息都当作 PageInteraction。
#[test]
fn test_browser_page_interaction_bridges_native_webview_to_runtime_selection() {
    let browser_source = include_str!("../../src/app/browser.rs");
    let wry_source = include_str!("../../src/app/browser/wry_backend.rs");

    assert!(
        wry_source.contains(
            "with_initialization_script_for_main_only(BROWSER_PAGE_INTERACTION_SCRIPT, false)"
        ),
        "Wry Browser 必须在每次导航前向主/子 frame 注入网页交互脚本，否则 iframe 内的输入也不会选中 Browser 卡片"
    );
    assert!(
        wry_source.contains("with_ipc_handler"),
        "Wry Browser 必须注册 IPC handler，才能把网页内 pointerdown/focusin 传回 Rust"
    );
    assert!(
        browser_source.contains("PageInteraction { node_id"),
        "BrowserEvent 必须显式表达网页交互，不能把焦点状态隐含在标题更新等无关事件中"
    );
    assert!(
        browser_source.contains("self.runtime.selection.select_only(node_id)"),
        "收到网页交互后必须把对应 Browser 写入 Runtime selection，蓝色选中边框才有状态来源"
    );
    assert!(
        !wry_source.contains(
            "let _ = event_sender.send(BrowserEvent::PageInteraction { node_id });\n        }),"
        ),
        "IPC handler 必须先核验固定消息内容，不能把任意网页 IPC 都误判成 Browser 页面交互"
    );
}

/// B-12: 离开网页内容时，必须先释放所有 Browser 的原生 responder。
///
/// 数据构造（含关键数值的推导过程）：
///   - visible_browsers = N；每个原生 Browser 都可能保持 macOS first responder，不能依赖
///     异步 PageInteraction 恰好已入队来判断唯一 active node。
///   - canvas mouse-down = 1 次左键；同一事件先对 N 个 view 调用 `focus_parent()` /
///     `set_focus(0)`，再调用 1 次 GPUI `focus_handle.focus(window)`。
///   - N = 0 时遍历为空；因此点击普通卡片不访问不存在的原生视图。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用户点击画布、地址栏/工具栏或任意卡片关闭控件。
///   2. app 统一遍历 BrowserStore 并释放每个原生 Browser 输入焦点。
///   3. canvas 路径再聚焦 app root，后续键盘事件由选中的卡片接收。
///   4. 页面交互事件即使尚未被帧循环 drain，也不会留下网页输入框吞键盘。
///
/// 预期结果：
///   - 正断言：Store 有统一的全量释放入口，画布和 Browser 命令都使用它。
///   - 正断言：释放动作位于 GPUI root 聚焦之前，保证 first responder 交接顺序。
///   - 负断言：不能只聚焦 GPUI root；这不会让 WebView 失焦。
///   - 负断言：不能保留 active_input_node 这种异步竞态状态作为释放前提。
#[test]
fn test_leaving_browser_releases_all_native_responders_before_root_focus() {
    let browser_source = include_str!("../../src/app/browser.rs");
    let store_source = include_str!("../../src/app/browser/store.rs");
    let canvas_events_source = include_str!("../../src/canvas/events.rs");
    let mouse_down_start = canvas_events_source
        .find("pub(super) fn on_mouse_down")
        .expect("canvas must keep its shared mouse-down route");
    let mouse_down_end = canvas_events_source[mouse_down_start..]
        .find("pub(super) fn should_focus_app_root_for_mouse_down")
        .map(|offset| mouse_down_start + offset)
        .expect("canvas mouse-down route should end before its focus predicate");
    let mouse_down_block = &canvas_events_source[mouse_down_start..mouse_down_end];
    let release_pos = mouse_down_block
        .find("app.release_all_browser_input();")
        .expect("clicking the GPUI canvas must release every native Browser responder");
    let root_focus_pos = mouse_down_block
        .find("app.focus_handle.focus(window);")
        .expect("canvas mouse-down must still focus the GPUI root after native handoff");

    assert!(
        store_source.contains("pub(crate) fn release_all_input"),
        "BrowserStore 必须提供全量释放入口，不能依赖异步网页事件记录唯一 active Browser"
    );
    assert!(
        browser_source.contains("pub(crate) fn release_all_browser_input"),
        "app 必须暴露统一的 Browser 焦点释放入口，供画布和卡片控件共同调用"
    );
    assert!(
        browser_source.contains("self.release_all_browser_input();\n        match action.command"),
        "地址栏和工具栏会阻止画布冒泡，Browser 命令前必须自行释放其他网页焦点"
    );
    assert!(
        release_pos < root_focus_pos,
        "离开网页的顺序必须是先释放 native first responder，再聚焦 GPUI root；反过来仍会导致网页吞键盘"
    );
    assert!(
        !store_source.contains("active_input_node:"),
        "释放路径不能依赖 active_input_node，否则异步 PageInteraction 未送达时会漏掉实际 first responder"
    );
    assert!(
        !mouse_down_block
            .contains("app.focus_handle.focus(window);\n        app.release_all_browser_input();"),
        "不能先聚焦 GPUI root 再释放 WebView；该顺序会留下短暂且可见的网页抢焦点窗口"
    );
}

/// B-13: CEF 的可见性同步不能在每一帧重新抢占网页焦点。
///
/// 数据构造（含关键数值的推导过程）：
///   - BrowserView::sync 每次 canvas render 都调用 `set_visible(true)`；动画帧约为
///     12ms，因此一秒最多约 1000ms / 12ms ≈ 83 次同步。
///   - visible = true 只表达显示状态；它与“用户正在网页中输入”是两个独立概念，
///     因而不能把 true 转换为 `host.set_focus(1)`。
///   - release_input 仍使用 `set_focus(0)`；0 明确表示交还 CEF BrowserHost 焦点。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 每帧同步 Browser 卡片，CEF 后端仅调用 was_hidden(false) 保持可见。
///   2. 用户点到画布时，BrowserStore 调用 release_input，CEF 接收 set_focus(0)。
///   3. 后续 render 再次 set_visible(true) 不会把焦点强行夺回网页。
///
/// 预期结果：
///   - 正断言：set_visible 仍同步 was_hidden，隐藏/显示生命周期不回退。
///   - 正断言：release_input 仍显式调用 set_focus(0)。
///   - 负断言：set_visible 不得调用 set_focus(visible as i32)。
///   - 负断言：可见性变更不能隐式调用 set_focus(1)。
#[test]
fn test_cef_visibility_sync_does_not_reclaim_browser_focus() {
    let cef_source = include_str!("../../src/app/browser/cef_backend.rs");
    let set_visible_start = cef_source
        .find("pub(crate) fn set_visible")
        .expect("CEF backend must keep a visibility lifecycle method");
    let set_visible_end = cef_source[set_visible_start..]
        .find("pub(crate) fn release_input")
        .map(|offset| set_visible_start + offset)
        .expect("CEF visibility method should appear before release_input");
    let set_visible_block = &cef_source[set_visible_start..set_visible_end];

    assert!(
        set_visible_block.contains("host.was_hidden((!visible) as i32)"),
        "CEF set_visible 必须继续同步隐藏状态，不能为了修焦点而丢失 Browser 显示生命周期"
    );
    assert!(
        cef_source.contains("host.set_focus(0)"),
        "CEF release_input 必须继续明确调用 set_focus(0)，供画布点击释放原生 responder"
    );
    assert!(
        !set_visible_block.contains("host.set_focus(visible as i32)"),
        "CEF 可见性同步不得把 visible=true 变成 set_focus(1)，否则每帧都会重新抢走网页焦点"
    );
    assert!(
        !set_visible_block.contains("host.set_focus(1)"),
        "CEF 显示 Browser 卡片不应隐式聚焦网页；焦点只能由用户的原生页面交互获得"
    );
}
