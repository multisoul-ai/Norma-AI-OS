use super::home_theme;

/// 场景：首页应与画布使用同一个现有主题，并保留可区分的交互状态。
/// 数据构造：遍历六个 ThemeId，将首页颜色与所选主题比较；透明度不影响 RGB 色相断言。
/// 执行过程：读取 home_theme 的静止、悬停、按下、焦点、正文与强调色。
/// 预期结果：前景与强调色跟随选择，背景使用该主题色系，不能固定墨黑背景和白色正文。
#[test]
fn desktop_home_material_has_distinct_interaction_states() {
    for id in ui::ThemeId::ALL {
        let original = ui::Theme::for_id(id);
        let theme = home_theme(id);
        assert_eq!(
            theme.text_primary, original.text_primary,
            "{id:?} 首页正文不能固定白色"
        );
        assert_eq!(
            theme.accent, original.accent,
            "{id:?} 首页强调色应跟随所选主题"
        );
        assert_eq!(
            theme.panel >> 8,
            original.canvas_from >> 8,
            "{id:?} 首页底色不能固定墨黑"
        );
        assert_ne!(theme.panel, theme.panel_high, "{id:?} 悬停必须改变卡片底色");
        assert_ne!(theme.panel, theme.node_fill, "{id:?} 按下必须与静止不同");
        assert_ne!(
            theme.panel_high, theme.node_fill,
            "{id:?} 按下必须与悬停不同"
        );
        assert_ne!(
            theme.node_stroke, theme.focused_stroke,
            "{id:?} 键盘焦点需独立描边"
        );
        assert_ne!(theme.panel, theme.text_primary, "{id:?} 名称不能与背景同色");
    }
}
