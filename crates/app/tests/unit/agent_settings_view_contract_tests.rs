/// AS-18: Agents 设置页应采用 Agent Library + 统一 Inspector，并提供完整 CRUD 入口。
///
/// 数据构造（含关键数值的推导过程）：
///   source file count        = 4（view.rs + inspector.rs + list.rs + actions.rs）
///   Library anchors          = 5（列表、搜索、All、Enabled、Disabled）
///   Inspector anchors        = 7（Inspector、Name、Workflow、Toggle、Add、Save、Discard）
///   destructive anchors      = 3（显式 Delete、确认层、确认按钮）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 master-detail、Library、Inspector 与 action 源码。
///   2. 检查左侧同时提供搜索、状态筛选与 New Agent。
///   3. 检查右侧字段始终存在，dirty 时提供 Save Changes / Discard。
///   4. 检查 Add to Canvas 与底部显式 Delete Agent 独立存在，删除需确认层。
///
/// 预期结果：
///   - 正断言：Library、统一 Inspector、CRUD 与三种筛选锚点全部存在。
///   - 正断言：Delete Agent 是 Inspector 底部显式入口，并进入确认层。
///   - 负断言：不再存在只读详情、Edit action、双击或 inline card editing。
///   - 负断言：Add to Canvas 不得被编辑或删除动作替代。
#[test]
fn test_agents_settings_view_uses_library_and_unified_inspector_contract() {
    let view_source = include_str!("../../src/settings/agents/view.rs");
    let inspector_source = include_str!("../../src/settings/agents/view/inspector.rs");
    let list_source = include_str!("../../src/settings/agents/view/list.rs");
    let text_input_source = include_str!("../../src/settings/agents/text_input.rs");
    let actions_source = include_str!("../../src/settings/agents/actions.rs");

    for anchor in [
        "settings-agents-master-detail",
        "settings-agent-inspector",
        "settings-agent-name-input",
        "settings-agent-workflow-input",
        "settings-agent-toggle-enabled",
        "settings-agent-add-to-canvas",
        "settings-agent-on-canvas-indicator",
        "settings-agent-save",
        "settings-agent-discard",
    ] {
        assert!(
            view_source.contains(anchor)
                || inspector_source.contains(anchor)
                || text_input_source.contains(anchor),
            "Agent unified Inspector 应暴露稳定锚点：{anchor}"
        );
    }
    assert!(
        [
            "settings-agents-list",
            "settings-agent-search-input",
            "settings-agent-filter-all",
            "settings-agent-filter-enabled",
            "settings-agent-filter-disabled",
            "settings-agent-new",
        ]
        .iter()
        .all(|anchor| list_source.contains(anchor)),
        "Agent Library 应同时提供列表、搜索、三种状态筛选与 New Agent"
    );
    assert!(
        inspector_source.contains("settings.delete_agent_2")
            && inspector_source.contains("settings-agent-delete")
            && view_source.contains("settings-agent-delete-dialog")
            && view_source.contains("settings-agent-delete-confirm"),
        "统一 Inspector 底部应显式显示 Delete Agent，并通过确认层完成破坏性操作"
    );
    assert!(
        inspector_source.contains("\"settings.add_to_canvas\"")
            && inspector_source.contains("on_agent_settings_create_card")
            && actions_source.contains("fn on_agent_settings_create_card"),
        "统一 Inspector 必须保留 Add to Canvas 及独立 spawn handler"
    );
    assert!(
        !list_source.contains("click_count"),
        "Agent Library 选择不得依赖隐藏的双击手势"
    );
    assert!(
        !list_source.contains("settings-agent-card-editing")
            && !view_source.contains("settings-agent-detail-readonly")
            && !inspector_source.contains("settings-agent-edit"),
        "统一 Inspector 不得残留 inline editing、只读详情或显式 Edit action"
    );
}

