use super::*;

/// 设置面板默认态：新建状态应关闭，并把 General 作为首次打开的默认分组。
///
/// 数据构造（含关键数值的推导过程）：
///   initial open = false（启动后不应遮挡画布）
///   section count = 4（General / Runtime / Agents / Skills）
///   default section = General（计划指定默认打开 General）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 SettingsPanelState::default() 构造初始状态
///   2. 读取 open 与 active_section
///   3. 确认默认分组不是 Runtime，避免首次打开落到非 General 页面
///
/// 预期结果：
///   - 正断言：open 为 false，active_section 为 General
///   - 负断言：active_section 不应为 Runtime
#[test]
fn test_settings_panel_defaults_closed_on_general() {
    let panel = SettingsPanelState::default();

    assert!(
        !panel.open,
        "设置面板默认应关闭，否则启动进入 Canvas 会立即遮挡画布"
    );
    assert_eq!(
        panel.active_section,
        SettingsSection::General,
        "设置面板默认分组应是 General"
    );
    assert_ne!(
        panel.active_section,
        SettingsSection::Runtime,
        "设置面板首次打开不应落到 Runtime 分组"
    );
}

/// 设置面板开关：open / close / toggle 应只改变可见性，不重置当前分组。
///
/// 数据构造（含关键数值的推导过程）：
///   initial open = false
///   selected section = Agents（先切到非默认分组，用来验证关闭不会重置选择）
///   toggle count = 2（第一次打开，第二次关闭）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 默认状态 select(Agents) → active_section = Agents
///   2. open() → open = true
///   3. close() → open = false，active_section 仍是 Agents
///   4. toggle() 两次 → true 再 false
///
/// 预期结果：
///   - 正断言：open / close / toggle 按预期改变 open
///   - 正断言：关闭后 Agents 选择保留
///   - 负断言：关闭不应把 active_section 重置为 General
#[test]
fn test_settings_panel_open_close_toggle_preserve_section() {
    let mut panel = SettingsPanelState::default();

    panel.select(SettingsSection::Agents);
    panel.open();
    assert!(panel.open, "open() 应把设置面板设为可见");

    panel.close();
    assert!(!panel.open, "close() 应把设置面板设为隐藏");
    assert_eq!(
        panel.active_section,
        SettingsSection::Agents,
        "关闭设置面板后应保留用户当前选择的 Agents 分组"
    );
    assert_ne!(
        panel.active_section,
        SettingsSection::General,
        "关闭设置面板不应把分组重置回 General"
    );

    panel.toggle();
    assert!(panel.open, "第一次 toggle() 应打开隐藏的设置面板");
    panel.toggle();
    assert!(!panel.open, "第二次 toggle() 应关闭打开的设置面板");
}

/// 设置面板分组选择：点击 Runtime / Skills 应切换右侧内容并保持弹窗打开。
///
/// 数据构造（含关键数值的推导过程）：
///   section count = 4（General / Runtime / Agents / Skills）
///   initial open = true（模拟用户已经打开设置弹窗）
///   target sequence = Runtime → Skills（覆盖两个非默认分组）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. open() 打开设置面板
///   2. select(Runtime) → active_section = Runtime，open 仍为 true
///   3. select(Skills) → active_section = Skills，open 仍为 true
///
/// 预期结果：
///   - 正断言：Runtime 与 Skills 都能成为 active_section
///   - 正断言：切换分组不会关闭弹窗
///   - 负断言：选择 Skills 后不应仍停留在 Runtime
#[test]
fn test_settings_panel_selects_sections_without_closing() {
    let mut panel = SettingsPanelState::default();
    panel.open();

    panel.select(SettingsSection::Runtime);
    assert_eq!(
        panel.active_section,
        SettingsSection::Runtime,
        "选择 Runtime 后右侧内容应切到 Runtime 分组"
    );
    assert!(panel.open, "选择 Runtime 不应关闭设置弹窗");

    panel.select(SettingsSection::Skills);
    assert_eq!(
        panel.active_section,
        SettingsSection::Skills,
        "选择 Skills 后右侧内容应切到 Skills 分组"
    );
    assert!(panel.open, "选择 Skills 不应关闭设置弹窗");
    assert_ne!(
        panel.active_section,
        SettingsSection::Runtime,
        "选择 Skills 后不应继续停留在 Runtime 分组"
    );
}

