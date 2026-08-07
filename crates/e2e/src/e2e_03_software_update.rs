//! Settings 左下角检查更新：在品牌版本卡片内点击标准 Sparkle 检查入口。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// 检查更新按钮必须位于左下角品牌卡片，点击后继续保持当前 Settings 页面。
///
/// 数据构造（含关键数值的推导过程）：
///   Settings 打开次数 = 1；
///   独立更新导航数 = 0；
///   品牌卡片数 = 1；
///   检查按钮点击次数 = 1；
///   测试 Framework 数 = 0（headless bundle 不嵌入 Sparkle，必须安全降级）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App、进入 Canvas 并打开 Settings。
///   2. 确认默认 Themes 页面保持选中，Software Update 导航不存在。
///   3. 读取面板、品牌卡片、版本与检查按钮 bounds，验证按钮位于左下卡片右侧。
///   4. 点击检查按钮；缺少 Framework 时 updater 安全降级，Settings 仍停留在 Themes。
///
/// 预期结果：
///   - 正断言：品牌卡片、当前版本和检查按钮全部存在，按钮位于卡片右半侧。
///   - 正断言：点击后 Settings 保持打开并继续停留 Themes。
///   - 负断言：不得渲染 Software Update 导航，也不得因测试 bundle 缺 Framework 崩溃。
#[test]
fn e2e_03_settings_footer_update_button_uses_safe_sparkle_check_flow() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 Settings update workspace");
    enter_canvas(&app, cx, workspace.path());

    click_debug(cx, "toolbar-settings");
    let opened = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        opened.active_section, "Themes",
        "打开 Settings 后应保持默认 Themes 页面，不能跳入已移除的更新页，snapshot={opened:?}"
    );
    assert!(
        cx.debug_bounds("settings-section-software-update")
            .is_none(),
        "Settings 不得再渲染 Software Update 导航入口"
    );
    let panel = cx
        .debug_bounds("settings-panel")
        .expect("Settings 打开后必须渲染统一面板");
    let footer = cx
        .debug_bounds("settings-brand-footer")
        .expect("Settings 左下角必须渲染品牌卡片");
    let version = cx
        .debug_bounds("settings-brand-version")
        .expect("品牌卡片必须显示当前安装版本");
    let update = cx
        .debug_bounds("settings-check-for-updates")
        .expect("品牌卡片必须渲染可点击的检查更新按钮");
    assert!(
        footer.contains(&update.center()),
        "检查更新按钮中心必须位于品牌卡片内部，footer={footer:?}, update={update:?}"
    );
    assert!(
        update.center().x > version.center().x,
        "检查更新按钮必须位于版本信息右侧，version={version:?}, update={update:?}"
    );
    assert!(
        footer.center().x < panel.center().x && footer.center().y > panel.center().y,
        "品牌卡片必须位于 Settings 面板左下区域，panel={panel:?}, footer={footer:?}"
    );

    click_debug(cx, "settings-check-for-updates");
    let checked = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        checked.settings_open,
        "点击检查更新后 Settings 必须保持打开，等待 Sparkle 标准更新流程，snapshot={checked:?}"
    );
    assert_eq!(
        checked.active_section, "Themes",
        "点击检查更新不得改变当前 Themes 路由，snapshot={checked:?}"
    );
    assert_ne!(
        checked.active_section, "Software Update",
        "检查更新不得恢复已移除的 Software Update 页面，snapshot={checked:?}"
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
