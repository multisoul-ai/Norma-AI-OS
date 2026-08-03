//! 底部 New 菜单的 hover 视觉反馈：真实指针命中与生产主题 token 的 L1 验收。

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
