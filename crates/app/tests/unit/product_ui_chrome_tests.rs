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

/// Product UI 启动主题：应用必须实际选择浅色玻璃主题，而不是只定义未使用的 token。
///
/// 数据构造（含关键数值的推导过程）：
///   bootstrap source = `app/bootstrap.rs`
///   主题选择调用数 = 1；该调用会同时影响 Launchpad、Canvas 卡片与 Settings。
///   参考稿主背景 = warm white / mist，因此启动主题必须为 `glass_light()`。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取生产 bootstrap 源码。
///   2. 检查唯一的主题初始化使用 `Theme::glass_light()`。
///   3. 排除 `Theme::dark()`，避免浅色 token 仅存在于 Theme 定义却从未生效。
///
/// 预期结果：
///   - 正断言：bootstrap 使用 `Theme::glass_light()`。
///   - 负断言：bootstrap 不得继续硬编码 `Theme::dark()`。
#[test]
fn test_product_ui_bootstrap_activates_light_glass_theme() {
    let bootstrap = include_str!("../../src/app/bootstrap.rs");

    assert!(
        bootstrap.contains("let theme = Theme::glass_light();"),
        "Product UI 启动时必须实际激活 glass_light 主题"
    );
    assert!(
        !bootstrap.contains("let theme = Theme::dark();"),
        "bootstrap 不得继续硬编码旧暗色主题，否则参考稿浅色 token 不会生效"
    );
}

/// Product UI Dock 对比度：浅色全局主题下，深色 Dock 的非激活图标与键帽仍须可读。
///
/// 数据构造（含关键数值的推导过程）：
///   Dock fill alpha = 0x57 / 0xff ≈ 34%，叠在画布上形成深色玻璃表面。
///   非激活图标 alpha = 0xd6 / 0xff ≈ 84%，Voice 文字 alpha = 0xff / 0xff = 100%。
///   全局 `text_primary = #111214`，不能直接复用于深色 Dock，否则对比度接近 1:1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Command Dock 图标与 Voice 单元生产源码。
///   2. 检查非激活 SVG 图标使用固定浅色前景。
///   3. 检查 Voice 键帽使用固定浅色文字，不继承全局深色文字 token。
///
/// 预期结果：
///   - 正断言：非激活图标使用 `0xffffffd6`。
///   - 正断言：Voice 文字使用 `0xffffffff`。
///   - 负断言：两者不得使用浅色主题的 `text_primary/text_tertiary` 深色前景。
#[test]
fn test_product_ui_dark_dock_keeps_light_foreground_under_light_theme() {
    let chrome = include_str!("../../src/app/chrome/dock.rs");
    let voice = include_str!("../../src/app/voice/bar.rs");

    assert!(
        chrome.contains("let icon_color = if active { theme.graphite } else { 0xffffffd6 };"),
        "玻璃 Dock 的非激活 SVG 图标必须使用 84% 白色前景"
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

/// Product UI Dock 材质：原生实现必须复用目标稿的半透明蓝灰外壳，而不是近黑实心胶囊。
///
/// 数据构造（含关键数值的推导过程）：
///   target fill = rgba(21, 28, 39, .34) → 0x151c2757（round(.34 × 255)=87=0x57）。
///   target border = rgba(255,255,255,.42) → 0xffffff6b（round(.42 × 255)=107=0x6b）。
///   active fill = rgba(255,255,255,.92) → 0xffffffeb（round(.92 × 255)=235=0xeb）。
///   外壳 radius=18px；按钮 radius=12px；按钮 size=38px；分隔线 height=22px。
///   active shadow = y 7px / blur 18px / rgba(4,9,18,.2) → 0x04091233。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 截取 Command Dock 与 chrome_icon_button 生产源码。
///   2. 检查外壳填充、描边、圆角和 spacing 的已审字面量。
///   3. 检查按钮尺寸、选中态浅色填充/投影、分隔线高度及旧近黑填充是否仍存在。
///
/// 预期结果：
///   - 正断言：Dock 使用 34% 蓝灰玻璃、42% 白描边和 18px 圆角。
///   - 正断言：按钮为 38px/12px，选中态为 92% 白色并带目标投影。
///   - 正断言：分隔线高 22px，不得回退到旧 20px。
///   - 负断言：Dock 不得继续使用旧 `theme.graphite` 0xe8 近黑填充。
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
        dock.contains(".border_color(rgba(0xffffff6b))") && dock.contains(".bg(rgba(0x151c2757))"),
        "Dock 应使用目标稿 42% 白描边与 34% 蓝灰玻璃填充"
    );
    assert!(
        button.contains(".w(px(38.0))")
            && button.contains(".h(px(38.0))")
            && button.contains(".rounded(px(12.0))")
            && button.contains("0xffffffeb")
            && button.contains("command_button_shadow(active)"),
        "Dock 按钮应为 38px/12px，选中态使用 92% 白色填充与目标投影"
    );
    assert!(
        divider.contains(".h(px(22.0))")
            && divider.contains("color: rgba(0x04091233).into()")
            && divider.contains("offset: point(px(0.0), px(7.0))")
            && divider.contains("blur_radius: px(18.0)"),
        "Dock 分隔线应高 22px，选中工具投影应为 y=7px/blur=18px/20% 深蓝"
    );
    assert!(
        !dock.contains("with_alpha(theme.graphite, 0xe8)"),
        "Dock 不得回退到会遮住壁纸色彩的旧 91% graphite 实心填充"
    );
}

/// Product UI 细节对齐：顶部路径、Dock 选中态和缩略图信息必须保持稳定可辨。
///
/// 数据构造（含关键数值的推导过程）：
///   顶部标题与路径 gap = 6px；路径字号 = 9px；Dock active fill = 0xffffffeb（92% 白）。
///   缩略图目标结构 = 节点区 + 控制区；控制区文本 = `−`、当前 zoom 百分比、`＋`。
///   选中集合来源 = runtime.selection，不能只读取可能滞后的 node.selected。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 top_bar、Dock 与 minimap 生产源码。
///   2. 检查顶部路径具备 gap、Menlo、9px 与截断保护。
///   3. 检查 Dock active hover/pressed 仍保持浅色选中面。
///   4. 检查缩略图显示 zoom 百分比，并以 runtime.selection 判断选中节点。
///
/// 预期结果：
///   - 正断言：顶部路径、Dock active 反馈、缩略图 zoom/selection 均存在。
///   - 负断言：选中 Dock 不得 hover 成透明深色；缩略图不得只依赖 node.selected。
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
        chrome.contains("active_hover_fill")
            && chrome.contains("active_pressed_fill")
            && chrome.contains("0xfffffff5")
            && chrome.contains("0xffffffdc"),
        "Dock 选中按钮 hover/pressed 应保持浅色选中面，不得视觉反选"
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
