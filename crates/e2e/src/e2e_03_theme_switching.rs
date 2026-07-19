//! Core UI V2 主题切换：通过真实 Settings 渲染树点击验证六主题入口与即时全局状态。

use gpui::{Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app, make_app_with_settings_store};

/// Settings → Themes：六套设计主题均可见，点击深浅主题会立即更新全局配色且不关闭面板。
///
/// 数据构造（含关键数值的推导过程）：
///   light theme cards = 3（paper-blue / linen-sage / silver-violet）
///   dark theme cards  = 3（sapphire-night / midnight-violet / forest-mint）
///   total cards       = 3 + 3 = 6
///   默认主题           = paper-blue（设计文档指定）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App、进入 Canvas，点击工具栏 Settings。
///   2. 点击 `settings-section-themes`，确认六个稳定主题卡片都来自真实渲染树。
///   3. 点击 `settings-theme-forest-mint`，读取公开 snapshot。
///   4. 点击 `settings-theme-paper-blue`，再次读取 snapshot。
///
/// 预期结果：
///   - 正断言：六个主题 click target 均存在，Themes 页面可被真实用户导航到。
///   - 正断言：Forest Mint 后 active id/base/accent 精确切换并保持 Settings 打开。
///   - 正断言：切回 Paper Blue 后 id/base/accent 精确恢复。
///   - 负断言：Forest Mint 不得继续报告 Paper Blue；切回后不得残留 dark mode。
#[test]
fn e2e_03_settings_switches_all_core_ui_v2_theme_styles() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let temporary_root = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, temporary_root.path());

    click_debug(cx, "toolbar-settings");
    click_debug(cx, "settings-section-themes");

    assert!(
        cx.debug_bounds("settings-theme-paper-blue").is_some(),
        "Themes 页面应渲染 Paper Blue 点击目标"
    );
    assert!(
        cx.debug_bounds("settings-theme-linen-sage").is_some(),
        "Themes 页面应渲染 Linen Sage 点击目标"
    );
    assert!(
        cx.debug_bounds("settings-theme-silver-violet").is_some(),
        "Themes 页面应渲染 Silver Violet 点击目标"
    );
    assert!(
        cx.debug_bounds("settings-theme-sapphire-night").is_some(),
        "Themes 页面应渲染 Sapphire Night 点击目标"
    );
    assert!(
        cx.debug_bounds("settings-theme-midnight-violet").is_some(),
        "Themes 页面应渲染 Midnight Violet 点击目标"
    );
    assert!(
        cx.debug_bounds("settings-theme-forest-mint").is_some(),
        "Themes 页面应渲染 Forest Mint 点击目标"
    );

    click_debug(cx, "settings-theme-forest-mint");
    let forest = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        forest.active_section, "Themes",
        "主题卡点击后应停留在 Themes 页面"
    );
    assert_eq!(
        forest.active_theme_id, "forest-mint",
        "点击 Forest Mint 应立即更新稳定主题 id"
    );
    assert_eq!(
        forest.theme_canvas_from, 0x091716ff,
        "Forest Mint Base 应切换为 #091716"
    );
    assert_eq!(
        forest.theme_accent, 0x64e0b5ff,
        "Forest Mint Accent 应切换为 Mint #64E0B5"
    );
    assert!(forest.theme_is_dark, "Forest Mint 必须报告 dark mode");
    assert!(forest.settings_open, "切换 Forest Mint 不应关闭 Settings");
    assert_ne!(
        forest.active_theme_id, "paper-blue",
        "Forest Mint 被选中后不得继续报告默认主题"
    );

    click_debug(cx, "settings-theme-paper-blue");
    let paper = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        paper.active_theme_id, "paper-blue",
        "点击 Paper Blue 应恢复默认稳定主题 id"
    );
    assert_eq!(
        paper.theme_canvas_from, 0xfffefbff,
        "Paper Blue Base 应恢复为 #FFFEFB"
    );
    assert_eq!(
        paper.theme_accent, 0x2f80edff,
        "Paper Blue Accent 应恢复为 Cobalt #2F80ED"
    );
    assert!(!paper.theme_is_dark, "切回 Paper Blue 后不得残留 dark mode");
    assert!(
        paper.settings_open,
        "切回 Paper Blue 后 Settings 仍应保持打开"
    );
}

