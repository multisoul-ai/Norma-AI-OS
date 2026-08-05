//! Settings 导航分组刷新：以真实渲染树点击六个可配置入口，锁定业务路由不变。

use gpui::{px, size, Modifiers, TestAppContext, VisualTestContext};

use crate::harness::{echo_factory, enter_canvas, make_app};

/// Settings 任务导向导航：六个可配置入口能分别进入对应页面，且切换不会关闭面板。
///
/// 数据构造（含关键数值的推导过程）：
///   visible entries = 6（Themes + Wallpaper + Keyboard Shortcuts + Voice + Agents + Orchestration）。
///   hidden mock entries = 2（Runtime + Skills，不参与点击序列）。
///   click count = 6（每个已实现入口各一次，覆盖三个任务导向分组）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App 并进入 Canvas，点击 `toolbar-settings` 打开 Settings 面板。
///   2. 依次点击 Themes、Wallpaper、Keyboard、Voice、Agents 与 Orchestration 的稳定 selector。
///   3. 每次点击后读取公开 `SettingsE2eSnapshot`，确认 route 仍指向原 section。
///   4. 每次确认 Settings 仍保持打开，证明导航重构没有截断原有交互会话。
///
/// 预期结果：
///   - 断言 A：六个入口各自进入 Themes / Wallpaper / Keyboard Shortcuts / Voice / Agents / Orchestration。
///   - 断言 B：每一次切换后 Settings 都保持打开，不能被导航点击关闭。
///   - 断言 C：Orchestration 页面不得误路由回 Voice，防止分组 DOM 重排错绑点击处理器。
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

    click_debug(cx, "settings-section-orchestration");
    let orchestration = app.update(cx, |app, cx| app.settings_e2e_snapshot(cx));
    assert_eq!(
        orchestration.active_section, "Orchestration",
        "Orchestration 导航应进入代理安装与委派设置页，snapshot: {orchestration:?}"
    );
    assert!(
        orchestration.settings_open,
        "点击 Orchestration 后 Settings 应保持打开，snapshot: {orchestration:?}"
    );
    assert_ne!(
        orchestration.active_section, "Voice",
        "点击 Orchestration 后不得仍停留在 Voice 页面，snapshot: {orchestration:?}"
    );
}

/// Settings 菜单切换：所有已实现页面必须复用同一个外层面板宽高与位置。
///
/// 数据构造（含关键数值的推导过程）：
///   viewport = 1440×900px；常规边距 = 32px。
///   available width = 1440 - 2×32 = 1376px；受 PANEL_MAX_W=1280px 限制，面板宽度 = 1280px。
///   available height = 900 - 2×32 = 836px；受 PANEL_MAX_H=820px 限制，面板高度 = 820px。
///   visible sections = 6（Themes / Wallpaper / Keyboard / Voice / Agents / Orchestration）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 把 headless viewport 调整为 1440×900px，进入 Canvas 并打开 Settings。
///   2. 在 Themes 页面记录 `settings-panel` 的 1280×820px bounds 作为统一外壳基准。
///   3. 依次切换 Wallpaper、Keyboard、Voice、Agents 与 Orchestration，并读取每页外层 bounds。
///   4. 逐页比较 origin 与 size，确认内容差异只由统一滚动区承载，不改变浮层外壳。
///
/// 预期结果：
///   - 正断言：六个页面的 Settings panel 都保持 1280×820px，且 origin 完全一致。
///   - 负断言：Voice 不得使用更窄的页面专属宽度，任何其他菜单也不得移动或缩放外壳。
#[test]
fn e2e_03_settings_sections_keep_one_stable_panel_frame() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let temporary_root = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, temporary_root.path());

    // available = (1440 - 2×32) × (900 - 2×32) = 1376×836px；
    // panel = min(1376×836, 1280×820) = 1280×820px。
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    click_debug(cx, "toolbar-settings");

    click_debug(cx, "settings-section-themes");
    let themes = cx
        .debug_bounds("settings-panel")
        .expect("Themes 页面应渲染统一 settings-panel");
    assert_eq!(
        (f32::from(themes.size.width), f32::from(themes.size.height)),
        (1280.0, 820.0),
        "Themes 应建立 1280×820px 的统一 Settings 外壳基准"
    );

    click_debug(cx, "settings-section-wallpaper");
    let wallpaper = cx
        .debug_bounds("settings-panel")
        .expect("Wallpaper 页面应继续渲染统一 settings-panel");
    assert_eq!(
        wallpaper, themes,
        "Wallpaper 切页后 Settings origin 与 size 都必须保持不变"
    );

    click_debug(cx, "settings-section-keyboard");
    let keyboard = cx
        .debug_bounds("settings-panel")
        .expect("Keyboard 页面应继续渲染统一 settings-panel");
    assert_eq!(
        keyboard, themes,
        "Keyboard 切页后 Settings origin 与 size 都必须保持不变"
    );

    click_debug(cx, "settings-section-voice");
    let voice = cx
        .debug_bounds("settings-panel")
        .expect("Voice 页面应继续渲染统一 settings-panel");
    assert_eq!(
        voice, themes,
        "Voice 不得使用页面专属窄宽度；Settings origin 与 size 必须与 Themes 一致"
    );
    assert!(
        f32::from(voice.size.width) > 1020.0,
        "大 viewport 下 Voice 宽度不得回退到旧 1020px 上限，actual={}px",
        f32::from(voice.size.width)
    );

    click_debug(cx, "settings-section-agents");
    let agents = cx
        .debug_bounds("settings-panel")
        .expect("Agents 页面应继续渲染统一 settings-panel");
    assert_eq!(
        agents, themes,
        "Agents 切页后 Settings origin 与 size 都必须保持不变"
    );

    click_debug(cx, "settings-section-orchestration");
    let orchestration = cx
        .debug_bounds("settings-panel")
        .expect("Orchestration 页面应继续渲染统一 settings-panel");
    assert_eq!(
        orchestration, themes,
        "Orchestration 切页后 Settings origin 与 size 都必须保持不变"
    );
}

