use super::home_theme;

/// 固定地球背景上的首页使用墨黑中性材质，蓝色只标识主操作和键盘焦点。
///
/// 数据构造（含关键数值的推导过程）：
///   6 个现有主题，首页卡片使用墨黑 0x0d131be0（224/255≈88%）。
///   预览底色 0x080d15db（219/255≈86%），保留地球轮廓但压住高亮。
///   墨黑 RGB 的蓝红差 = 27-13=14；亮蓝操作色蓝红差 = 165-43=122。
/// 执行过程：
///   1. 读取每个全局主题对应的首页局部主题。
///   2. 比较静止、悬停、按下、焦点与固定背景下的预览材质。
///
/// 预期结果：
///   - 正断言：固定中性材质与低饱和蓝操作色在所有主题相同，焦点可辨。
///   - 负断言：hover/pressed 不与静止色相同；首页不能更改工作画布的 Dock 配色。
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
        assert_eq!(theme.accent, 0x2b6aa5ff, "首页主操作应使用克制的蓝色");
        let chrome = crate::app::top_bar_theme(original);
        assert_eq!(
            theme.panel, 0x0d131be0,
            "{id:?} 首页卡片必须使用固定墨黑材质"
        );
        assert_eq!(
            theme.dock, 0x080d14ed,
            "{id:?} 首页顶栏必须使用固定墨黑材质"
        );
        assert_eq!(
            crate::app::top_bar_theme(original).dock,
            original.dock,
            "{id:?} 固定首页配色不能改变项目画布的 Dock"
        );
        assert_eq!(
            theme.text_primary, chrome.text_primary,
            "{id:?} 名称需沿用顶部浅色文字"
        );
        assert_eq!(
            theme.text_secondary, 0xcbd4dee8,
            "{id:?} 数量应使用柔和的冷灰，低于主标题亮度"
        );
        assert_eq!(
            theme.text_tertiary, 0xb1becbc8,
            "{id:?} 空态文字应进一步退后，同时保持可读对比"
        );
        assert_eq!(
            theme.canvas_to, 0x080d15db,
            "{id:?} 预览应使用稳定的深色底色"
        );
        assert_ne!(theme.canvas_to & 0xff, 255, "{id:?} 预览不得恢复不透明白底");
        assert!(
            ((theme.panel >> 8) & 0xff) - ((theme.panel >> 24) & 0xff) <= 24,
            "{id:?} 大面积卡片不能继续使用明显偏蓝的材质"
        );
        assert_ne!(
            theme.edge_highlight, theme.focused_stroke,
            "{id:?} 普通卡片边框不能和键盘焦点使用同一亮蓝色"
        );
    }
}