/// Settings → Themes 自动保存：选择非默认主题后，重启 App 仍恢复同一主题。
///
/// 数据构造（含关键数值的推导过程）：
///   settings stores = 1（两个 App 复用同一 SQLite 文件）
///   app launches    = 2（首次选择 + 重载验证）
///   theme changes   = 1（paper-blue → forest-mint）
///   默认主题         = paper-blue（用于负断言，避免“重载为默认值”误判通过）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用隔离 SettingsStore 启动第一个 headless App，进入 Canvas 并打开 Themes。
///   2. 点击 `settings-theme-forest-mint`，等待异步保存任务完成。
///   3. 销毁第一个 App 上下文，使用同一 SettingsStore 启动第二个 App。
///   4. 进入 Canvas、重新打开 Themes，通过公开 snapshot 读取恢复后的全局主题。
///
/// 预期结果：
///   - 正断言：首次点击后主题即时变为 Forest Mint。
///   - 正断言：第二次启动后稳定主题 id、dark mode 与 Accent 均恢复为 Forest Mint。
///   - 负断言：第二次启动不得回退到默认 Paper Blue。
#[test]
fn e2e_03_theme_selection_persists_across_app_reload() {
    let temp = tempfile::TempDir::new().expect("应创建隔离的 Themes Settings 数据目录");
    let store = storage::SettingsStore::new(temp.path().join("themes.sqlite"));
    let workspace = tempfile::TempDir::new().expect("应创建隔离的 Themes workspace");

    {
        let mut cx = TestAppContext::single();
        let (app, cx) = make_app_with_settings_store(&mut cx, echo_factory(), store.clone());
        enter_canvas(&app, cx, workspace.path());

        click_debug(cx, "toolbar-settings");
        click_debug(cx, "settings-section-themes");
        click_debug(cx, "settings-theme-forest-mint");

        let selected = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
        assert_eq!(
            selected.active_theme_id, "forest-mint",
            "首次点击后应立即应用 Forest Mint"
        );
        assert_ne!(
            selected.active_theme_id, "paper-blue",
            "首次点击后不得继续使用默认 Paper Blue"
        );
        cx.run_until_parked();
    }

    let mut reload_cx = TestAppContext::single();
    let (reloaded_app, reload_cx) =
        make_app_with_settings_store(&mut reload_cx, echo_factory(), store);
    enter_canvas(&reloaded_app, reload_cx, workspace.path());
    click_debug(reload_cx, "toolbar-settings");
    click_debug(reload_cx, "settings-section-themes");

    let reloaded = reloaded_app.update(reload_cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        reloaded.active_theme_id, "forest-mint",
        "重载后应从 SettingsStore 恢复 Forest Mint"
    );
    assert!(
        reloaded.theme_is_dark,
        "重载后的 Forest Mint 必须保持 dark mode"
    );
    assert_eq!(
        reloaded.theme_accent, 0x64e0b5ff,
        "重载后的 Accent 应保持 Forest Mint 的 #64E0B5"
    );
    assert_ne!(
        reloaded.active_theme_id, "paper-blue",
        "重载后不得回退到默认 Paper Blue"
    );
}

