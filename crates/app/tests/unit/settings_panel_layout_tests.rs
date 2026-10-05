use super::*;

/// 大窗口 Settings 约占视口面积的四分之一，并在视口中居中。
///
/// 数据构造（含关键数值的推导过程）：
///   viewport = 1920×1200；宽高都达到常规边距阈值（640×520），margin = 32px。
///   available = (1920 - 64) × (1200 - 64) = 1856×1136。
///   panel = viewport × 0.5 = 960×600；960≤1856 且 600≤1136，最小尺寸 520×400 不抬高。
///   origin = ((1920-960)/2, (1200-600)/2) = (480, 300)。
///   area ratio = (960×600) / (1920×1200) = 576000 / 2304000 = 0.25。
///   panel width 960 ≥ 760 → Sidebar。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 SettingsPanelLayout::for_viewport(1920, 1200)。
///   2. 读取面板宽高、原点、面积比和导航模式。
///   3. 与旧的 1280×820 固定上限比较。
///
/// 预期结果：
///   - 正断言：面板为 960×600，原点 (480, 300)，面积比 0.25，导航为 Sidebar。
///   - 负断言：面板不得继续使用旧的 1280×820 上限。
#[test]
fn test_settings_panel_large_viewport_occupies_quarter_area_and_stays_centered() {
    let layout = SettingsPanelLayout::for_viewport(1920.0, 1200.0);
    let area_ratio = (layout.width * layout.height) / (1920.0 * 1200.0);

    assert_eq!(
        layout.width, 960.0,
        "1920px 宽视口的面板宽度应为 1920×0.5=960px"
    );
    assert_eq!(
        layout.height, 600.0,
        "1200px 高视口的面板高度应为 1200×0.5=600px"
    );
    assert_eq!(layout.left, 480.0, "水平居中：(1920-960)/2=480px");
    assert_eq!(layout.top, 300.0, "垂直居中：(1200-600)/2=300px");
    assert_eq!(
        area_ratio, 0.25,
        "面板面积应为视口面积的四分之一，actual={area_ratio}"
    );
    assert_eq!(
        layout.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "960px 面板宽度高于 760px 断点，应保留完整 Sidebar"
    );
    assert!(
        layout.content_width() > 0.0,
        "Sidebar 内容区必须为正宽，actual={}px",
        layout.content_width()
    );
    assert_ne!(
        (layout.width, layout.height),
        (1280.0, 820.0),
        "Settings 面板不得继续停在旧的 1280×820 上限"
    );
}

/// 导航仍按面板宽度在完整侧栏、紧凑侧栏和顶栏之间切换，内容区保持可滚动。
///
/// 数据构造（含关键数值的推导过程）：
///   三档视口的宽高都高于紧凑边距阈值，margin = 32px，available 都大于半视口，因此面板取 0.5 比例。
///   1600×1000 → panel = 800×500；800 ≥ 760 → Sidebar。
///     sidebar = clamp(800×0.28, 160, 286) = 224；content width = 800-224 = 576。
///   1440×900 → panel = 720×450；580 ≤ 720 < 760 → CompactSidebar。
///     sidebar = clamp(720×0.26, 144, 188) ≈ 187.2；content width ≈ 532.8。
///   1100×800 → panel width = max(1100×0.5, 520) = 550，height = max(800×0.5, 400) = 400。
///     550 < 580 → TopBar；content width = 550；content height = 400-84 = 316。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 1600×1000、1440×900、1100×800 分别计算布局。
///   2. 读取导航模式、内容宽高。
///   3. 确认窄面板不再占用侧栏。
///
/// 预期结果：
///   - 正断言：三档分别命中 Sidebar、CompactSidebar、TopBar，且内容宽高为正。
///   - 负断言：1100px 宽视口不得继续使用 Sidebar。
#[test]
fn test_settings_panel_layout_adapts_navigation_without_starving_content() {
    let wide = SettingsPanelLayout::for_viewport(1600.0, 1000.0);
    let medium = SettingsPanelLayout::for_viewport(1440.0, 900.0);
    let narrow = SettingsPanelLayout::for_viewport(1100.0, 800.0);

    assert_eq!(wide.width, 800.0, "1600px 视口面板宽度应为 800px");
    assert_eq!(wide.height, 500.0, "1000px 视口面板高度应为 500px");
    assert_eq!(
        wide.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "800px 面板应使用完整 Sidebar"
    );
    assert!(
        wide.content_width() > 400.0,
        "完整侧栏后内容区应明显可用，actual={}px",
        wide.content_width()
    );

    assert_eq!(medium.width, 720.0, "1440px 视口面板宽度应为 720px");
    assert_eq!(medium.height, 450.0, "900px 视口面板高度应为 450px");
    assert_eq!(
        medium.navigation_mode,
        SettingsNavigationMode::CompactSidebar,
        "720px 面板位于 580–760 区间，应使用紧凑侧栏"
    );
    assert!(
        medium.content_width() > 400.0,
        "紧凑侧栏后内容区应明显可用，actual={}px",
        medium.content_width()
    );

    assert_eq!(
        narrow.width, 550.0,
        "1100px 视口面板宽度应为 max(550, 520)=550px"
    );
    assert_eq!(
        narrow.height, 400.0,
        "800×0.5=400，与最小高度相同，面板高度应为 400px"
    );
    assert_eq!(
        narrow.navigation_mode,
        SettingsNavigationMode::TopBar,
        "550px 面板低于 580px 断点，应改用顶栏"
    );
    assert_eq!(
        narrow.content_width(),
        550.0,
        "顶栏模式应把整个面板宽度还给内容区"
    );
    assert_eq!(
        narrow.content_height(),
        316.0,
        "顶栏扣除 84px 后内容高度应为 400-84=316px"
    );
    assert_ne!(
        narrow.navigation_mode,
        SettingsNavigationMode::Sidebar,
        "窄面板不得继续占用完整侧栏"
    );
}