/// General 页面还原锚点：设置面板应包含图一中的 Appearance 风格结构与关键文案。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings_panel.rs + settings_content.rs + settings_appearance.rs
///            （检查生产渲染源码与拆分后的内容模块）
///   nav anchors = General + Language & Region + Appearance + Date & Time + Keyboard Shortcuts
///   content anchors = Themes + Light Mode + Dark Mode + System Preferences + Accent Colors
///   lower anchors = Show Animations + Tables View
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 settings_panel.rs 生产源码
///   2. 检查左侧 General 展开子项是否存在
///   3. 检查右侧 Appearance 页面关键模块是否存在
///
/// 预期结果：
///   - 正断言：图一中的导航与内容锚点都存在
///   - 负断言：General 页面不应只停留在 Workspace Path 这类旧泛化 mock 内容
#[test]
fn test_general_settings_panel_contains_reference_appearance_anchors() {
    let source = format!(
        "{}{}{}",
        include_str!("../../src/settings_panel.rs"),
        include_str!("../../src/settings_content.rs"),
        include_str!("../../src/settings_appearance.rs")
    );
    let nav_anchors = [
        "Language & Region",
        "Appearance",
        "Date & Time",
        "Keyboard Shortcuts",
    ];
    let content_anchors = [
        "Themes",
        "Light Mode",
        "Dark Mode",
        "System Preferences",
        "Accent Colors",
        "Show Animations",
        "Tables View",
    ];

    for anchor in nav_anchors {
        assert!(
            source.contains(anchor),
            "General 左侧展开导航应包含图一锚点: {anchor}"
        );
    }
    for anchor in content_anchors {
        assert!(
            source.contains(anchor),
            "General 右侧内容应包含图一 Appearance 锚点: {anchor}"
        );
    }
    assert!(
        !source.contains("Workspace Path"),
        "General 页面不应继续以 Workspace Path 泛化 mock 作为主要内容"
    );
}

/// 设置面板主题色：Settings 视觉应跟随重点卡片的暗色玻璃 token，而不是独立白色面板。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings_panel.rs + settings_content.rs + settings_appearance.rs
///   required tokens = panel / panel_high / node_fill / term_bg
///                     （来自重点卡片的暗色玻璃底色层级）
///   required accents = accent_orange / accent_cyan / accent_violet
///                      （当前项目选中、聚焦与强调色）
///   banned colors = 0xfffffff8 / 0xfbfbfdff / 0xe9e9efff / 0x24213dff
///                   （上一版参考图白色 Settings 面板残留）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 拼接 Settings 生产渲染源码
///   2. 检查主题层级是否改为项目 token 驱动
///   3. 检查旧白底/深蓝文字常量是否被移除
///
/// 预期结果：
///   - 正断言：Settings 源码引用重点卡片使用的主题 token 与强调色
///   - 负断言：Settings 源码不应继续保留上一版白色面板硬编码常量
#[test]
fn test_settings_panel_uses_card_theme_tokens_not_light_mock_palette() {
    let source = format!(
        "{}{}{}",
        include_str!("../../src/settings_panel.rs"),
        include_str!("../../src/settings_content.rs"),
        include_str!("../../src/settings_appearance.rs")
    );
    let required_tokens = [
        "theme.panel",
        "theme.panel_high",
        "theme.node_fill",
        "theme.term_bg",
        "theme.text_primary",
        "theme.text_secondary",
        "theme.text_tertiary",
        "theme.accent_orange",
        "theme.accent_cyan",
        "theme.accent_violet",
    ];
    let banned_light_palette = ["0xfffffff8", "0xfbfbfdff", "0xe9e9efff", "0x24213dff"];

    for token in required_tokens {
        assert!(
            source.contains(token),
            "Settings 主题色应参考重点卡片主题 token: {token}"
        );
    }
    for color in banned_light_palette {
        assert!(
            !source.contains(color),
            "Settings 不应继续保留上一版白色 mock 调色常量: {color}"
        );
    }
}
