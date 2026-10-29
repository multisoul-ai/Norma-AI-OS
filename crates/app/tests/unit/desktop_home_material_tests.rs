use super::home_theme;

/// 场景：Combo固定深色玻璃在所有主题保留Material状态层。
/// 数据：6主题；卡片66%=168/255，预览255/255；8%/12%层由视图合成。
/// 执行：取每个主题的首页局部token，对比默认、悬停、按下和焦点。
/// 预期：深浅主题都使用同一可读玻璃；预览不透明且原主题不被修改。
#[test]
fn desktop_home_material_has_distinct_interaction_states() {
    for id in ui::ThemeId::ALL {
        let original = ui::Theme::for_id(id);
        let theme = home_theme(id);
        assert_eq!(theme.panel, 0x10161ea8, "卡片按最终稿66%玻璃");
        assert_eq!(theme.canvas_to, 0x090d13ff, "预览必须不透明");
        assert_eq!(theme.focused_stroke, 0xa8c7faff, "焦点使用主色环");
        assert_ne!(theme.panel, theme.panel_high, "悬停有状态层");
        assert_ne!(theme.panel_high, theme.node_fill, "按下区别于悬停");
        assert_ne!(theme.node_stroke, theme.focused_stroke, "焦点区别普通描边");
        assert_eq!(
            ui::Theme::for_id(id).dock,
            original.dock,
            "首页局部token不改变全局主题"
        );
    }
}