/// 半视口低于最小尺寸时抬到 520×400；极小视口则收进可用区域，面板不越界。
///
/// 数据构造（含关键数值的推导过程）：
///   最小尺寸视口 = 1000×760；margin = 32px（宽高都达到常规边距阈值）。
///   half = 500×380；available = (1000-64)×(760-64) = 936×696。
///   panel = max(500, 520).min(936) × max(380, 400).min(696) = 520×400。
///   origin = ((1000-520)/2, (760-400)/2) = (240, 180)。
///   极小视口 = 120×100；margin = 12px。
///   available = (120-24)×(100-24) = 96×76。
///   panel = max(60, 520).min(96) × max(50, 400).min(76) = 96×76。
///   origin = ((120-96)/2, (100-76)/2) = (12, 12)。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 计算 1000×760，确认最小宽高在可用区域内生效。
///   2. 计算 120×100，确认最小尺寸不会把面板推出视口。
///   3. 比较右缘和底缘是否落在视口内。
///
/// 预期结果：
///   - 正断言：有空间时面板至少 520×400，并保持居中。
///   - 正断言：极小视口面板等于可用区域，四边留在视口内。
///   - 负断言：极小视口不得使用 520×400，否则会溢出 120×100。
#[test]
fn test_settings_panel_layout_respects_minimums_and_tiny_viewport_bounds() {
    let fitted = SettingsPanelLayout::for_viewport(1000.0, 760.0);

    assert_eq!(fitted.width, 520.0, "半宽 500px 应抬到最小宽度 520px");
    assert_eq!(fitted.height, 400.0, "半高 380px 应抬到最小高度 400px");
    assert_eq!(
        fitted.left, 240.0,
        "最小宽度面板应水平居中：(1000-520)/2=240px"
    );
    assert_eq!(
        fitted.top, 180.0,
        "最小高度面板应垂直居中：(760-400)/2=180px"
    );
    assert!(
        fitted.left + fitted.width <= 1000.0 && fitted.top + fitted.height <= 760.0,
        "最小尺寸面板不得越过 1000×760 视口，layout={fitted:?}"
    );
    assert!(
        fitted.content_width() > 0.0,
        "最小宽度面板仍须留下正的内容宽度，actual={}px",
        fitted.content_width()
    );

    let tiny = SettingsPanelLayout::for_viewport(120.0, 100.0);

    assert_eq!(
        tiny.width, 96.0,
        "120px 视口可用宽度为 120-2×12=96px，不得使用 520px 最小宽度"
    );
    assert_eq!(
        tiny.height, 76.0,
        "100px 视口可用高度为 100-2×12=76px，不得使用 400px 最小高度"
    );
    assert_eq!(tiny.left, 12.0, "极小视口应保留 12px 左边距");
    assert_eq!(tiny.top, 12.0, "极小视口应保留 12px 上边距");
    assert!(
        tiny.left >= 0.0 && tiny.top >= 0.0,
        "极小视口原点不得为负，layout={tiny:?}"
    );
    assert!(
        tiny.left + tiny.width <= 120.0 && tiny.top + tiny.height <= 100.0,
        "极小视口面板不得溢出右缘或底缘，layout={tiny:?}"
    );
    assert_ne!(
        (tiny.width, tiny.height),
        (520.0, 400.0),
        "极小视口不得强行使用 520×400，否则会超出 120×100"
    );
}
