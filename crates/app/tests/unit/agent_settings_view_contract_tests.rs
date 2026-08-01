/// AS-18: Agents 设置页应采用双栏 master-detail，并只在用户点击 Edit 后显示编辑表单。
///
/// 数据构造（含关键数值的推导过程）：
///   source file count        = 3（view.rs + view/list.rs + actions.rs）
///   required layout anchors  = 5（双栏容器、Agent 列表、只读详情、Edit、Add to Canvas）
///   editing anchor count     = 2（编辑详情、Save），只能由显式 Edit action 进入
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取双栏容器、左侧列表和 action 源码
///   2. 检查左侧列表只负责单击选择，不再通过双击展开卡片内编辑
///   3. 检查右侧同时定义默认只读摘要与显式 Edit action
///   4. 检查 Edit handler 调用 begin_card_edit，且 Add to Canvas 保留独立入口
///
/// 预期结果：
///   - 正断言：双栏、列表、只读详情、Edit、Add to Canvas 锚点全部存在
///   - 正断言：Edit handler 明确进入 draft 编辑状态并提供 Save/Cancel
///   - 负断言：列表不应继续依赖 click_count 双击或 inline card editing
///   - 负断言：Add to Canvas 不应被编辑动作替代或隐藏
#[test]
fn test_agents_settings_view_uses_readonly_master_detail_contract() {
    let view_source = include_str!("../../src/settings/agents/view.rs");
    let list_source = include_str!("../../src/settings/agents/view/list.rs");
    let actions_source = include_str!("../../src/settings/agents/actions.rs");

    for anchor in [
        "settings-agents-master-detail",
        "settings-agent-detail-readonly",
        "settings-agent-edit",
        "settings-agent-add-to-canvas",
        "settings-agent-detail-editing",
    ] {
        assert!(
            view_source.contains(anchor),
            "Agents master-detail view should expose stable anchor: {anchor}"
        );
    }
    assert!(
        list_source.contains("settings-agents-list")
            && list_source.contains("on_agent_settings_select_definition"),
        "left Agent list should expose a stable list anchor and select definitions on click"
    );
    assert!(
        view_source.contains("on_agent_settings_begin_detail_edit")
            && actions_source.contains("fn on_agent_settings_begin_detail_edit")
            && actions_source.contains("self.agent_definitions.begin_card_edit(id);"),
        "explicit Edit should enter the existing draft edit state through the detail action"
    );
    assert!(
        view_source.contains("settings-agent-save")
            && view_source.contains("settings-agent-cancel-edit"),
        "editing detail should expose concrete Save and Cancel completion paths"
    );
    assert!(
        view_source.contains("\"Add to Canvas\"")
            && view_source.contains("on_agent_settings_create_card"),
        "read-only detail must preserve the important Add to Canvas action and its spawn handler"
    );
    assert!(
        !list_source.contains("click_count"),
        "left Agent selection should not require a hidden double-click editing gesture"
    );
    assert!(
        !list_source.contains("settings-agent-card-editing"),
        "editing should stay in the right detail pane instead of expanding inline inside the list"
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

    let readable_runtime_profile = ".text_color(rgba(with_alpha(theme.text_primary, 0xc0)))\n                .child(definition.runtime_profile.display_title().to_string())";
    let unreadable_runtime_profile = ".text_color(rgba(theme.text_tertiary))\n                .child(definition.runtime_profile.display_title().to_string())";
    assert!(
        list_source.contains(readable_runtime_profile),
        "Agent 列表 Runtime Profile 必须使用 text_primary RGB + 0xc0 alpha 的可读 muted 颜色"
    );
    assert!(
        !list_source.contains(unreadable_runtime_profile),
        "Agent 列表 Runtime Profile 不得继续使用浅色模式仅约 2.19:1 的 text_tertiary"
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
