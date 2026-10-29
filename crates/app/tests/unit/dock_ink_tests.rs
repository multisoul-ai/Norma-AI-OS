use super::*;

/// 场景：Dock 图标前景必须跟随表面极性，不能把固定白色带入浅色 Dock。
/// 数据构造：六主题与 selected/unselected 两状态。
/// 执行过程：调用真正绘制图标使用的色彩函数。
/// 预期结果：浅色状态使用深字与强调色，深色状态保留原白字，不出现白底白图标。
#[test]
fn dock_icon_ink_tracks_theme_and_selected_state() {
    for id in ui::ThemeId::ALL {
        let theme = Theme::for_id(id);
        if id.is_dark() {
            assert_eq!(
                dock_icon_color(theme, false),
                0xffffffd6,
                "{id:?} 深色普通图标保持白色"
            );
            assert_eq!(
                dock_icon_color(theme, true),
                0xffffffff,
                "{id:?} 深色选中图标保持白色"
            );
        } else {
            assert_eq!(
                dock_icon_color(theme, false),
                theme.text_secondary,
                "{id:?} 普通图标应采用深字"
            );
            assert_eq!(
                dock_icon_color(theme, true),
                theme.accent,
                "{id:?} 选中应采用强调色"
            );
            assert_ne!(
                dock_icon_color(theme, false),
                0xffffffd6,
                "{id:?} 浅色 Dock 不能使用固定白图标"
            );
        }
    }
}
