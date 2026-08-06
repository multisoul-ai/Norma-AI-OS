use super::*;

/// 设置面板默认态：新建状态应关闭，并把 Themes 作为首次打开的可配置分组。
///
/// 数据构造（含关键数值的推导过程）：
///   initial open = false（启动后不应遮挡画布）
///   visible section count = 7（Themes / Wallpaper / Performance / Keyboard / Voice / Agents / Orchestration）
///   hidden mock count = 2（Runtime / Skills，不应作为首次页面）
///   default section = Themes（Appearance 中首个已实现的可配置页面）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 调用 SettingsPanelState::default() 构造初始状态
///   2. 读取 open 与 active_section
///   3. 确认默认分组不是 Wallpaper，证明主题切换是首个 Appearance 入口
///
/// 预期结果：
///   - 正断言：open 为 false，active_section 为 Themes
///   - 负断言：active_section 不应为 Wallpaper
#[test]
fn test_settings_panel_defaults_closed_on_first_visible_section() {
    let panel = SettingsPanelState::default();

    assert!(
        !panel.open,
        "设置面板默认应关闭，否则启动进入 Canvas 会立即遮挡画布"
    );
    assert_eq!(
        panel.active_section,
        SettingsSection::Themes,
        "设置面板默认分组应是首个可配置的 Themes 页面"
    );
    assert_ne!(
        panel.active_section,
        SettingsSection::Wallpaper,
        "Themes 已实现后，首次打开不应继续落到 Wallpaper"
    );
}

/// Settings 外壳的两个不透明子面必须各自绘制外侧圆角，不能只依赖父级矩形 overflow 裁剪。
///
/// 数据构造（含关键数值推导）：
///   panel radius = 24px；Sidebar 模式下 navigation 位于左侧、content 位于右侧；
///   TopBar 模式下 navigation 位于顶部、content 位于底部。
///   GPUI 0.2.2 的 ContentMask 为矩形，因此子面若无自身 24px 外侧圆角，会填平方角。
///
/// 执行过程：
///   1. 读取 Settings panel 生产渲染源码。
///   2. 检查 Sidebar/CompactSidebar 为 navigation 设置 left radius、content 设置 right radius。
///   3. 检查 TopBar 为 navigation 设置 top radius、content 设置 bottom radius。
///
/// 预期结果：
///   - 正断言：四种外侧圆角修饰均存在，所有响应式布局都保持 24px 外轮廓。
///   - 负断言：不应只依赖 settings-panel 的 overflow_hidden + rounded 组合裁剪子面。
#[test]
fn test_settings_panel_children_own_their_outer_corner_radii() {
    let source = include_str!("../../src/settings/panel.rs");

    assert!(
        source.contains("navigation.rounded_l(px(24.0))")
            && source.contains("content.rounded_r(px(24.0))"),
        "侧栏布局必须让 navigation/content 分别绘制左侧和右侧 24px 圆角"
    );
    assert!(
        source.contains("navigation.rounded_t(px(24.0))")
            && source.contains("content.rounded_b(px(24.0))"),
        "顶栏布局必须让 navigation/content 分别绘制顶部和底部 24px 圆角"
    );
    assert!(
        !source.contains(".child(settings_navigation("),
        "Settings 不得直接挂载未修饰的 navigation，否则又会只依赖父级矩形裁剪"
    );
}

