use super::view::command_panel_theme;
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

/// 场景：浅色主题下快捷指令面板的标题、指令名与说明文字必须在深色 Dock 底上可读。
///
/// 数据构造：遍历 ThemeId::ALL；Dock 底色取 theme.dock，并按最不利的白色壁纸合成
///   （Paper Blue：0x313d44cc，alpha 0xcc/255 = 0.8 → 底色约 0x535e63）。
/// 执行过程：取 command_panel_theme(theme) 的 text_primary / text_secondary，
///   分别与合成后的 Dock 底色计算 WCAG 对比度。
///
/// 预期结果：
///   - 主文字对比度 ≥ 4.5：浅色主题下不能再用深色字叠深色 Dock，否则文字不可见。
///   - 次要文字（说明、补全）对比度 ≥ 4.5：小字号同样必须可读。
#[test]
fn command_panel_text_is_readable_on_dock_in_every_theme() {
    for id in ThemeId::ALL {
        let original = Theme::for_id(id);
        let panel = command_panel_theme(original);
        let primary = contrast_over_dock(panel.text_primary, original.dock);
        let secondary = contrast_over_dock(panel.text_secondary, original.dock);
        assert!(
            primary >= 4.5,
            "{id:?} 的面板主文字对比度 {primary:.2} 低于 4.5:1，文字会淹没在 Dock 底色里"
        );
        assert!(
            secondary >= 4.5,
            "{id:?} 的面板次要文字对比度 {secondary:.2} 低于 4.5:1，说明文字不可读"
        );
    }
}
