use super::*;

/// 场景：大窗口按每边 70% 计算后受上限约束，面板仍居中。
/// 数据构造：1920×0.7=1344、1200×0.7=840，分别被 1280×820 上限截住；
/// left=(1920-1280)/2=320、top=(1200-820)/2=190；侧栏 1280×0.28=358.4 → 上限 286。
/// 执行过程：计算 1920×1200 和 3840×2160 的面板布局，比较尺寸、位置和导航。
/// 预期结果：两种大屏均为 1280×820；常规大窗口内容宽度 1280-286=994，不回退为半屏。
#[test]
fn test_settings_panel_layout_caps_large_viewports_and_centers() {
    let layout = SettingsPanelLayout::for_viewport(1920.0, 1200.0);
    let huge = SettingsPanelLayout::for_viewport(3840.0, 2160.0);

    assert_eq!(
        (layout.width, layout.height),
        (1280.0, 820.0),
        "大窗口面板应被最大尺寸截住"
    );
    assert_eq!(
        (layout.left, layout.top),
        (320.0, 190.0),
        "面板应水平、垂直居中"
    );
    assert_eq!(
        (huge.width, huge.height),
        (1280.0, 820.0),
        "超大屏不得继续放大面板"
    );
    assert_eq!(
        layout.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "1280px 宽面板保持完整侧栏"
    );
    assert_eq!(
        layout.content_width(),
        994.0,
        "侧栏最大 286px，内容宽度为 994px"
    );
    assert_ne!(
        (layout.width, layout.height),
        (960.0, 600.0),
        "不得回退到 viewport 每边 50%"
    );
}

/// 场景：1280×800 桌面基准使用每边 70% 的面板，无最小/最大值干预。
/// 数据构造：1280×0.7=896、800×0.7=560；760<896<1280，520<560<820；
/// left=(1280-896)/2=192、top=(800-560)/2=120；sidebar=896×0.28=250.88。
/// 执行过程：计算桌面基准布局，比较尺寸、居中位置及内容宽度。
/// 预期结果：面板为 896×560、内容宽度 645.12；不得仍为旧 640×400。
#[test]
fn test_settings_panel_layout_uses_seventy_percent_on_desktop() {
    let layout = SettingsPanelLayout::for_viewport(1280.0, 800.0);

    assert_eq!(
        (layout.width, layout.height),
        (896.0, 560.0),
        "基准桌面每边应占 70%"
    );
    assert_eq!(
        (layout.left, layout.top),
        (192.0, 120.0),
        "基准桌面面板应居中"
    );
    assert!(
        (layout.content_width() - 645.12).abs() < 1e-3,
        "内容宽度应为 896-250.88=645.12px"
    );
    assert_eq!(
        layout.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "896px 使用完整侧栏"
    );
    assert_ne!(
        (layout.width, layout.height),
        (640.0, 400.0),
        "不得保留拥挤的旧桌面尺寸"
    );
}

/// 场景：保持 760/580px 导航断点，小窗口在 available 限制下仍有内容区。
/// 数据构造：1600×1000 → 1120×700（Sidebar）；800×700 → available 宽 800-64=736，
/// 最小尺寸被截为 736×520（CompactSidebar）；574×700 → margin=12，面板 550×520（TopBar）。
/// 执行过程：计算三种布局并检查导航模式、内容宽高。
/// 预期结果：Sidebar/CompactSidebar/TopBar 三档保留，TopBar 内容高 520-84=436，侧栏宽 0。
#[test]
fn test_settings_panel_layout_adapts_navigation_without_starving_content() {
    let wide = SettingsPanelLayout::for_viewport(1600.0, 1000.0);
    let medium = SettingsPanelLayout::for_viewport(800.0, 700.0);
    let narrow = SettingsPanelLayout::for_viewport(574.0, 700.0);

    assert_eq!(
        wide.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "1120px 使用完整侧栏"
    );
    assert_eq!(
        medium.navigation_mode,
        SettingsNavigationMode::CompactSidebar,
        "736px 使用紧凑侧栏"
    );
    assert_eq!(
        narrow.navigation_mode,
        SettingsNavigationMode::TopBar,
        "550px 使用顶栏导航"
    );
    assert_eq!(
        (wide.width, medium.width, narrow.width),
        (1120.0, 736.0, 550.0),
        "面板宽度须遵守比例及 available 限制"
    );
    for (name, layout) in [("宽", wide), ("中", medium), ("窄", narrow)] {
        assert!(
            layout.content_width() > 0.0 && layout.content_height() > 0.0,
            "{name}面板必须保留正的内容宽高"
        );
    }
    assert_eq!(narrow.content_height(), 436.0, "顶栏扣除 84px 后应剩 436px");
    assert_eq!(narrow.sidebar_width, 0.0, "顶栏不得占用侧栏宽度");
}

/// 场景：70% 尺寸低于最小值时提升到 760×520，available 小于最小值时优先保留边距。
/// 数据构造：900×700 → 630×490 → 760×520，left=70、top=90；
/// 800×550 → available=736×486 → 面板 736×486，left=top=32；
/// 500×380 → available=476×356 → 面板 476×356，left=top=12。
/// 执行过程：计算三种小屏布局，比较尺寸、位置与导航并检查边界。
/// 预期结果：最小值生效且可用区域优先；小屏不得溢出窗口，分别使用 Sidebar/CompactSidebar/TopBar。
#[test]
fn test_settings_panel_layout_respects_minimum_and_viewport_bounds() {
    let small = SettingsPanelLayout::for_viewport(900.0, 700.0);
    let constrained = SettingsPanelLayout::for_viewport(800.0, 550.0);
    let tiny = SettingsPanelLayout::for_viewport(500.0, 380.0);

    assert_eq!(
        (small.width, small.height),
        (760.0, 520.0),
        "比例过小时应提升到最小尺寸"
    );
    assert_eq!(
        (small.left, small.top),
        (70.0, 90.0),
        "提升到最小值后仍居中"
    );
    assert_eq!(
        (constrained.width, constrained.height),
        (736.0, 486.0),
        "available 优先于最小尺寸"
    );
    assert_eq!(
        (constrained.left, constrained.top),
        (32.0, 32.0),
        "保留常规边距"
    );
    assert_eq!(
        (tiny.width, tiny.height),
        (476.0, 356.0),
        "极小屏保留 12px 边距"
    );
    assert_eq!((tiny.left, tiny.top), (12.0, 12.0), "极小屏仍居中");
    for (width, height, layout) in [(800.0, 550.0, constrained), (500.0, 380.0, tiny)] {
        assert!(
            layout.left + layout.width <= width && layout.top + layout.height <= height,
            "受限面板不得超出 viewport"
        );
    }
    assert_eq!(
        (
            small.navigation_mode,
            constrained.navigation_mode,
            tiny.navigation_mode
        ),
        (
            SettingsNavigationMode::Sidebar,
            SettingsNavigationMode::CompactSidebar,
            SettingsNavigationMode::TopBar
        ),
        "导航仍按既有面板宽度断点选择"
    );
}
