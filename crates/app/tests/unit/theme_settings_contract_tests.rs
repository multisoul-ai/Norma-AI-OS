/// 设置面板主题色：Settings 壳层与参考图式主题预览必须完全由主题 token 驱动。
///
/// 数据构造（含关键数值的推导过程）：
///   source = settings_panel.rs + settings_content.rs + settings_appearance.rs
///   shell tokens = panel / panel_high / text_primary / text_secondary / text_tertiary
///   preview tokens = canvas_from / canvas_to / title_bar / node_fill / node_stroke
///   selection token = accent
///   banned colors = 0xfffffff8 / 0xfbfbfdff / 0xe9e9efff / 0x24213dff
///                   （上一版参考图白色 Settings 面板残留）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 拼接 Settings 生产渲染源码
///   2. 检查面板壳层、主题预览与选中态是否由项目 token 驱动
///   3. 检查旧白底/深蓝文字常量是否被移除
///
/// 预期结果：
///   - 正断言：Settings 壳层引用 panel 与文字 token。
///   - 正断言：主题预览引用 canvas、title bar、node fill/stroke 与 accent token。
///   - 负断言：抽象设置窗口预览不要求 Terminal 内容井 token。
///   - 负断言：Settings 源码不应继续保留上一版白色面板硬编码常量
#[test]
fn test_settings_panel_and_theme_previews_use_tokens_not_hardcoded_palette() {
    let source = format!(
        "{}{}{}{}",
        include_str!("../../src/settings/panel.rs"),
        include_str!("../../src/settings/content.rs"),
        include_str!("../../src/settings/appearance.rs"),
        include_str!("../../src/settings/typography.rs")
    );
    assert!(
        source.contains("theme.panel") && source.contains("theme.panel_high"),
        "Settings 壳层必须同时消费 panel 与 panel_high token"
    );
    assert!(
        source.contains("theme.text_primary")
            && source.contains("theme.text_secondary")
            && source.contains("theme.text_tertiary"),
        "Settings 标题、正文与辅助文字必须消费三级文字 token"
    );
    assert!(
        source.contains("theme.canvas_from") && source.contains("theme.canvas_to"),
        "主题预览必须消费 Base / Raised canvas token"
    );
    assert!(
        source.contains("theme.title_bar"),
        "主题预览顶部窗口栏必须消费 title_bar token"
    );
    assert!(
        source.contains("theme.node_fill") && source.contains("theme.node_stroke"),
        "主题预览内容块必须消费 node fill/stroke token"
    );
    assert!(
        source.contains("preview.accent"),
        "每张主题卡的选中边框与单选标记必须消费该预览自己的 accent token"
    );
    assert!(
        !source.contains("theme.term_bg"),
        "参考图式设置窗口预览不应重新混入 Terminal 黑色内容井"
    );
    assert!(
        !source.contains("0xfffffff8") && !source.contains("0xfbfbfdff"),
        "Settings 不应保留上一版白色 mock 面板常量"
    );
    assert!(
        !source.contains("0xe9e9efff") && !source.contains("0x24213dff"),
        "Settings 不应保留上一版 mock 边线或深蓝文字常量"
    );
}