/// 设置面板响应式布局：小窗口和极窄窗口都不应把面板或侧栏推出 viewport。
///
/// 数据构造（含关键数值的推导过程）：
///   常见小窗口 = 420×360；紧凑边距 = 12px，因此面板 = 396×336。
///   极窄窗口 = 120×100；紧凑边距 = 12px，因此面板 = 96×76，侧栏上限 = 96×45%=43.2px。
///
/// 执行过程：分别计算两个 viewport 的纯布局，并读取面板四边与侧栏宽度。
///
/// 预期结果：面板四边均在 viewport 内；侧栏小于面板宽度且为内容保留至少 55% 宽度。
#[test]
fn test_settings_panel_layout_stays_inside_small_viewports() {
    let small = SettingsPanelLayout::for_viewport(420.0, 360.0);
    assert_eq!(small.left, 12.0, "420px 宽窗口应保留 12px 左边距");
    assert_eq!(small.top, 12.0, "360px 高窗口应保留 12px 上边距");
    assert_eq!(small.width, 396.0, "面板宽度应为 420-2×12=396px");
    assert_eq!(small.height, 336.0, "面板高度应为 360-2×12=336px");
    assert!(
        small.left + small.width <= 420.0 && small.top + small.height <= 360.0,
        "小窗口面板不得越过 viewport 右边或底边，layout={small:?}"
    );

    let narrow = SettingsPanelLayout::for_viewport(120.0, 100.0);
    assert!(
        narrow.left + narrow.width <= 120.0 && narrow.top + narrow.height <= 100.0,
        "极窄窗口面板仍不得溢出 viewport，layout={narrow:?}"
    );
    assert!(
        narrow.sidebar_width <= narrow.width * 0.45,
        "侧栏最多占面板 45%，应为内容保留至少 55%，layout={narrow:?}"
    );
}

/// Keyboard Shortcuts 页面：Settings 应把侧栏占位升级为真实可点击页面，并渲染可编辑命令与只读输入行为。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings_panel.rs + settings_content.rs + settings_keyboard.rs + shortcuts.rs
///   editable anchors = settings-shortcut-new-terminal / settings-shortcut-toggle-settings
///                      / settings-shortcut-delete-card / settings-shortcut-voice-interaction
///   read-only anchors = Paste / Arrow Navigation / Space Pan / Terminal Input
///   restore anchors = settings-shortcuts-restore-all / settings-shortcut-restore-delete-card / Restore
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 拼接 Settings 生产渲染源码
///   2. 检查 Keyboard 分组、页面标题、可编辑行、只读行与恢复入口
///   3. 检查 Keyboard Shortcuts 不再以 sidebar_child disabled 占位形式存在
///
/// 预期结果：
///   - 正断言：Keyboard Shortcuts 页面关键锚点都存在
///   - 正断言：SettingsSection::Keyboard 可被 select
///   - 负断言：Keyboard Shortcuts 不应仍是 disabled sidebar_child
#[test]
fn test_keyboard_shortcuts_settings_page_contains_editable_and_read_only_rows() {
    let source = format!(
        "{}{}{}{}{}{}",
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/navigation.rs"),
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/keyboard.rs"),
        include_str!("../../src/shortcuts.rs"),
        include_str!("../../src/shortcuts/catalog.rs")
    );
    let required = [
        "SettingsSection::Keyboard",
        "Keyboard Shortcuts",
        "Customize app commands and review system input behavior",
        "settings-shortcut-new-terminal",
        "settings-shortcut-toggle-settings",
        "settings-shortcut-delete-card",
        "settings-shortcut-restore-delete-card",
        "settings-shortcut-new-claude-agent",
        "settings-shortcut-new-codex-agent",
        "settings-shortcut-new-cursor-agent",
        "settings-shortcut-new-opencode-agent",
        "settings-shortcut-voice-interaction",
        "settings-shortcut-restore-voice-interaction",
        "settings-shortcuts-restore-all",
        "Paste",
        "Voice Interaction",
        "Arrow Navigation",
        "Space Pan",
        "Terminal Input",
        "Modified",
        "Restore",
    ];

    for anchor in required {
        assert!(
            source.contains(anchor),
            "Keyboard Shortcuts 设置页应包含关键锚点: {anchor}"
        );
    }
    assert!(
        !source.contains("sidebar_child(theme, \"Keyboard Shortcuts\", false)"),
        "Keyboard Shortcuts 不应继续是 disabled sidebar_child 占位"
    );
}

