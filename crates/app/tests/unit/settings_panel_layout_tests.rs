use super::*;

/// Settings 导航自适应：宽、中、窄窗口分别保留完整侧栏、紧凑侧栏和顶栏导航，内容区不被挤压为零宽。
///
/// 数据构造（含关键数值的推导过程）：
///   宽 viewport = 1280×900；常规边距 = 32px，面板可用宽度为 1216px。
///   中 viewport = 700×700；常规边距 = 32px，面板可用宽度 = 700 - 2×32 = 636px。
///   窄 viewport = 560×520；紧凑边距 = 12px，面板可用宽度 = 560 - 2×12 = 536px。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为三个 viewport 调用 `SettingsPanelLayout::for_viewport`。
///   2. 宽面板选择完整左侧导航，保留分组标题和文字标签。
///   3. 中面板选择紧凑左侧导航，收紧导航宽度但仍保留内容列。
///   4. 窄面板选择可横向滚动的顶栏导航，内容宽度回收为整个面板宽度。
///
/// 预期结果：
///   - 断言 A：三档 viewport 分别命中预期导航模式，避免窄窗口继续渲染挤压侧栏。
///   - 断言 B：宽、中、窄三档内容区宽度均为正，保证任何现有 Settings section 仍可交互。
///   - 断言 C：窄模式扣除 84px 顶栏后内容区高度仍为正，保证内容仍可滚动。
///   - 断言 D：窄模式不是侧栏，避免内容区被导航长期占用。
#[test]
fn test_settings_panel_layout_adapts_navigation_without_starving_content() {
    // 宽窗口：1280 - 2×32 = 1216px，低于 PANEL_MAX_W = 1280px。
    let wide = SettingsPanelLayout::for_viewport(1280.0, 900.0);
    // 中窗口：700 - 2×32 = 636px，位于紧凑侧栏区间。
    let medium = SettingsPanelLayout::for_viewport(700.0, 700.0);
    // 窄窗口：560 - 2×12 = 536px，必须改用顶栏以释放内容宽度。
    let narrow = SettingsPanelLayout::for_viewport(560.0, 520.0);

    assert_eq!(
        wide.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "宽面板应保留完整左侧导航，不能意外降级为紧凑导航"
    );
    assert_eq!(
        medium.navigation_mode,
        SettingsNavigationMode::CompactSidebar,
        "中等面板应使用紧凑侧栏，避免完整侧栏挤压内容"
    );
    assert_eq!(
        narrow.navigation_mode,
        SettingsNavigationMode::TopBar,
        "窄面板应把导航移到顶栏，保证内容区仍有可用宽度"
    );
    assert!(
        wide.content_width() > 0.0,
        "宽面板内容区必须为正宽，actual={}px",
        wide.content_width()
    );
    assert!(
        medium.content_width() > 0.0,
        "中面板内容区必须为正宽，actual={}px",
        medium.content_width()
    );
    assert!(
        narrow.content_width() > 0.0,
        "窄面板内容区必须为正宽，actual={}px",
        narrow.content_width()
    );
    assert!(
        narrow.content_height() > 0.0,
        "窄面板扣除顶栏后内容区必须为正高，actual={}px",
        narrow.content_height()
    );
    assert_ne!(
        narrow.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "窄面板不得继续占用完整侧栏，否则设置内容会被挤压"
    );
}

/// Product UI V2 大窗口 Settings：面板应接近 1280×820 玻璃窗口，同时保留 286px 稳定侧栏。
///
/// 数据构造（含关键数值的推导过程）：
///   viewport = 1440×960；常规外边距 = 32px
///   available = (1440 - 2×32) × (960 - 2×32) = 1376×896
///   target max = 1280×820，因此面板应收敛为 1280×820
///   sidebar = 1280×0.28 = 358.4px，再受 SIDEBAR_MAX_W=286px 上限约束
///   content width = 1280 - 286 = 994px > 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 SettingsPanelLayout::for_viewport(1440, 960)。
///   2. 读取面板宽高、侧栏宽度和内容宽度。
///   3. 对比旧 1180×760 上限，确认 V2 窗口有更充足的内容空间。
///
/// 预期结果：
///   - 正断言：面板为 1280×820，侧栏为 286px，内容宽度为 994px。
///   - 正断言：导航保持 Sidebar 模式。
///   - 负断言：面板不得停留在旧 1180×760 尺寸。
#[test]
fn test_settings_panel_large_viewport_matches_product_ui_v2_window() {
    let layout = SettingsPanelLayout::for_viewport(1440.0, 960.0);

    assert_eq!(
        layout.width, 1280.0,
        "1440px viewport 下 Settings 应使用 Product UI V2 的 1280px 最大宽度"
    );
    assert_eq!(
        layout.height, 820.0,
        "960px viewport 下 Settings 应使用 Product UI V2 的 820px 最大高度"
    );
    assert_eq!(
        layout.sidebar_width, 286.0,
        "大窗口 Settings 应使用稳定的 286px 侧栏"
    );
    assert_eq!(
        layout.content_width(),
        994.0,
        "content width 应等于 1280 - 286 = 994px"
    );
    assert_eq!(
        layout.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "大窗口必须保留完整 Sidebar 导航"
    );
    assert_ne!(
        (layout.width, layout.height),
        (1180.0, 760.0),
        "Settings 面板不得继续停留在旧 1180×760 上限"
    );
}
