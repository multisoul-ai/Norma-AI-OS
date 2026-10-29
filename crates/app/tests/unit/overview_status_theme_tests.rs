use super::BadgeTone;
use crate::workspace::desktop::surface_theme::{on_color, surface_theme};

fn over(fg: u32, bg: [f32; 3]) -> [f32; 3] {
    let alpha = (fg & 255) as f32 / 255.0;
    [24, 16, 8].map(|shift| {
        ((fg >> shift) & 255) as f32 / 255.0 * alpha + bg[(24 - shift) / 8] * (1.0 - alpha)
    })
}
fn luminance(rgb: [f32; 3]) -> f32 {
    let rgb = rgb.map(|c| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}
fn contrast(ink: u32, bg: [f32; 3]) -> f32 {
    let a = luminance(over(ink, bg));
    let b = luminance(bg);
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// 场景：11px 状态计数角标与待处理实心角标在六主题和相反亮度壁纸上都可读。
/// 数据构造：生产 BadgeTone 配色、主题提亮层与当前抽屉蒙层，黑/白壁纸覆盖亮度两端。
/// 执行过程：依次合成壁纸、抽屉、角标背景，再按 sRGB 线性化计算实际前景对比。
/// 预期结果：所有状态正文达到 4.5:1；实心角标不能沿用固定深字。
#[test]
fn desktop_status_badges_remain_readable_in_every_theme() {
    for id in ui::ThemeId::ALL {
        let theme = surface_theme(ui::Theme::for_id(id));
        for wallpaper in [[0.0; 3], [1.0; 3]] {
            for tone in [
                BadgeTone::Running,
                BadgeTone::Amber,
                BadgeTone::Idle,
                BadgeTone::Neutral,
            ] {
                let (fill, ink) = tone.colors(theme);
                let wallpaper = theme
                    .wallpaper_veil()
                    .map_or(wallpaper, |(_, to)| over(to, wallpaper));
                let (_, tint) = super::super::material::drawer_tint(theme, true);
                let background = over(fill, over(tint, wallpaper));
                let ratio = contrast(ink, background);
                assert!(
                    ratio >= 4.5,
                    "{id:?} 状态角标对比 {ratio:.2} 必须达到 4.5:1"
                );
            }
            let ratio = contrast(
                on_color(theme.accent_warm),
                over(theme.accent_warm, wallpaper),
            );
            assert!(
                ratio >= 4.5,
                "{id:?} 实心待处理角标对比 {ratio:.2} 必须达到 4.5:1"
            );
        }
    }
}