/// Settings → Themes 参考图布局：每套主题使用“大预览 + 独立选择栏”的三列等宽卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   card height    = border(1px × 2) + preview(150px) + choice row(62px) = 214px
///   light columns  = Paper Blue + Linen Sage + Silver Violet = 3 列
///   minimum width  = 160px；3 × 160 + 2 × 16px gap = 512px
///   group count    = light(1) + dark(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App、进入 Canvas，并打开默认 Themes 页面。
///   2. 读取 Light/Dark 分组与前三张浅色主题卡的真实渲染 bounds。
///   3. 分别读取 Paper Blue 卡片内部 preview 与 choice row 的 bounds。
///   4. 比较三列位置、宽度、卡片高度，以及 preview/choice 的上下拼接关系。
///
/// 预期结果：
///   - 正断言：Light/Dark 两个分组均存在，三张浅色卡同排、等宽且顺序正确。
///   - 正断言：卡片精确为 214px 高，preview 为 150px，choice row 为 62px。
///   - 正断言：choice row 紧接 preview，并与卡片底边对齐。
///   - 负断言：preview 与 choice row 不得重叠，避免主题名重新压进预览画面。
#[test]
fn e2e_03_theme_cards_follow_reference_preview_and_choice_layout() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let temporary_root = tempfile::TempDir::new().expect("temporary workspace root should create");
    enter_canvas(&app, cx, temporary_root.path());

    click_debug(cx, "toolbar-settings");

    let light_group = required_bounds(cx, "settings-theme-group-light");
    let dark_group = required_bounds(cx, "settings-theme-group-dark");
    let paper = required_bounds(cx, "settings-theme-paper-blue");
    let linen = required_bounds(cx, "settings-theme-linen-sage");
    let silver = required_bounds(cx, "settings-theme-silver-violet");
    let preview = required_bounds(cx, "settings-theme-paper-blue-preview");
    let choice = required_bounds(cx, "settings-theme-paper-blue-choice");

    assert!(
        f32::from(light_group.origin.y) < f32::from(dark_group.origin.y),
        "Light 分组必须位于 Dark 分组上方"
    );
    assert_eq!(
        paper.origin.y, linen.origin.y,
        "Paper Blue 与 Linen Sage 应位于同一浅色主题行"
    );
    assert_eq!(
        linen.origin.y, silver.origin.y,
        "Linen Sage 与 Silver Violet 应位于同一浅色主题行"
    );
    assert!(
        paper.origin.x < linen.origin.x && linen.origin.x < silver.origin.x,
        "浅色主题卡应按 Paper Blue、Linen Sage、Silver Violet 从左到右排列"
    );
    assert!(
        (f32::from(paper.size.width) - f32::from(linen.size.width)).abs() <= 2.0,
        "Paper Blue 与 Linen Sage 的宽度差不得超过 2px 设备像素舍入"
    );
    assert!(
        (f32::from(linen.size.width) - f32::from(silver.size.width)).abs() <= 2.0,
        "Linen Sage 与 Silver Violet 的宽度差不得超过 2px 设备像素舍入"
    );
    assert!(
        f32::from(paper.size.width) >= 160.0,
        "三列主题卡的单列宽度不得小于 160px"
    );
    assert_eq!(
        f32::from(paper.size.height),
        214.0,
        "参考图式主题卡总高度必须为 2px border + 150px preview + 62px choice = 214px"
    );
    assert_eq!(
        f32::from(preview.size.height),
        150.0,
        "主题预览区域必须为 150px 高"
    );
    assert_eq!(
        f32::from(choice.size.height),
        62.0,
        "主题选择栏必须为 62px 高"
    );
    assert_eq!(
        f32::from(preview.origin.y),
        f32::from(paper.origin.y) + 1.0,
        "主题预览应在卡片顶部 1px 边框后开始"
    );
    assert_eq!(
        f32::from(choice.origin.y),
        f32::from(preview.origin.y) + f32::from(preview.size.height),
        "主题选择栏必须紧接 preview，不能与预览内容重叠"
    );
    assert_eq!(
        f32::from(choice.origin.y) + f32::from(choice.size.height),
        f32::from(paper.origin.y) + f32::from(paper.size.height) - 1.0,
        "主题选择栏底边必须在卡片底部 1px 边框前结束"
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

fn required_bounds(
    cx: &mut VisualTestContext,
    selector: &'static str,
) -> gpui::Bounds<gpui::Pixels> {
    cx.run_until_parked();
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("selector `{selector}` should have debug bounds"))
}
