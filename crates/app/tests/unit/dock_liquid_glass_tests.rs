use super::{dock_glass_style, DOCK_GLASS_MAX_LUMINANCE};
use crate::app::top_bar_theme as command_panel_theme;
use ui::{Theme, ThemeId};

fn linear_channel(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// Contrast of `foreground` (RGBA hex) over a neutral gray whose linear luminance is
/// `backdrop`, after the foreground's alpha is composited onto that gray.
fn contrast_over_luminance(foreground: u32, backdrop: f32) -> f32 {
    let alpha = (foreground & 0xff) as f32 / 255.0;
    let text = [24, 16, 8].map(|shift| {
        let channel = linear_channel(((foreground >> shift) & 0xff) as f32 / 255.0);
        channel * alpha + backdrop * (1.0 - alpha)
    });
    let text = 0.2126 * text[0] + 0.7152 * text[1] + 0.0722 * text[2];
    (text.max(backdrop) + 0.05) / (text.min(backdrop) + 0.05)
}

/// 场景：Dock 改为液态玻璃后，白色壁纸是最不利背景；玻璃的自适应压暗必须保住文字对比度。
///
/// 数据构造：遍历 ThemeId::ALL，取 command_panel_theme 的 text_primary / text_secondary，
///   以及 Dock 图标色 0xffffffd6；最亮背景被着色器压到亮度上限 DOCK_GLASS_MAX_LUMINANCE = 0.15。
/// 执行过程：把前景按 alpha 合成到亮度为 0.15 的中性灰上，计算 WCAG 对比度。
///   纯白时 (1.0 + 0.05) / (0.15 + 0.05) = 5.25。
/// 预期结果：三种前景在每个主题下都 ≥ 4.5:1；上限本身不得超过 0.183（白字 4.5:1 的临界亮度）。
#[test]
fn dock_glass_keeps_text_readable_over_white_wallpaper() {
    assert!(
        DOCK_GLASS_MAX_LUMINANCE <= 0.183,
        "亮度上限 {DOCK_GLASS_MAX_LUMINANCE} 超过白字 4.5:1 的临界值 0.183"
    );
    for id in ThemeId::ALL {
        let panel = command_panel_theme(Theme::for_id(id));
        for (label, color) in [
            ("主文字", panel.text_primary),
            ("次要文字", panel.text_secondary),
            ("图标", 0xffffffd6),
        ] {
            let contrast = contrast_over_luminance(color, DOCK_GLASS_MAX_LUMINANCE);
            assert!(
                contrast >= 4.5,
                "{id:?} 的{label}在玻璃上的对比度 {contrast:.2} 低于 4.5:1"
            );
        }
    }
}

/// 场景：液态玻璃必须仍是“玻璃”，而不是换个名字的不透明底。
///
/// 数据构造：每个主题的 dock_glass_style。
/// 执行过程：读取着色强度、折射、边缘与高光参数。
/// 预期结果：着色 alpha 在 (0, 0.5) 之间，背景可透出；折射 > 0、边缘宽度 ≥ 折射，
///   高光与色散处于 (0, 1]；模糊半径小于折射，避免退化成磨砂层。
#[test]
fn dock_glass_style_is_translucent_and_refractive() {
    for id in ThemeId::ALL {
        let style = dock_glass_style(Theme::for_id(id));
        assert!(
            style.tint.a > 0.0 && style.tint.a < 0.5,
            "{id:?} 的玻璃着色 alpha {} 必须半透明",
            style.tint.a
        );
        assert!(style.refraction > gpui::px(0.0), "{id:?} 必须有边缘折射");
        assert!(
            style.bezel >= style.refraction,
            "{id:?} 的边缘宽度必须容纳折射位移"
        );
        assert!(
            style.blur < style.refraction,
            "{id:?} 的模糊不能盖过折射，否则就是磨砂层"
        );
        assert!(
            style.specular > 0.0 && style.specular <= 1.0,
            "{id:?} 的边缘高光必须可见且不过曝"
        );
        assert!(
            style.dispersion > 0.0 && style.dispersion <= 1.0,
            "{id:?} 的色散必须在 (0, 1] 内"
        );
    }
}