/// AS-19：浅色主题下 Agent 列表的 Runtime Profile 必须保持普通小字号文字可读性。
///
/// 数据构造（含关键数值的推导过程）：
///   master fill alpha   = 0xb8 / 0xff ≈ 72.2%
///   list fill alpha     = 0x72 / 0xff ≈ 44.7%
///   selected fill alpha = 0x18 / 0xff ≈ 9.4%
///   old tertiary alpha  = 0x6b / 0xff ≈ 42.0%
///   readable alpha      = 0xc0 / 0xff ≈ 75.3%
///   minimum contrast    = 4.5:1（12px 普通文字的 WCAG AA 下限）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 按生产绘制顺序合成 Canvas → master-detail → Agent list → selected row 四层背景。
///   2. 将旧 tertiary 与新的 readable muted 文字分别合成到选中行背景上。
///   3. 用 sRGB 相对亮度公式计算 Paper Blue、Linen Sage、Silver Violet 的对比度。
///   4. 读取 Agent list 生产源码，确认 Runtime Profile 使用经过验证的 readable muted 颜色。
///
/// 预期结果：
///   - 负断言：Paper Blue 的旧 tertiary 对比度低于 4.5:1，能稳定复现 Codex 难以看清。
///   - 正断言：三个浅色主题的新 muted 颜色都达到 4.5:1。
///   - 正断言：Runtime Profile 明确使用 text_primary RGB + 0xc0 alpha。
///   - 负断言：Runtime Profile 不得继续使用 theme.text_tertiary。
#[test]
fn test_agent_list_runtime_profile_remains_legible_in_light_themes() {
    let list_source = include_str!("../../src/settings/agents/view/list.rs");
    let paper_blue = ui::Theme::for_id(ui::ThemeId::PaperBlue);
    let linen_sage = ui::Theme::for_id(ui::ThemeId::LinenSage);
    let silver_violet = ui::Theme::for_id(ui::ThemeId::SilverViolet);

    let old_paper_contrast = agent_runtime_contrast(paper_blue, paper_blue.text_tertiary);
    let paper_contrast =
        agent_runtime_contrast(paper_blue, with_test_alpha(paper_blue.text_primary, 0xc0));
    let linen_contrast =
        agent_runtime_contrast(linen_sage, with_test_alpha(linen_sage.text_primary, 0xc0));
    let silver_contrast = agent_runtime_contrast(
        silver_violet,
        with_test_alpha(silver_violet.text_primary, 0xc0),
    );

    assert!(
        old_paper_contrast < 4.5,
        "旧 Paper Blue tertiary 应低于 4.5:1 以复现缺陷，实际为 {old_paper_contrast:.3}:1"
    );
    assert!(
        paper_contrast >= 4.5,
        "Paper Blue 的 Agent Runtime Profile 应达到 4.5:1，实际为 {paper_contrast:.3}:1"
    );
    assert!(
        linen_contrast >= 4.5,
        "Linen Sage 的 Agent Runtime Profile 应达到 4.5:1，实际为 {linen_contrast:.3}:1"
    );
    assert!(
        silver_contrast >= 4.5,
        "Silver Violet 的 Agent Runtime Profile 应达到 4.5:1，实际为 {silver_contrast:.3}:1"
    );

    let runtime_anchor = list_source
        .find("settings-agent-list-runtime-profile")
        .expect("Agent list 源码应保留 Runtime Profile 稳定锚点");
    let runtime_source = &list_source[runtime_anchor..];
    assert!(
        runtime_source.contains(".text_color(rgba(with_alpha(theme.text_primary, 0xc0)))"),
        "Agent 列表 Runtime Profile 必须使用 text_primary RGB + 0xc0 alpha 的可读 muted 颜色"
    );
    assert!(
        !runtime_source.contains(".text_color(rgba(theme.text_tertiary))"),
        "Agent 列表 Runtime Profile 不得继续使用浅色模式仅约 2.19:1 的 text_tertiary"
    );
}

/// AS-20：浅色主题下 Agent 表单与搜索框必须有可辨识的控件边界。
///
/// 数据构造（含关键数值的推导过程）：
///   Paper Blue text_secondary alpha = 0x94 / 0xff ≈ 58.0%
///   old edge_highlight RGB           = white，叠在白色 panel 上趋近 1:1
///   checked control surfaces         = 2 个：表单字段 + 搜索框
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 Agent 表单与 Library 搜索框源码。
///   2. 从各自稳定 selector 开始截取控件绘制片段。
///   3. 检查静止态边框使用 text_secondary，而不是白色 edge_highlight。
///
/// 预期结果：
///   - 正断言：表单字段与搜索框都使用 text_secondary 绘制可辨识边界。
///   - 负断言：两个控件都不得继续以 edge_highlight 作为静止态边框。
#[test]
fn test_agent_form_and_search_have_visible_light_theme_boundaries() {
    let text_input_source = include_str!("../../src/settings/agents/text_input.rs");
    let list_source = include_str!("../../src/settings/agents/view/list.rs");
    let field_anchor = text_input_source
        .find("let mut surface = div()")
        .expect("Agent 表单源码应保留 editable field surface");
    let field_source = &text_input_source[field_anchor..];
    let search_anchor = list_source
        .find("settings-agent-search-input")
        .expect("Agent Library 源码应保留搜索框稳定锚点");
    let search_source = &list_source[search_anchor..];

    assert!(
        field_source.contains("theme.text_secondary"),
        "Agent 表单静止态边框应使用 text_secondary，确保 Paper Blue 下可辨识"
    );
    assert!(
        !field_source.contains("theme.edge_highlight\n            },\n            0x54"),
        "Agent 表单静止态边框不得继续使用接近白底的 edge_highlight"
    );
    assert!(
        search_source.contains(".border_color(rgba(theme.text_secondary))"),
        "Agent 搜索框静止态边框应使用 text_secondary，确保 Paper Blue 下可辨识"
    );
    assert!(
        !search_source.contains("with_alpha(theme.edge_highlight, 0x3c)"),
        "Agent 搜索框不得继续使用浅色主题下接近不可见的 edge_highlight 边框"
    );
}

