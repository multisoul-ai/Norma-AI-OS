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

/// Product UI 启动主题：应用必须恢复已保存主题，并在没有保存值时回退到 Paper Blue。
///
/// 数据构造（含关键数值的推导过程）：
///   bootstrap source = `app/bootstrap.rs` + `app/bootstrap/stored.rs`
///   selection source = `settings/theme_selection.rs`
///   主题 id 初始化数 = 1；该 id 会同时影响 Launchpad、Canvas 卡片与 Settings
///   持久化优先级 = 已保存合法 ThemeId > `ThemeId::default()`（Paper Blue）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取生产 bootstrap 源码。
///   2. 检查启动时通过共享 `SettingsStore` 调用
///      `ThemeSettingsState::load_from_store()`，同时恢复主题与持久化状态。
///   3. 检查无合法保存值时使用 `unwrap_or_default()` 回退 Paper Blue。
///   4. 排除 `Theme::dark()`，避免主题选择被硬编码暗色覆盖。
///
/// 预期结果：
///   - 正断言：bootstrap 使用恢复出的主题 id 构造主题，缺省值仍为 Paper Blue。
///   - 负断言：bootstrap 不得硬编码 `Theme::dark()` 或跳过持久化恢复。
#[test]
fn test_product_ui_bootstrap_activates_light_glass_theme() {
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
        "没有合法保存值时必须回退 ThemeId::default() 对应的 Paper Blue"
    );
    assert!(
        !bootstrap.contains("let theme = Theme::dark();") && !stored.contains("Theme::dark()"),
        "bootstrap 不得硬编码旧暗色主题，否则保存的选择与 Paper Blue 缺省值都不会生效"
    );
}

/// Product UI Dock 对比度：浅色全局主题下，深色 Dock 的非激活图标与键帽仍须可读。
///
/// 数据构造（含关键数值的推导过程）：
///   Dock fill alpha = 0x57 / 0xff ≈ 34%，叠在画布上形成深色玻璃表面。
///   非激活图标 alpha = 0xd6 / 0xff ≈ 84%，selected 模式与 Voice 文字 = 0xff / 0xff = 100%。
///   全局 `text_primary = #111214`，不能直接复用于深色 Dock，否则对比度接近 1:1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Command Dock 图标与 Voice 单元生产源码。
///   2. 检查非激活 SVG 图标使用固定浅色前景。
///   3. 检查 Voice 键帽使用固定浅色文字，不继承全局深色文字 token。
///
/// 预期结果：
///   - 正断言：非激活图标使用 `0xffffffd6`，selected 模式使用 `0xffffffff`。
///   - 正断言：Voice 文字使用 `0xffffffff`。
///   - 负断言：两者不得使用浅色主题的 `text_primary/text_tertiary` 深色前景。
#[test]
fn test_product_ui_dark_dock_keeps_light_foreground_under_light_theme() {
    let chrome = include_str!("../../src/app/chrome/dock.rs");
    let voice = include_str!("../../src/app/voice/bar.rs");

    assert!(
        chrome.contains("let icon_color = if active { 0xffffffff } else { 0xffffffd6 };"),
        "玻璃 Dock 的非激活 SVG 图标必须使用 84% 白色，selected 模式必须使用 100% 白色前景"
    );
    assert!(
        voice.contains("text: 0xffffffff"),
        "玻璃 Dock 内 Voice 文字必须使用可读的白色前景"
    );
    assert!(
        !chrome.contains("} else {\n        theme.text_primary\n    };"),
        "非激活 Dock 图标不得继承浅色主题的深色 text_primary"
    );
    assert!(
        !voice.contains(".text_color(rgba(theme.text_tertiary))"),
        "Voice 文字不得继承浅色主题的深色 text_tertiary"
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