/// Orchestration 页面锚点：Settings 应提供四组件状态和 Provider 安装入口。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings_panel.rs + settings_content.rs + settings_orchestration.rs
///   nav anchor = Orchestration
///   action anchors = Codex / Claude / Cursor / OpenCode 四个 Provider action id
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 拼接 Settings 生产渲染源码。
///   2. 检查 Orchestration 导航、四组件和 Provider action id。
///
/// 预期结果：
///   - 正断言：Orchestration 与五个 Provider、四组件、安装/修复/卸载操作都存在。
///   - 负断言：不得保留第二个 Agent Hooks 导航或直接前台安装入口。
#[test]
fn test_orchestration_settings_panel_contains_provider_components_and_actions() {
    let source = format!(
        "{}{}{}{}",
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/navigation.rs"),
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/orchestration.rs")
    );
    let anchors = [
        "SettingsSection::Orchestration",
        "Orchestration",
        "settings-orchestration-codex-action",
        "settings-orchestration-claude-action",
        "settings-orchestration-cursor-action",
        "settings-orchestration-opencode-action",
        "Skill",
        "Norma CLI",
        "Agent Hook",
        "Authorization",
        "Install",
        "Repair",
        "Uninstall",
        "Unavailable",
    ];

    for anchor in anchors {
        assert!(
            source.contains(anchor),
            "Orchestration 设置页应包含组件/操作锚点: {anchor}"
        );
    }
    assert!(
        source.contains("Restart existing agent sessions after installation."),
        "Orchestration 页面必须固定提示重启已有 Agent 会话"
    );
    assert!(
        source.contains("Orchestration is not supported in the first release"),
        "Grok 行必须明确说明首版不支持编排"
    );
    assert!(
        !source.contains("SettingsSection::Hooks"),
        "迁移后不得继续提供第二个 Agent Hooks 导航入口"
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
///   - 负断言：关闭不应把 active_section 重置为 Themes
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
        SettingsSection::Themes,
        "关闭设置面板不应把用户选择重置回 Themes"
    );

    panel.toggle();
    assert!(panel.open, "第一次 toggle() 应打开隐藏的设置面板");
    panel.toggle();
    assert!(!panel.open, "第二次 toggle() 应关闭打开的设置面板");
}

/// 设置面板分组选择：点击 Runtime / Voice / Skills 应切换右侧内容并保持弹窗打开。
///
/// 数据构造（含关键数值的推导过程）：
///   section count = 8（Themes / Wallpaper / Keyboard / Runtime / Voice / Agents / Skills / Hooks）
///   initial open = true（模拟用户已经打开设置弹窗）
///   target sequence = Runtime → Voice → Skills（覆盖三个非默认分组）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. open() 打开设置面板
///   2. select(Runtime) → active_section = Runtime，open 仍为 true
///   3. select(Voice) → active_section = Voice，open 仍为 true
///   4. select(Skills) → active_section = Skills，open 仍为 true
///
/// 预期结果：
///   - 正断言：Runtime / Voice / Skills 都能成为 active_section
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

    panel.select(SettingsSection::Voice);
    assert_eq!(
        panel.active_section,
        SettingsSection::Voice,
        "选择 Voice 后右侧内容应切到 Voice 分组"
    );
    assert!(panel.open, "选择 Voice 不应关闭设置弹窗");
    assert_ne!(
        panel.active_section,
        SettingsSection::Runtime,
        "选择 Voice 后不应继续停留在 Runtime 分组"
    );

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

/// Settings 导航暴露七个已实现页面，并继续隐藏 Runtime/Skills mock 分组。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings_panel.rs + settings_content.rs（生产侧栏与内容路由）；
///   visible sections = Themes + Keyboard + Voice + Agents + Orchestration + Wallpaper + Performance，共 7 个；
///   hidden mock sections = Runtime + Skills，共 2 个。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 settings_panel.rs 生产源码
///   2. 检查侧栏接线的 7 个可配置页面
///   3. 检查内容路由明确把 2 个旧 mock 分组收敛为空白 fallback
///
/// 预期结果：
///   - 正断言：所有实际可配置页面均在侧栏和内容路由中存在
///   - 负断言：旧 mock 页面不得重新接入内容渲染
#[test]
fn test_settings_sidebar_exposes_only_implemented_configuration_pages() {
    let source = format!(
        "{}{}{}",
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/navigation.rs"),
        include_str!("../../src/settings/content.rs")
    );
    let visible_sections = [
        "SettingsSection::Themes",
        "SettingsSection::Keyboard",
        "SettingsSection::Voice",
        "SettingsSection::Agents",
        "SettingsSection::Orchestration",
        "SettingsSection::Wallpaper",
        "SettingsSection::Performance",
    ];

    for section in visible_sections {
        assert!(
            source.contains(section),
            "已实现的 Settings 分组必须保持侧栏/内容路由接线: {section}"
        );
    }
    assert!(
        source.contains("SettingsSection::Runtime | SettingsSection::Skills => div()"),
        "旧 Runtime/Skills 分组必须保留为空白 fallback，不能恢复为 mock 内容"
    );
}

