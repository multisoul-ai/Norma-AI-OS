use super::{
    ideas_tabs::tab_feedback,
    overview_brain::{pill_colors, PillStyle},
    overview_parts::status_colors,
};
use crate::workspace::desktop::{
    overview_rows::{overview_feedback, OverviewStatusTone},
    surface_theme::surface_theme,
};

fn over(fg: u32, bg: [f32; 3]) -> [f32; 3] {
    let alpha = (fg & 255) as f32 / 255.0;
    [24, 16, 8].map(|shift| {
        ((fg >> shift) & 255) as f32 / 255.0 * alpha + bg[(24 - shift) / 8] * (1.0 - alpha)
    })
}
fn luma(v: [f32; 3]) -> f32 {
    let v = v.map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    v[0] * 0.2126 + v[1] * 0.7152 + v[2] * 0.0722
}
fn contrast(ink: u32, bg: [f32; 3]) -> f32 {
    let (a, b) = (luma(over(ink, bg)), luma(bg));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// 场景：真实运行/审批/空闲标签和幕僚按钮不能被亮壁纸冲淡。
/// 数据构造：六套生产主题、60%抽屉蒙层以及黑白壁纸亮度两端。
/// 执行过程：调用渲染使用的配色，依次合成壁纸、蒙层、状态底色。
/// 预期结果：状态字和可操作按钮的rest/hover/pressed均达到4.5:1。
#[test]
fn drawer_status_and_button_contrast() {
    for id in ui::ThemeId::ALL {
        let theme = surface_theme(ui::Theme::for_id(id));
        for wallpaper in [[0.0; 3], [1.0; 3]] {
            let (_, tint) = super::material::drawer_tint(theme, true);
            let bg = over(tint, wallpaper);
            for tone in [
                OverviewStatusTone::Running,
                OverviewStatusTone::Approval,
                OverviewStatusTone::Idle,
            ] {
                let (fill, ink) = status_colors(theme, tone);
                let ratio = contrast(ink, over(fill, bg));
                assert!(ratio >= 4.5, "{id:?} {tone:?} 标签对比{ratio}应达到4.5:1");
            }
            for style in [PillStyle::Primary, PillStyle::Quiet] {
                let (rest, hover, pressed, ink) = pill_colors(theme, style);
                for fill in [rest, hover, pressed] {
                    let ratio = contrast(ink, over(fill, bg));
                    assert!(ratio >= 4.5, "{id:?} 按钮对比{ratio}应达到4.5:1");
                }
            }
        }
    }
}

/// 场景：已经选中的想法分段仍需对悬停和按压即时反馈。
/// 数据构造：六套主题，选中/未选中两种状态。
/// 执行过程：读取实际渲染使用的状态层配色。
/// 预期结果：rest、hover和pressed两两不同，选择不能吞掉反馈。
#[test]
fn selected_idea_tab_retains_feedback() {
    for id in ui::ThemeId::ALL {
        let theme = surface_theme(ui::Theme::for_id(id));
        for selected in [true, false] {
            let states = tab_feedback(theme, selected, overview_feedback(theme));
            assert_ne!(
                states.rest, states.hover,
                "{id:?} selected={selected} 悬停必须有反馈"
            );
            assert_ne!(
                states.hover, states.pressed,
                "{id:?} selected={selected} 按下应比悬停更明确"
            );
            assert_ne!(
                states.rest, states.pressed,
                "{id:?} selected={selected} 按下不能等同静止"
            );
        }
    }
}
