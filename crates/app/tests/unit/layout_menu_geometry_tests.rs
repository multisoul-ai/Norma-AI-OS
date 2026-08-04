use super::*;
use canvas::CanvasRect;

/// B-15: 布局菜单打开时返回底部居中、宽 294、距底 82 的屏幕矩形；关闭时返回 None。
///
/// 数据构造（含关键数值的推导过程）：
///   viewport = 1280×800（生产默认窗口，见 workspace/windowing.rs initial_window_dimensions）
///   LAYOUT_MENU_WIDTH_PX  = 294   （菜单面板宽度，与 layout_menu.rs render_layout_menu 的 w(294) 一致）
///   LAYOUT_MENU_BOTTOM_PX = 82    （菜单底部距窗口底部，与 bottom(px(82.0)) 一致）
///   LAYOUT_MENU_HEIGHT_PX = 240   （面板内容推导：heading 20 + 填充块 49 + 轨道 26 + 尺寸块 49 + reflow 32 + gap×4=40 + padding×2=24）
///   menu_left = (1280 - 294) / 2 = 493
///   menu_top  = 800 - 82 - 240 = 478
///
/// 执行过程（逐步说明系统如何处理）：
///   1. open=false → 菜单未渲染，返回 None（无遮挡判断，任何浏览器都不应被隐藏）。
///   2. open=true  → 按宽度/底部/高度常量合成面板矩形。
///
/// 预期结果：
///   - 负断言：关闭时返回 None，防止菜单未打开时误隐藏浏览器。
///   - 正断言：矩形 = (493, 478, 294, 240)，水平居中且底部距窗底 82。
#[test]
fn test_layout_menu_screen_rect_is_bottom_centered_when_open() {
    assert!(
        layout_menu_screen_rect(false, 1280.0, 800.0).is_none(),
        "菜单未打开时必须返回 None，否则会导致所有浏览器都被误判遮挡"
    );

    let rect =
        layout_menu_screen_rect(true, 1280.0, 800.0).expect("菜单打开时必须返回菜单面板的屏幕矩形");

    assert_eq!(
        rect,
        CanvasRect::new(493.0, 478.0, 294.0, 240.0),
        "菜单矩形必须水平居中、宽 294、底部距窗底 82，才能与浏览器卡片做精确重叠判断"
    );
}

/// B-16: 浏览器内容区只有与打开的布局菜单矩形相交时才被判定遮挡，其余不受影响。
///
/// 数据构造（含关键数值的推导过程）：
///   menu = layout_menu_screen_rect(true, 1280, 800) = (493, 478, 294, 240)
///   covered_card = (500, 480, 200, 200)：y∈[480,680] 落在菜单 y∈[478,718] 内，x∈[500,700] 落在菜单 x∈[493,787] 内
///   top_card     = (500, 50, 200, 200)：y∈[50,250] 完全在菜单上方，不相交
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 菜单打开，浏览器内容区与菜单矩形相交 → 判定遮挡（该卡片将被排除出 visible_browser_nodes）。
///   2. 菜单关闭（None）→ 即使内容区坐标不变也不遮挡。
///   3. 菜单打开但内容区在菜单上方 → 不遮挡（保持正常渲染）。
///
/// 预期结果：
///   - 正断言：covered_card 在菜单打开时被判定遮挡。
///   - 负断言：covered_card 在菜单关闭时不被遮挡。
///   - 负断言：top_card 在菜单打开时不被遮挡，说明不会误伤远离菜单的卡片。
#[test]
fn test_browser_occluded_by_layout_menu_only_when_overlapping() {
    let menu_open = layout_menu_screen_rect(true, 1280.0, 800.0);
    let covered_card = CanvasRect::new(500.0, 480.0, 200.0, 200.0);
    let top_card = CanvasRect::new(500.0, 50.0, 200.0, 200.0);

    assert!(
        browser_occluded_by_layout_menu(covered_card, menu_open),
        "与打开的菜单矩形相交的浏览器内容区必须被判定遮挡"
    );
    assert!(
        !browser_occluded_by_layout_menu(covered_card, None),
        "菜单未打开时即使坐标相同也不能判定遮挡"
    );
    assert!(
        !browser_occluded_by_layout_menu(top_card, menu_open),
        "菜单上方、不相交的浏览器卡片必须保持正常渲染，不能被误判遮挡"
    );
}

/// B-17: 关闭状态必须返回 None（独立于尺寸参数），避免误隐藏浏览器。
///
/// 数据构造（含关键数值的推导过程）：
///   任意窗口尺寸 1280×800 与 1024×768
///   open=false → 不应依赖窗口尺寸，恒为 None
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用两种窗口尺寸调用 open=false 的 layout_menu_screen_rect
///   2. 检查两者均为 None
///
/// 预期结果：
///   - 正断言：open=false 恒为 None
///   - 负断言：open=false 不应返回 Some，否则菜单关闭也会参与遮挡判断
#[test]
fn test_layout_menu_screen_rect_return_none_when_closed() {
    assert!(
        layout_menu_screen_rect(false, 1280.0, 800.0).is_none(),
        "菜单关闭时任意窗口尺寸都应返回 None"
    );
    assert!(
        layout_menu_screen_rect(false, 1024.0, 768.0).is_none(),
        "菜单关闭时不依赖窗口尺寸，恒为 None"
    );
}
