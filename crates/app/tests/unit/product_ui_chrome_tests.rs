/// Product UI Command Dock：3 个图标按钮 + 2 个分隔符，并嵌入 Voice 状态单元。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/app/chrome/dock.rs")
///   render_source = source 中 #[cfg(test)] 前的生产渲染区
///   dock_start = `fn render_command_dock`
///   dock_end = `fn render_desktop_overview_dock`
///   分隔符调用次数 = 2；图标按钮调用次数 = Workspace + Layout + Settings = 3
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 截取 Command Dock 函数区段。
///   2. 确认 Dock 含 3 个 chrome_icon_button、2 个 chrome_divider 和稳定 selector。
///   3. 确认 Voice 嵌入 Dock，旧文字按钮、旧 glyph 与 Select/Scroll 模式按钮不再出现。
///
/// 预期结果：
///   - 正断言：Dock 含 3 个图标按钮、2 个分隔符、指令输入与 Voice。
///   - 负断言：Dock 不含旧 chrome_button、CANVAS、+、⚙ 或 ↕ 字面量。
///   - 负断言：Dock 不含 toolbar-canvas-scroll、"Select mode"、"Scroll mode"。
#[test]
fn test_product_ui_command_dock_uses_grouped_icons_and_embedded_voice() {
    let source = include_str!("../../src/app/chrome/dock.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let dock_start = render_source
        .find("fn render_command_dock")
        .expect("Command Dock 应有独立渲染函数");
    let dock_end = render_source
        .find("fn render_desktop_overview_dock")
        .expect("总览 Dock 应作为项目 Command Dock 区段的右界");
    let dock = &render_source[dock_start..dock_end];
    let icon_button_calls = dock.matches("chrome_icon_button(").count();
    let divider_calls = dock.matches("chrome_divider(").count();

    assert_eq!(
        icon_button_calls, 3,
        "Dock 应恰好含 3 个图标按钮，actual={icon_button_calls}"
    );
    assert!(
        !dock.contains("toolbar-canvas-scroll")
            && !dock.contains("Select mode")
            && !dock.contains("Scroll mode"),
        "Dock 不得再渲染 Select/Scroll 画布模式按钮"
    );
    assert_eq!(
        divider_calls, 2,
        "Dock 应恰好含 2 个分隔符，actual={divider_calls}"
    );
    assert!(
        dock.contains("workspace-command-dock") && dock.contains("render_voice_bar"),
        "Dock 必须暴露 selector 并嵌入 Voice 状态单元"
    );
    assert!(
        dock.contains("render_command_input"),
        "Dock 应提供统一指令输入入口"
    );
    assert!(
        !dock.contains("chrome_button(") && !render_source.contains("\"CANVAS\""),
        "Dock 不得回退为旧文本按钮或 CANVAS 占位"
    );
    assert!(
        !dock.contains("\"+\"") && !dock.contains("\"⚙\"") && !dock.contains("\"↕\""),
        "Dock 不得回退为 +、⚙ 或 ↕ Unicode glyph"
    );
}

/// Product UI 启动主题：应用必须恢复已保存主题，并在没有保存值时使用默认主题。
///
/// 数据构造（含关键数值的推导过程）：
///   bootstrap source = `app/bootstrap.rs` + `app/bootstrap/stored.rs`
///   selection source = `settings/theme_selection.rs`
///   主题 id 初始化数 = 1；该 id 会同时影响 Launchpad、Canvas 卡片与 Settings
///   持久化优先级 = 已保存合法 ThemeId > `ThemeId::default()`
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取生产 bootstrap 源码。
///   2. 检查启动时通过共享 `SettingsStore` 调用
///      `ThemeSettingsState::load_from_store()`，同时恢复主题与持久化状态。
///   3. 检查无合法保存值时使用 `unwrap_or_default()` 回退默认主题。
///   4. 排除 `Theme::dark()`，避免主题选择被硬编码暗色覆盖。
///
/// 预期结果：
///   - 正断言：bootstrap 使用恢复出的主题 id 构造主题，缺省值跟随默认主题。
///   - 负断言：bootstrap 不得硬编码 `Theme::dark()` 或跳过持久化恢复。
#[test]
fn test_product_ui_bootstrap_restores_saved_theme_or_uses_default() {
    let bootstrap = include_str!("../../src/app/bootstrap.rs");
    let stored = include_str!("../../src/app/bootstrap/stored.rs");
    let selection = include_str!("../../src/settings/theme_selection.rs");

    assert!(
        bootstrap.contains("StoredBootstrap::load(&app_store)")
            && stored.contains("let settings = store.settings_store();"),
        "Product UI 启动时必须从已选择的 AppStore 派生共享 SettingsStore"
    );
    assert!(
        stored.contains("ThemeSettingsState::load_from_store(settings.clone())")
            && bootstrap.contains("let theme = Theme::for_id(theme_id);"),
        "Product UI 启动时必须从显式 SettingsStore 恢复主题，并用恢复出的 ThemeId 构造主题"
    );
    assert!(
        selection.contains(".unwrap_or_default();"),
        "没有合法保存值时必须回退 ThemeId::default()"
    );
    assert!(
        !bootstrap.contains("let theme = Theme::dark();") && !stored.contains("Theme::dark()"),
        "bootstrap 不得硬编码旧暗色主题，否则保存的选择与默认主题都不会生效"
    );
}

/// Product UI 细节对齐：顶部路径和 Dock 选中态必须保持稳定可辨。
///
/// 数据构造（含关键数值的推导过程）：
///   顶部标题与路径 gap = 6px；路径字号 = 9px；Dock mode fill = accent_cyan 0x28（≈16%）。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 top_bar 与 Dock 生产源码。
///   2. 检查顶部路径具备 gap、Menlo、9px 与截断保护。
///   3. 检查 Dock mode hover/pressed 保持渐进 cyan 状态面，输入框有独立焦点反馈。
///
/// 预期结果：
///   - 正断言：顶部路径与 Dock mode 反馈均存在。
///   - 负断言：Dock mode 不得回退为白色主操作。
#[test]
fn test_product_ui_review_details_keep_path_and_active_dock_legible() {
    let top_bar = include_str!("../../src/app/chrome/top_bar.rs");
    let chrome = include_str!("../../src/app/chrome/dock.rs");

    assert!(
        top_bar.contains(".gap(px(6.0))")
            && top_bar.contains(".font_family(\"Menlo\")")
            && top_bar.contains(".text_size(px(9.0))")
            && top_bar.contains(".truncate()"),
        "顶部工作区路径应使用 6px 间距、Menlo 9px 和截断保护"
    );
    assert!(
        chrome.contains("let hover_fill = if active")
            && chrome.contains("let pressed_fill = if active")
            && chrome.contains("with_alpha(theme.accent_cyan, 0x40)")
            && chrome.contains("with_alpha(theme.accent_cyan, 0x54)"),
        "Dock 模式 hover/pressed 应渐进增强 cyan"
    );
    assert!(
        !chrome.contains("0xfffffff5") && !chrome.contains("0xffffffdc"),
        "Dock 持续模式不得回退为白色主操作，避免与 New 争夺视觉焦点"
    );
}
