use super::*;

fn luminance(color: Hsla) -> f32 {
    let color: gpui::Rgba = color.into();
    let linear = [color.r, color.g, color.b].map(|v| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
}

/// 场景：浅色终端不能继续使用暗底设计的高亮黄、白和青色正文。
/// 数据构造：三个浅色主题，16 个 ANSI 颜色；4.5:1 为小字号正文对比下限。
/// 执行过程：读取真正绘制使用的 palette，逐色与终端底色计算相对亮度对比。
/// 预期结果：16 色在浅底均可读；RGB 和 256 色 cube 不被主题重写。
#[test]
fn light_terminal_named_inks_are_readable_without_rewriting_application_colors() {
    for id in [
        ui::ThemeId::PaperBlue,
        ui::ThemeId::LinenSage,
        ui::ThemeId::SilverViolet,
    ] {
        let palette = TerminalPalette::from_theme(Theme::for_id(id));
        for (index, ink) in palette.ansi_colors.into_iter().enumerate() {
            let ratio = (luminance(palette.background) + 0.05) / (luminance(ink) + 0.05);
            assert!(
                ratio >= 4.5,
                "{id:?} ANSI {index} 对比 {ratio} 应达到 4.5:1"
            );
        }
        assert_eq!(
            palette.resolve(TerminalColor::Rgb {
                r: 17,
                g: 83,
                b: 149
            }),
            rgb_to_hsla(17, 83, 149),
            "{id:?} 应保留程序指定的 RGB"
        );
        assert_eq!(
            palette.resolve(TerminalColor::Indexed(196)),
            rgb_to_hsla(255, 0, 0),
            "{id:?} 256 色 cube 中的纯红必须保持原值"
        );
        assert_ne!(
            palette.ansi_colors[15],
            rgb_to_hsla(238, 238, 236),
            "{id:?} 浅底的命名亮白不能仍是近白正文"
        );
    }
}

/// 场景：浅色外框搭配深色终端的候选方案应继续使用深色 ANSI。
/// 数据构造：保留 Paper Blue 外框，换成 Sapphire Night 终端底色与正文。
/// 执行过程：比较混合方案和原深色 palette，再与浅色方案对照。
/// 预期结果：ANSI 根据终端阅读面选择，不能根据外框亮度误选。
#[test]
fn terminal_palette_polarity_follows_content_surface_not_shell() {
    let dark = Theme::glass_dark();
    let light = Theme::glass_light();
    let mixed = Theme {
        term_bg: dark.term_bg,
        term_fg: dark.term_fg,
        ..light
    };
    assert_eq!(
        TerminalPalette::from_theme(mixed).ansi_colors,
        TerminalPalette::from_theme(dark).ansi_colors,
        "深色内容井应保留原深色 ANSI"
    );
    assert_ne!(
        TerminalPalette::from_theme(mixed).ansi_colors,
        TerminalPalette::from_theme(light).ansi_colors,
        "浅色外框不能决定终端调色板"
    );
}
