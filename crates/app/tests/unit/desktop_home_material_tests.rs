use super::home_theme;

/// 首页材质在深、浅主题下都有独立的静止、悬停、按下和键盘焦点反馈。
///
/// 数据构造（含关键数值的推导过程）：
///   6 个现有主题，首页从各自的 Theme 派生局部材质；不写入主题设置。
///   静止面板直接采用各主题 Dock 的颜色与透明度，三级文字采用顶部栏的浅色前景。
///   预览底色 36/255≈14% 不透明，叠在卡片上仍能透出壁纸。
/// 执行过程：
///   1. 读取首页局部主题，逐项比较静止、悬停、按下和焦点 token。
///   2. 比较面板、描边及文字与顶部/底部材质，并检查预览不会成为不透明底。
/// 预期结果：
///   - 正断言：主文字与背景不同；焦点有独立描边；所有主题与顶部/底部材质一致。
///   - 负断言：hover/pressed 不与静止色相同；局部材质不能修改全局主题。
#[test]
fn desktop_home_material_has_distinct_interaction_states() {
    for id in ui::ThemeId::ALL {
        let original = ui::Theme::for_id(id);
        let theme = home_theme(id);
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
        assert_eq!(theme.accent, 0x0969daff, "首页主操作应使用第三张的实心蓝色");
        let chrome = crate::app::top_bar_theme(original);
        assert_eq!(theme.panel, original.dock, "{id:?} 卡片必须复用 Dock 材质");
        assert_eq!(
            theme.node_stroke, original.dock_line,
            "{id:?} 描边需与 Dock 一致"
        );
        assert_eq!(
            theme.text_primary, chrome.text_primary,
            "{id:?} 名称需沿用顶部浅色文字"
        );
        assert_eq!(
            theme.text_secondary, chrome.text_secondary,
            "{id:?} 数量需沿用顶部辅助文字"
        );
        assert_eq!(
            theme.text_tertiary, chrome.text_tertiary,
            "{id:?} 空态文字需保持可读对比"
        );
        assert!(
            theme.canvas_to & 0xff <= 36,
            "{id:?} 预览衬底只能是轻量透明 tint"
        );
        assert_ne!(theme.canvas_to & 0xff, 255, "{id:?} 预览不得恢复不透明白底");
    }
}