/// Settings 品牌脚注：软件 icon 与当前版本号应固定显示在大窗口侧栏左下角。
///
/// 数据构造（含关键数值的推导过程）：
///   viewport = 1440×900px；面板 = min((1440-2×32)×(900-2×32), 1280×820)
///            = 1280×820px，origin = ((1440-1280)/2, (900-820)/2) = (80, 40)。
///   sidebar width = min(1280×0.28, 286) = 286px。
///   app icon = 40×40px；品牌脚注必须完整位于 x=[80, 80+286] 的侧栏内。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动 headless App、进入 Canvas，并把 viewport 调整为 1440×900px。
///   2. 点击 `toolbar-settings` 打开 Settings，读取面板、品牌脚注、icon 与版本号 bounds。
///   3. 验证品牌脚注位于面板下半区且未越过 286px 侧栏边界。
///   4. 验证软件 icon 保持 40×40px，版本号与 icon 均被脚注容器包住。
///
/// 预期结果：
///   - 正断言：品牌脚注、软件 icon 与版本号三个用户可见元素同时存在。
///   - 正断言：脚注位于侧栏左下区域，icon 尺寸清晰且两项内容均在容器内。
///   - 负断言：脚注不得进入右侧内容区，也不得漂移到 Settings 上半区。
#[test]
fn e2e_03_settings_sidebar_shows_app_icon_and_version_at_bottom_left() {
    let mut cx = TestAppContext::single();
    let (app, cx) = make_app(&mut cx, echo_factory());
    let temporary_root = tempfile::TempDir::new().expect("测试应创建临时 workspace");
    enter_canvas(&app, cx, temporary_root.path());

    // available = (1440 - 2×32) × (900 - 2×32) = 1376×836px；
    // panel = min(1376×836, 1280×820) = 1280×820px，sidebar = 286px。
    cx.simulate_resize(size(px(1440.0), px(900.0)));
    cx.run_until_parked();
    click_debug(cx, "toolbar-settings");

    let panel = cx
        .debug_bounds("settings-panel")
        .expect("打开 Settings 后应渲染统一 settings-panel");
    let footer = cx
        .debug_bounds("settings-brand-footer")
        .expect("Settings 侧栏左下角应渲染品牌脚注");
    let icon = cx
        .debug_bounds("settings-brand-icon")
        .expect("Settings 品牌脚注应渲染当前软件 icon");
    let version = cx
        .debug_bounds("settings-brand-version")
        .expect("Settings 品牌脚注应渲染当前软件版本号");

    assert_eq!(
        icon.size,
        size(px(40.0), px(40.0)),
        "Settings 软件 icon 应保持 40×40px 的清晰尺寸"
    );
    assert!(
        f32::from(footer.origin.y) > f32::from(panel.origin.y) + f32::from(panel.size.height) * 0.5,
        "品牌脚注必须位于 Settings 面板下半区，panel={panel:?}, footer={footer:?}"
    );
    assert!(
        f32::from(footer.origin.x + footer.size.width) <= f32::from(panel.origin.x) + 286.0,
        "品牌脚注不得越过 286px 侧栏进入右侧内容区，panel={panel:?}, footer={footer:?}"
    );
    assert!(
        footer.contains(&icon.center()),
        "软件 icon 必须完整位于品牌脚注容器内，footer={footer:?}, icon={icon:?}"
    );
    assert!(
        footer.contains(&version.center()),
        "版本号必须完整位于品牌脚注容器内，footer={footer:?}, version={version:?}"
    );
    assert!(
        f32::from(footer.origin.y) < f32::from(panel.origin.y + panel.size.height),
        "品牌脚注不得漂移到 Settings 面板底边之外，panel={panel:?}, footer={footer:?}"
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
