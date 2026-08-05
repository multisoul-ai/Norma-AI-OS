//! Settings → Software Update：从真实导航进入页面并点击标准 Sparkle 检查入口。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Software Update 页面必须显示当前版本和可点击检查入口，点击后继续保持 Settings 上下文。
///
/// 数据构造（含关键数值的推导过程）：
///   Settings 打开次数 = 1；
///   导航点击次数 = 1（Software Update）；
///   检查按钮点击次数 = 1；
///   测试 Framework 数 = 0（headless bundle 不嵌入 Sparkle，必须安全降级）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App、进入 Canvas 并打开 Settings。
///   2. 点击 `settings-section-software-update`，确认路由进入 Software Update。
///   3. 读取版本与检查按钮 bounds，证明用户可见控件已经渲染。
///   4. 点击检查按钮；缺少 Framework 时 updater 安全降级，Settings 仍保持打开。
///
/// 预期结果：
///   - 正断言：Software Update 路由、当前版本和检查按钮全部存在。
///   - 正断言：点击后 Settings 保持打开，等待 Sparkle 标准 UI 接管真实发行版流程。
///   - 负断言：点击后不得误路由回 Themes，也不得因测试 bundle 缺 Framework 崩溃。
#[test]
fn e2e_03_software_update_button_uses_safe_sparkle_check_flow() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 Software Update workspace");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-software-update");

    let opened = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        opened.active_section, "Software Update",
        "点击 Software Update 后必须进入真实更新设置页，snapshot={opened:?}"
    );
    assert!(
        cx.debug_bounds("settings-software-update-current-version")
            .is_some(),
        "Software Update 页面必须显示当前安装版本"
    );
    assert!(
        cx.debug_bounds("settings-check-for-updates").is_some(),
        "Software Update 页面必须渲染可点击的 Check for Updates… 按钮"
    );

    click_debug(cx, "settings-check-for-updates");
    let checked = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        checked.settings_open,
        "点击检查更新后 Settings 必须保持打开，等待 Sparkle 标准更新流程，snapshot={checked:?}"
    );
    assert_ne!(
        checked.active_section, "Themes",
        "点击检查更新不得误路由回默认 Themes 页面，snapshot={checked:?}"
    );
}

fn click_debug(cx: &mut VisualTestContext, selector: &'static str) {
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` should have debug bounds before click"));
    let position = bounds.center();
    cx.simulate_mouse_move(position, None, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}
