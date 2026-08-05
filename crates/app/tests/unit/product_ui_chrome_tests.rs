/// Product UI Command Dock：7 个图标按钮 + 2 个分隔符，并嵌入 Voice 状态单元。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/app/chrome/dock.rs")
///   render_source = source 中 #[cfg(test)] 前的生产渲染区
///   dock_start = `fn render_command_dock`
///   dock_end = `fn command_dock_shadow`
///   分隔符调用次数 = 2；图标按钮调用次数 = 7
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 截取 Command Dock 函数区段。
///   2. 确认 Dock 含 7 个 chrome_icon_button、2 个 chrome_divider 和稳定 selector。
///   3. 确认 Voice 嵌入 Dock，旧文字按钮与旧 glyph 不再出现。
///
/// 预期结果：
///   - 正断言：Dock 含 7 个图标按钮、2 个分隔符、Preview 与 Voice。
///   - 负断言：Dock 不含旧 chrome_button、CANVAS、+、⚙ 或 ↕ 字面量。
#[test]
fn test_product_ui_command_dock_uses_grouped_icons_and_embedded_voice() {
    let source = include_str!("../../src/app/chrome/dock.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let dock_start = render_source
        .find("fn render_command_dock")
        .expect("Command Dock 应有独立渲染函数");
    let dock_end = render_source
        .find("fn command_dock_shadow")
        .expect("command_dock_shadow 应作为 Command Dock 区段的右界");
    let dock = &render_source[dock_start..dock_end];
    let icon_button_calls = dock.matches("chrome_icon_button(").count();
    let divider_calls = dock.matches("chrome_divider(").count();

    assert_eq!(
        icon_button_calls, 7,
        "Dock 应恰好含 7 个图标按钮，actual={icon_button_calls}"
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
        dock.contains("ToolbarIcon::Preview"),
        "Product UI Dock 应提供 Preview 直达入口"
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

/// Command Dock 信息层级：画布、创建与工具入口必须形成 2｜5｜2 三组布局。
///
/// 数据构造（含关键数值的推导过程）：
///   画布控制 = Select/Scroll + Layout = 2 项
///   创建快捷入口 = Terminal + Codex + New + Browser + Preview = 5 项
///   工具入口 = Voice + Settings = 2 项
///   组间分隔符 = 画布→创建、创建→工具 = 2 条
///   连续顺序 = 2 个画布控制 < 第 1 条分隔符 < 5 个创建入口 < 第 2 条分隔符 < 2 个工具入口
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 截取 Command Dock 的生产渲染区，排除 helper 与测试文本的干扰。
///   2. 定位每个稳定 debug selector、两条分隔符和 Voice 渲染调用的位置。
///   3. 验证画布控制、创建动作和工具入口按 2｜5｜2 连续分组。
///   4. 验证持续生效的画布模式不会再使用 92% 白色的大面积主按钮填充。
///
/// 预期结果：
///   - 断言 A：New、四个快捷创建入口、两条分隔符与 Voice 均存在。
///   - 断言 B：画布、创建、工具严格按三段顺序排列，New 位于创建组正中。
///   - 断言 C：选中模式使用低强调填充，不得复用 0xffffffeb 的主操作外观。
///   - 断言 D：创建入口不得穿插到画布组，Settings 不得回到 Voice 左侧。
#[test]
fn test_product_ui_command_dock_orders_canvas_creation_and_utility_as_three_groups() {
    let source = include_str!("../../src/app/chrome/dock.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let dock_start = render_source
        .find("fn render_command_dock")
        .expect("Command Dock 应有独立渲染函数");
    let dock_end = render_source
        .find("fn command_dock_shadow")
        .expect("command_dock_shadow 应作为 Command Dock 区段的右界");
    let dock = &render_source[dock_start..dock_end];

    let new_trigger = dock
        .find("\"toolbar-new-terminal\"")
        .expect("Dock 必须保留 New 菜单触发器");
    let terminal = dock
        .find("\"toolbar-terminal\"")
        .expect("Dock 必须保留 Terminal 快捷入口");
    let codex = dock
        .find("\"toolbar-new-codex-agent\"")
        .expect("Dock 必须保留 Codex 快捷入口");
    let browser = dock
        .find("\"toolbar-new-browser\"")
        .expect("Dock 必须保留 Browser 快捷入口");
    let preview = dock
        .find("\"toolbar-open-preview\"")
        .expect("Dock 必须保留 Preview 快捷入口");
    let canvas_mode = dock
        .find("\"toolbar-canvas-scroll\"")
        .expect("Dock 必须保留 Select/Scroll 切换入口");
    let layout = dock
        .find("\"toolbar-layout\"")
        .expect("Dock 必须保留 Layout 入口");
    let settings = dock
        .find("\"toolbar-settings\"")
        .expect("Dock 必须保留 Settings 入口");
    let first_divider = dock
        .find(".child(chrome_divider(theme))")
        .expect("画布控制组后必须有第 1 条分隔符");
    let second_divider = dock[first_divider + 1..]
        .find(".child(chrome_divider(theme))")
        .map(|offset| first_divider + 1 + offset)
        .expect("创建组后必须有第 2 条分隔符");
    let voice = dock
        .find("render_voice_bar")
        .expect("Dock 必须保留 Voice 状态入口");

    assert!(
        canvas_mode < layout && layout < first_divider,
        "画布组必须按 Select/Scroll、Layout 连续排在第 1 条分隔符前"
    );
    assert!(
        first_divider < terminal
            && terminal < codex
            && codex < new_trigger
            && new_trigger < browser
            && browser < preview
            && preview < second_divider,
        "创建组必须按 Terminal、Codex、New、Browser、Preview 连续位于两条分隔符之间"
    );
    assert!(
        second_divider < voice && voice < settings,
        "工具组必须按 Voice、Settings 连续位于第 2 条分隔符之后"
    );
    assert!(
        !render_source.contains("let fill = if active { 0xffffffeb }")
            && render_source.contains("with_alpha(theme.accent_cyan, 0x28)"),
        "持续激活的画布模式必须使用低强调 cyan 状态，而非 92% 白色的主操作外观"
    );
    assert!(
        terminal > layout && settings > voice,
        "创建入口不得穿插到画布组，Settings 不得回到 Voice 左侧"
    );
}

/// Product UI 启动主题：应用必须恢复已保存主题，并在没有保存值时回退到 Paper Blue。
///
/// 数据构造（含关键数值的推导过程）：
///   bootstrap source = `app/bootstrap.rs`
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
    let selection = include_str!("../../src/settings/theme_selection.rs");

    assert!(
        bootstrap.contains("let settings_store = app_store.settings_store();"),
        "Product UI 启动时必须从已选择的 AppStore 派生共享 SettingsStore"
    );
    assert!(
        bootstrap.contains("ThemeSettingsState::load_from_store(settings_store.clone())")
            && bootstrap.contains("let theme = Theme::for_id(theme_id);"),
        "Product UI 启动时必须从显式 SettingsStore 恢复主题，并用恢复出的 ThemeId 构造主题"
    );
    assert!(
        selection.contains(".unwrap_or_default();"),
        "没有合法保存值时必须回退 ThemeId::default() 对应的 Paper Blue"
    );
    assert!(
        !bootstrap.contains("let theme = Theme::dark();"),
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

/// Product UI Dock 材质：外壳必须消费当前主题的 Dock token，创建与持续模式不得争夺同一主强调。
///
/// 数据构造（含关键数值的推导过程）：
///   theme.dock = 六套主题各自的 Dock 玻璃色；theme.dock_line = 对应边线色。
///   模式 selected fill = accent_cyan × 0x28 / 0xff ≈ 16%，hover = 0x40 / 0xff ≈ 25%。
///   New 主操作 fill = accent_cyan × 0x4d / 0xff ≈ 30%，菜单打开 = 0x70 / 0xff ≈ 44%。
///   外壳 radius=18px；按钮 radius=12px；按钮 size=38px；分隔线 height=22px。
///   active shadow = y 7px / blur 18px / rgba(4,9,18,.2) → 0x04091233。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 截取 Command Dock 与 chrome_icon_button 生产源码。
///   2. 检查外壳消费主题填充/描边 token，并保留已审圆角和 spacing。
///   3. 检查按钮尺寸、模式低强调、New 主操作与分隔线高度及旧白色块填充是否仍存在。
///
/// 预期结果：
///   - 正断言：Dock 使用主题玻璃色、主题边线色和 18px 圆角。
///   - 正断言：按钮为 38px/12px，模式为低强调 cyan，New 是唯一主操作并带目标投影。
///   - 正断言：分隔线高 22px，不得回退到旧 20px。
///   - 负断言：模式按钮不得继续使用 92% 白色块或旧 `theme.graphite` 0xe8 近黑填充。
#[test]
fn test_product_ui_command_dock_matches_reviewed_glass_material_literals() {
    let source = include_str!("../../src/app/chrome/dock.rs");
    let render_source = source.split("#[cfg(test)]").next().unwrap_or(source);
    let dock_start = render_source
        .find("fn render_command_dock")
        .expect("Command Dock 应有独立渲染函数");
    let button_start = render_source
        .find("fn chrome_icon_button")
        .expect("Dock 图标应复用 chrome_icon_button");
    let divider_start = render_source
        .find("fn chrome_divider")
        .expect("chrome_divider 应作为按钮区段右界");
    let tooltip_start = render_source
        .find("fn tooltip_view")
        .expect("tooltip_view 应作为分隔线区段右界");
    let dock = &render_source[dock_start..button_start];
    let button = &render_source[button_start..divider_start];
    let divider = &render_source[divider_start..tooltip_start];

    assert!(
        dock.contains(".gap(px(3.0))")
            && dock.contains(".p(px(6.0))")
            && dock.contains(".rounded(px(18.0))"),
        "Dock 应复用目标稿 3px gap、6px padding 与 18px 圆角"
    );
    assert!(
        dock.contains(".border_color(rgba(theme.dock_line))")
            && dock.contains(".bg(rgba(theme.dock))"),
        "Dock 应使用当前主题的 dock 与 dock_line token"
    );
    assert!(
        button.contains(".w(px(38.0))")
            && button.contains(".h(px(38.0))")
            && button.contains(".rounded(px(12.0))")
            && button.contains("with_alpha(theme.accent_cyan, 0x28)")
            && button.contains("with_alpha(theme.accent_cyan, 0x4d)")
            && button.contains("with_alpha(theme.accent_cyan, 0x70)")
            && button.contains("command_button_shadow(active)"),
        "Dock 按钮应为 38px/12px；模式使用低强调 cyan，New 使用独立主操作填充与目标投影"
    );
    assert!(
        divider.contains(".h(px(22.0))")
            && divider.contains("color: rgba(0x04091233).into()")
            && divider.contains("offset: point(px(0.0), px(7.0))")
            && divider.contains("blur_radius: px(18.0)"),
        "Dock 分隔线应高 22px，选中工具投影应为 y=7px/blur=18px/20% 深蓝"
    );
    assert!(
        !button.contains("0xffffffeb") && !dock.contains("with_alpha(theme.graphite, 0xe8)"),
        "Dock 模式不得回退为会抢夺 New 主操作的 92% 白色块或遮住壁纸的旧 graphite 实心填充"
    );
}

/// Product UI 细节对齐：顶部路径、Dock 选中态和缩略图信息必须保持稳定可辨。
///
/// 数据构造（含关键数值的推导过程）：
///   顶部标题与路径 gap = 6px；路径字号 = 9px；Dock mode fill = accent_cyan 0x28（≈16%）。
///   缩略图目标结构 = 节点区 + 控制区；控制区文本 = `−`、当前 zoom 百分比、`＋`。
///   选中集合来源 = runtime.selection，不能只读取可能滞后的 node.selected。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 top_bar、Dock 与 minimap 生产源码。
///   2. 检查顶部路径具备 gap、Menlo、9px 与截断保护。
///   3. 检查 Dock mode hover/pressed 保持渐进 cyan 状态面，New 主操作另有独立填充。
///   4. 检查缩略图显示 zoom 百分比，并以 runtime.selection 判断选中节点。
///
/// 预期结果：
///   - 正断言：顶部路径、Dock mode 反馈、缩略图 zoom/selection 均存在。
///   - 负断言：Dock mode 不得回退为白色主操作；缩略图不得只依赖 node.selected。
#[test]
fn test_product_ui_review_details_keep_path_active_dock_and_minimap_legible() {
    let top_bar = include_str!("../../src/app/chrome/top_bar.rs");
    let chrome = include_str!("../../src/app/chrome/dock.rs");
    let minimap = include_str!("../../src/canvas/minimap.rs");

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
            && chrome.contains("with_alpha(theme.accent_cyan, 0x54)")
            && chrome.contains("fn chrome_primary_icon_button"),
        "Dock 模式 hover/pressed 应渐进增强 cyan，New 必须保留独立主操作样式"
    );
    assert!(
        !chrome.contains("0xfffffff5") && !chrome.contains("0xffffffdc"),
        "Dock 持续模式不得回退为白色主操作，避免与 New 争夺视觉焦点"
    );
    assert!(
        minimap.contains("canvas-minimap-zoom")
            && minimap.contains("format!(\"{:.0}%\"")
            && minimap.contains("runtime.selection.contains(node.id)"),
        "缩略图应显示实时 zoom 百分比，并使用 runtime.selection 标记选中节点"
    );
    assert!(
        !minimap.contains("if node.selected { 0xf0 } else { 0xa0 }"),
        "缩略图不得只读取 node.selected，否则 runtime selection 会显示不同步"
    );
}
