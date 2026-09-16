//! 底部 New 菜单的 hover 视觉反馈：真实指针命中与生产主题 token 的 L1 验收。

use app::NewBrowser;
use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// E2E-14：鼠标移入 New 菜单项时必须使用与面板可辨识的主题强调色反馈。
///
/// 数据构造（含关键数值的推导过程）：
///   默认主题 = Paper Blue（应用首次启动的默认主题）。
///   hover 最小 RGB 距离 = 24；三通道平均变化约为 24 / sqrt(3) ≈ 13.9。
///   菜单项数量 ≥ 2；Terminal 是 hover 目标，Claude 是未 hover 的负对照 hitbox。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas 并点击底部 New，确认生产菜单已展开。
///   2. 读取 Terminal 与 Claude 菜单项的真实 bounds，把鼠标移到 Terminal 中心。
///   3. 从生产视觉 token 快照读取 panel、hover、hover border 与 pressed 配色。
///   4. 将 hover 色按 alpha 合成到 panel 上，计算进入前后的 RGB 距离。
///
/// 预期结果：
///   - 正断言：鼠标移入 Terminal 后菜单保持打开，Terminal 与 Claude hitbox 都存在。
///   - 正断言：hover 合成后的 RGB 距离至少为 24，且 hover border 非透明。
///   - 负断言：hover 与 pressed 不能共用同一填充，否则悬停和按下状态无法区分。
#[test]
fn e2e_14_new_card_menu_hover_has_visible_theme_feedback() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("卡片菜单 hover e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-new-terminal");
    let terminal = cx
        .debug_bounds("new-menu-terminal")
        .expect("New 菜单必须渲染 Terminal hover 目标");
    let claude = cx
        .debug_bounds("new-menu-claude")
        .expect("New 菜单必须保留 Claude 作为未 hover 的相邻负对照");

    cx.simulate_mouse_move(terminal.center(), None, Modifiers::none());
    cx.run_until_parked();

    let (menu_open, panel, hover_fill, hover_border, pressed_fill) =
        app.update(cx, |app, _| app.new_menu_visual_e2e_snapshot());
    let distance = rgb_distance_after_overlay(panel, hover_fill);

    assert!(
        menu_open,
        "鼠标移入 Terminal 菜单项时 New 菜单必须保持打开，不能把 hover 误判为外部点击"
    );
    assert_eq!(
        cx.debug_bounds("new-menu-terminal"),
        Some(terminal),
        "hover 后 Terminal 菜单项 hitbox 必须保持稳定，不能因高亮发生布局跳动"
    );
    assert_eq!(
        cx.debug_bounds("new-menu-claude"),
        Some(claude),
        "hover Terminal 时相邻 Claude 菜单项仍应存在，不能被选中背景遮蔽或移除"
    );
    assert!(
        distance >= 24.0,
        "New 菜单 hover 与默认 panel 的 RGB 距离必须至少为 24，实际为 {distance:.2}"
    );
    assert_ne!(
        hover_border & 0xff,
        0,
        "New 菜单 hover border alpha 必须非零，确保背景变化之外还有清晰边界"
    );
    assert_ne!(
        hover_fill, pressed_fill,
        "New 菜单 hover 与 pressed 必须使用不同填充，不能把两个交互状态合并"
    );
}

/// E2E-14：New 菜单与 Browser 原生内容区重叠时，菜单必须保持在最上层可见。
///
/// 数据构造（含关键数值的推导过程）：
///   Browser 卡片数 = 5，由 5 次 `NewBrowser` action 创建；每行 3 张 → ceil(5/3) = 2 行。
///   New 菜单宽度 = 180px；生产布局高度固定为 362px，避免文字度量的 0.5px 波动。
///   默认 headless viewport 中第 5 张 Browser 卡片位于第二行中间列，菜单从底部 Dock 向上展开，
///   因此两者 bounds 必须存在非空交集，该交集即用户截图中的遮挡区域。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 进入 Canvas，通过公开 `NewBrowser` action 创建 5 张 Browser 卡片。
///   2. 确认菜单未打开时 Browser 使用正常内容区，没有渲染遮挡占位层。
///   3. 点击底部 New，读取菜单与 Browser 内容区的真实 bounds 并确认重叠。
///   4. 检查 Browser 已切换为 GPUI 遮挡占位层，从渲染树移除会穿透 GPUI 的原生视图。
///
/// 预期结果：
///   - 正断言：New 菜单展开后与 Browser 内容区存在交集，确实覆盖用户报告的场景。
///   - 正断言：重叠时必须渲染 `browser-card-occluded` 占位层，保证菜单位于最上层。
///   - 负断言：菜单未打开时不得提前遮挡 Browser，否则正常网页内容会丢失。
#[test]
fn e2e_14_new_card_menu_occludes_overlapping_native_browser_content() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("New 菜单遮挡 e2e 应创建临时 workspace");
    enter_canvas(&app, cx, workspace.path());

    cx.dispatch_action(NewBrowser);
    cx.dispatch_action(NewBrowser);
    cx.dispatch_action(NewBrowser);
    cx.dispatch_action(NewBrowser);
    cx.dispatch_action(NewBrowser);
    cx.run_until_parked();
    let browser_content = cx
        .debug_bounds("browser-native-surface")
        .expect("NewBrowser 应先渲染正常 Browser 内容区");
    assert!(
        cx.debug_bounds("browser-card-occluded").is_none(),
        "New 菜单未打开时 Browser 不得提前切换成遮挡占位层"
    );

    click_debug(cx, "toolbar-new-terminal");
    let menu = cx
        .debug_bounds("new-menu")
        .expect("点击 New 后必须渲染完整卡片菜单");

    assert!(
        menu.intersects(&browser_content),
        "回归场景必须让 New 菜单与 Browser 内容区重叠，menu={menu:?} browser={browser_content:?}"
    );
    assert!(
        cx.debug_bounds("browser-card-occluded").is_some(),
        "New 菜单覆盖 Browser 内容区时必须用 GPUI 占位层移除原生视图穿透"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("hover e2e 点击前必须存在 debug selector {selector:?}"));
    cx.simulate_mouse_move(bounds.center(), None, Modifiers::none());
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn rgb_distance_after_overlay(background: u32, overlay: u32) -> f32 {
    let overlay_alpha = (overlay & 0xff) as f32 / 255.0;
    let channel = |shift: u32| {
        let background = ((background >> shift) & 0xff) as f32;
        let foreground = ((overlay >> shift) & 0xff) as f32;
        background + (foreground - background) * overlay_alpha
    };
    let red_delta = channel(24) - ((background >> 24) & 0xff) as f32;
    let green_delta = channel(16) - ((background >> 16) & 0xff) as f32;
    let blue_delta = channel(8) - ((background >> 8) & 0xff) as f32;
    (red_delta * red_delta + green_delta * green_delta + blue_delta * blue_delta).sqrt()
}
