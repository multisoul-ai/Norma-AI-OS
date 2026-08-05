//! Settings → Performance：闲时动画降频的默认值、即时切换与持久化闭环。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app_with_settings_store};

/// Settings → Performance：闲时动画降频默认关闭，显式开启后立即生效并跨重启恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   settings stores = 1（两个 App 复用同一 SQLite 文件）
///   app launches = initial(1) + reload(1) = 2
///   default throttle = false
///   user toggles = off → on，共 1 次
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用空 SettingsStore 启动 App，打开 Settings 并进入 Performance。
///   2. 从公开 snapshot 读取默认值，并确认真实 Switch 点击目标存在。
///   3. 点击 Switch，确认内存状态立即变为开启，再等待后台持久化完成。
///   4. 使用同一 SettingsStore 重启 App，重新进入 Performance 并读取恢复状态。
///
/// 预期结果：
///   - 正断言：Performance 入口和 Switch 都存在，点击后立即开启且重启后仍开启。
///   - 负断言：首次启动不得默认开启；重启后不得回退为默认关闭。
#[test]
fn e2e_03_idle_animation_throttling_defaults_off_and_persists_opt_in() {
    let temp = tempfile::TempDir::new().expect("应创建隔离的 Performance Settings 数据目录");
    let store = storage::SettingsStore::new(temp.path().join("performance.sqlite"));
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 Performance workspace");

    {
        let mut cx = TestAppContext::single();
        let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
        enter_canvas(&app, cx, workspace.path());

        click_debug(cx, "toolbar-settings");
        assert!(
            cx.debug_bounds("settings-section-performance").is_some(),
            "Settings 导航必须渲染 Performance 入口"
        );
        click_debug(cx, "settings-section-performance");
        assert!(
            cx.debug_bounds("settings-idle-animation-throttling")
                .is_some(),
            "Performance 页面必须渲染闲时动画降频 Switch"
        );

        let initial = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            initial.active_section, "Performance",
            "点击 Performance 后 Settings route 必须切换到对应页面"
        );
        assert!(
            !initial.idle_animation_throttling_enabled,
            "空 SettingsStore 首次启动时闲时动画降频必须默认关闭"
        );

        click_debug(cx, "settings-idle-animation-throttling");
        let enabled = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert!(
            enabled.idle_animation_throttling_enabled,
            "点击 Switch 后闲时动画降频必须立即变为开启"
        );
        assert!(
            enabled.settings_open,
            "切换性能选项后 Settings 面板必须保持打开"
        );
        cx.run_until_parked();
    }

    let mut reload_cx = TestAppContext::single();
    let (reloaded_app, reload_cx) =
        make_app_with_settings_store(&mut reload_cx, echo_factory(), store);
    enter_canvas(&reloaded_app, reload_cx, workspace.path());
    click_debug(reload_cx, "toolbar-settings");
    click_debug(reload_cx, "settings-section-performance");

    let reloaded = reloaded_app.update(reload_cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert!(
        reloaded.idle_animation_throttling_enabled,
        "重启后必须从 SettingsStore 恢复用户显式开启的闲时动画降频"
    );
    assert_ne!(
        reloaded.active_section, "Themes",
        "重启后的 Performance 验证不得误读默认 Themes 页面状态"
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
