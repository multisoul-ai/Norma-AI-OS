use super::view::{command_dock_surface, command_panel_theme};
use ui::{Theme, ThemeId};

fn linear_channel(channel: f32) -> f32 {
    if channel <= 0.04045 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * linear_channel(rgb[0])
        + 0.7152 * linear_channel(rgb[1])
        + 0.0722 * linear_channel(rgb[2])
}

fn over_white(color: u32) -> [f32; 3] {
    let alpha = (color & 0xff) as f32 / 255.0;
    [24, 16, 8].map(|shift| {
        let channel = ((color >> shift) & 0xff) as f32 / 255.0;
        channel * alpha + (1.0 - alpha)
    })
}

fn contrast_over_dock(foreground: u32, dock: u32) -> f32 {
    let backdrop = over_white(dock);
    let alpha = (foreground & 0xff) as f32 / 255.0;
    let text = [24, 16, 8].map(|shift| ((foreground >> shift) & 0xff) as f32 / 255.0 * alpha);
    let text = [0, 1, 2].map(|i| text[i] + backdrop[i] * (1.0 - alpha));
    let (a, b) = (luminance(text), luminance(backdrop));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// 场景：指令候选标题和说明在六主题 Dock 上均需可读。
/// 数据构造：遍历六主题，将生产 Dock 底色（command_dock_surface）与白壁纸合成；
/// 正文、说明与第三级文字需达到 4.5:1。
/// 执行过程：调用生产 command_panel_theme，计算真实 token 的 WCAG 对比。
/// 预期结果：浅色使用深字，深色使用浅字，说明不因透明度而丢失。
#[test]
fn command_panel_text_is_readable_on_dock_in_every_theme() {
    for id in ThemeId::ALL {
        let original = Theme::for_id(id);
        let panel = command_panel_theme(original);
        let dock = command_dock_surface(original);
        let primary = contrast_over_dock(panel.text_primary, dock);
        let secondary = contrast_over_dock(panel.text_secondary, dock);
        let tertiary = contrast_over_dock(panel.text_tertiary, dock);
        assert!(
            primary >= 4.5,
            "{id:?} 的面板主文字对比度 {primary:.2} 低于 4.5:1，文字会淹没在 Dock 底色里"
        );
        assert!(
            secondary >= 4.5,
            "{id:?} 的面板次要文字对比度 {secondary:.2} 低于 4.5:1，说明文字不可读"
        );
        assert!(
            tertiary >= 4.5,
            "{id:?} 的面板第三级文字对比度 {tertiary:.2} 低于 4.5:1"
        );
    }
}