/// AS-21：Paper Blue 下所有未选中的 Runtime Profile 按钮必须保持可见。
///
/// 数据构造（含关键数值的推导过程）：
///   Runtime options             = 5 个：Claude/Codex/Cursor/Grok/OpenCode
///   selected options            = 1 个
///   unselected options          = 5 - 1 = 4 个
///   button fill alpha           = 0x18 / 0xff ≈ 9.4%
///   Paper Blue edge_highlight   = white / 0xf0，叠在白色 panel 上趋近 1:1
///   minimum readable contrast   = 3.0:1（大号控件文字与边界的最低可辨识目标）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 合成 Paper Blue Canvas、Inspector 与未选中按钮背景。
///   2. 分别计算旧 edge_highlight 与新 text_secondary 的按钮文字对比度。
///   3. 读取 Runtime Profile 按钮源码，确认未选中态使用通过验证的颜色 token。
///
/// 预期结果：
///   - 负断言：旧 edge_highlight 对比度低于 1.2:1，稳定复现四个选项“消失”。
///   - 正断言：text_secondary 对比度至少达到 3.0:1。
///   - 正断言：未选中按钮明确使用 text_secondary。
///   - 负断言：未选中按钮不得继续使用 edge_highlight。
#[test]
fn test_unselected_runtime_profile_buttons_remain_visible_in_paper_blue() {
    let inspector_source = include_str!("../../src/settings/agents/view/inspector.rs");
    let theme = ui::Theme::for_id(ui::ThemeId::PaperBlue);
    let inspector_surface = source_over(
        rgba_channels(with_test_alpha(theme.node_fill, 0xb8)),
        rgba_channels(theme.canvas_from),
    );
    let old_button_surface = source_over(
        rgba_channels(with_test_alpha(theme.edge_highlight, 0x18)),
        inspector_surface,
    );
    let old_text = source_over(rgba_channels(theme.edge_highlight), old_button_surface);
    let readable_button_surface = source_over(
        rgba_channels(with_test_alpha(theme.text_secondary, 0x18)),
        inspector_surface,
    );
    let readable_text = source_over(rgba_channels(theme.text_secondary), readable_button_surface);
    let old_contrast = contrast_ratio(old_text, old_button_surface);
    let readable_contrast = contrast_ratio(readable_text, readable_button_surface);
    let profile_anchor = inspector_source
        .find("fn profile_button(")
        .expect("Agent Inspector 源码应保留 Runtime Profile 按钮函数");
    let profile_source = &inspector_source[profile_anchor..];

    assert!(
        old_contrast < 1.2,
        "旧 edge_highlight 应低于 1.2:1 以稳定复现未选中 Runtime 选项不可见，实际为 {old_contrast:.3}:1"
    );
    assert!(
        readable_contrast >= 3.0,
        "Paper Blue 的未选中 Runtime 选项应达到至少 3.0:1，实际为 {readable_contrast:.3}:1"
    );
    assert!(
        profile_source.contains("theme.text_secondary"),
        "未选中的 Runtime Profile 按钮必须使用浅色主题下可见的 text_secondary"
    );
    assert!(
        !profile_source.contains("theme.edge_highlight"),
        "未选中的 Runtime Profile 按钮不得继续使用会融入 Paper Blue 背景的 edge_highlight"
    );
}

fn agent_runtime_contrast(theme: ui::Theme, foreground: u32) -> f64 {
    let canvas = rgba_channels(theme.canvas_from);
    let master = source_over(
        rgba_channels(with_test_alpha(theme.node_fill, 0xb8)),
        canvas,
    );
    let list = source_over(
        rgba_channels(with_test_alpha(theme.panel_high, 0x72)),
        master,
    );
    let selected = source_over(
        rgba_channels(with_test_alpha(theme.accent_orange, 0x18)),
        list,
    );
    let painted_text = source_over(rgba_channels(foreground), selected);
    contrast_ratio(painted_text, selected)
}

fn with_test_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

fn rgba_channels(color: u32) -> [f64; 4] {
    let [red, green, blue, alpha] = color.to_be_bytes();
    [
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0,
        f64::from(alpha) / 255.0,
    ]
}

fn source_over(foreground: [f64; 4], background: [f64; 4]) -> [f64; 4] {
    let alpha = foreground[3];
    [
        foreground[0] * alpha + background[0] * (1.0 - alpha),
        foreground[1] * alpha + background[1] * (1.0 - alpha),
        foreground[2] * alpha + background[2] * (1.0 - alpha),
        1.0,
    ]
}

fn contrast_ratio(first: [f64; 4], second: [f64; 4]) -> f64 {
    let first_luminance = relative_luminance(first);
    let second_luminance = relative_luminance(second);
    let lighter = first_luminance.max(second_luminance);
    let darker = first_luminance.min(second_luminance);
    (lighter + 0.05) / (darker + 0.05)
}

fn relative_luminance(color: [f64; 4]) -> f64 {
    0.2126 * linear_srgb(color[0]) + 0.7152 * linear_srgb(color[1]) + 0.0722 * linear_srgb(color[2])
}

fn linear_srgb(channel: f64) -> f64 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}