/// T-W5: Settings 侧栏必须新增 Wallpaper 项，并在 content 分发里渲染它。
///
/// 数据来源：将 panel.rs / content.rs / wallpaper.rs 三个源文件拼接为单一
/// source 字符串，逐一检查下列 anchor 子串是否存在。
///
/// 执行过程：
///   1. include_str! 拼接三处源码 → src
///   2. 遍历 required anchors，断言每个都出现在 src 中
///
/// 预期结果：
///   - "SettingsSection::Wallpaper"：枚举新增 Wallpaper 变体
///   - "\"Wallpaper\""：title()/content_title() 返回 Wallpaper
///   - "wallpaper_settings"：app 字段 / 渲染 fn 名已接线
///   - "wallpaper_state"：settings_content 签名新增该参数
#[test]
fn test_settings_has_wallpaper_section() {
    let src = format!(
        "{}{}{}",
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/wallpaper.rs")
    );
    let required = [
        "SettingsSection::Wallpaper",
        "\"Wallpaper\"",      // title
        "wallpaper_settings", // toolbar/state field wired
        "wallpaper_state",    // param
    ];
    for anchor in required {
        assert!(
            src.contains(anchor),
            "wallpaper settings wiring missing: {anchor}"
        );
    }
}

/// 检查更新入口收口到左下角品牌卡片，不再占用独立 Settings 导航与内容页面。
///
/// 数据构造（含关键数值的推导过程）：
///   独立更新页面数 = 0；
///   左下角品牌卡片 = 1；
///   检查更新按钮 = 1；
///   Sparkle 调用入口 = 1（`crate::updater::check_for_updates`）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Settings 导航、内容分发与模块声明源码。
///   2. 确认品牌卡片右侧渲染带 tooltip 的更新图标按钮。
///   3. 确认按钮复用 App-global Sparkle 检查入口。
///   4. 排除 Software Update section、页面函数和独立下载实现。
///
/// 预期结果：
///   - 正断言：品牌卡片、版本号、更新按钮与 Sparkle 调用同时存在。
///   - 正断言：按钮使用生产 AssetSource 可加载的循环箭头图标，并提供可访问 tooltip。
///   - 负断言：Settings 不再出现 Software Update 导航、标题或独立页面。
///   - 负断言：Settings 不得另造 HTTP 或子进程下载器。
#[test]
fn test_settings_footer_owns_the_only_update_entry() {
    let source = format!(
        "{}{}{}",
        include_str!("../../src/settings/mod.rs"),
        include_str!("../../src/settings/navigation.rs"),
        include_str!("../../src/settings/content.rs")
    );

    assert!(
        source.contains("settings-brand-footer")
            && source.contains("settings-brand-version")
            && source.contains("settings-check-for-updates")
            && source.contains("crate::updater::check_for_updates(cx)"),
        "Settings 左下角品牌卡片必须同时展示版本号与共享 Sparkle 检查入口"
    );
    assert!(
        source.contains("Icon::default().path(SETTINGS_CHECK_UPDATES_ASSET)")
            && source.contains("Check for updates")
            && source.contains(".tooltip("),
        "检查更新按钮必须使用生产环境可加载的循环箭头图标，并通过 tooltip 明确说明用途"
    );
    assert!(
        !source.contains("SettingsSection::SoftwareUpdate")
            && !source.contains("software_update_settings")
            && !source.contains("\"Software Update\""),
        "Settings 不得继续保留 Software Update 导航、标题或独立内容页面"
    );
    assert!(
        !source.contains("reqwest")
            && !source.contains("curl")
            && !source.contains("std::process::Command"),
        "Settings 页面不得另造 HTTP 或子进程下载器，必须复用 Sparkle 的签名更新流程"
    );
}
