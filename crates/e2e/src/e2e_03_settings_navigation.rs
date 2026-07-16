//! Settings 导航分组刷新：以真实渲染树点击六个可配置入口，锁定现有业务路由不变。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Settings 任务导向导航：六个可配置入口能分别进入对应页面，且切换不会关闭面板。
///
/// 数据构造（含关键数值的推导过程）：
///   visible entries = 6（Themes + Wallpaper + Keyboard Shortcuts + Voice + Agents + Agent Hooks）。
///   hidden mock entries = 2（Runtime + Skills，不参与点击序列）。
///   click count = 6（每个已实现入口各一次，覆盖三个任务导向分组）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App 并进入 Canvas，点击 `toolbar-settings` 打开 Settings 面板。
///   2. 依次点击 Themes、Wallpaper、Keyboard、Voice、Agents 与 Hooks 的稳定 debug selector。
///   3. 每次点击后读取公开 `SettingsE2eSnapshot`，确认 route 仍指向原 section。
///   4. 每次确认 Settings 仍保持打开，证明导航重构没有截断原有交互会话。
///
/// 预期结果：
///   - 断言 A：六个入口各自精确进入 Themes / Wallpaper / Keyboard Shortcuts / Voice / Agents / Agent Hooks。
///   - 断言 B：每一次切换后 Settings 都保持打开，不能被导航点击关闭。
///   - 断言 C：最后的 Hooks 页面不得误路由回 Voice，防止分组 DOM 重排错绑点击处理器。
#[test]
fn e2e_03_task_oriented_settings_navigation_preserves_all_existing_routes() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let temporary_root = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, temporary_root.path());

    click_debug(cx, "toolbar-settings");

    click_debug(cx, "settings-section-themes");
    let themes = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        themes.active_section, "Themes",
        "Themes 导航应进入真实主题选择页面，snapshot: {themes:?}"
    );
    assert!(
        themes.settings_open,
        "点击 Themes 后 Settings 应保持打开，snapshot: {themes:?}"
    );

    click_debug(cx, "settings-section-wallpaper");
    let wallpaper = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        wallpaper.active_section, "Wallpaper",
        "Wallpaper 导航应进入原 Wallpaper 页面，snapshot: {wallpaper:?}"
    );
    assert!(
        wallpaper.settings_open,
        "点击 Wallpaper 后 Settings 应保持打开，snapshot: {wallpaper:?}"
    );

    click_debug(cx, "settings-section-keyboard");
    let keyboard = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        keyboard.active_section, "Keyboard Shortcuts",
        "Keyboard 导航应进入原 Keyboard Shortcuts 页面，snapshot: {keyboard:?}"
    );
    assert!(
        keyboard.settings_open,
        "点击 Keyboard 后 Settings 应保持打开，snapshot: {keyboard:?}"
    );

    click_debug(cx, "settings-section-voice");
    let voice = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        voice.active_section, "Voice",
        "Voice 导航应进入原 Voice 页面，snapshot: {voice:?}"
    );
    assert!(
        voice.settings_open,
        "点击 Voice 后 Settings 应保持打开，snapshot: {voice:?}"
    );

    click_debug(cx, "settings-section-agents");
    let agents = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        agents.active_section, "Agents",
        "Agents 导航应进入原 Agents 页面，snapshot: {agents:?}"
    );
    assert!(
        agents.settings_open,
        "点击 Agents 后 Settings 应保持打开，snapshot: {agents:?}"
    );

    click_debug(cx, "settings-section-hooks");
    let hooks = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        hooks.active_section, "Agent Hooks",
        "Agent Hooks 导航应进入原 Hooks 页面，snapshot: {hooks:?}"
    );
    assert!(
        hooks.settings_open,
        "点击 Agent Hooks 后 Settings 应保持打开，snapshot: {hooks:?}"
    );
    assert_ne!(
        hooks.active_section, "Voice",
        "点击 Agent Hooks 后不得仍停留在 Voice 页面，snapshot: {hooks:?}"
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
