fn home_text_contrast(text: u32, surface: u32, wallpaper: u32) -> f32 {
    let composite = |foreground: u32, background: [f32; 3]| {
        let alpha = (foreground & 0xff) as f32 / 255.0;
        [24, 16, 8].map(|shift| {
            ((foreground >> shift) & 0xff) as f32 / 255.0 * alpha
                + background[(24 - shift) / 8] * (1.0 - alpha)
        })
    };
    let luminance = |color: [f32; 3]| {
        let linear = color.map(|channel| {
            if channel <= 0.04045 {
                channel / 12.92
            } else {
                ((channel + 0.055) / 1.055).powf(2.4)
            }
        });
        linear[0] * 0.2126 + linear[1] * 0.7152 + linear[2] * 0.0722
    };
    let background = composite(surface, composite(wallpaper, [0.0; 3]));
    let foreground = luminance(composite(text, background));
    let background = luminance(background);
    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05)
}

/// 首页卡片的项目数量和空态提示在亮、暗壁纸上都保持可读对比。
///
/// 数据构造（含关键数值的推导过程）：
///   6个当前主题和3个旧浅色夹具；纯白/纯黑壁纸覆盖sRGB亮度两端。
///   卡片按Combo稿使用66%深色玻璃；合成色 = 前景 × alpha + 背景 × (1-alpha)。
///   不透明度约66%：255×0.66≈168，保留壁纸并支撑文字。
///   WCAG 小字号对比下限 = 4.5:1；相对亮度按 sRGB 线性化及 0.2126/0.7152/0.0722 加权。
/// 执行过程：
///   1. 读取首页卡片材质及两级辅助文字颜色。
///   2. 分别叠加到纯白、纯黑壁纸，计算项目数量与空态提示的最终对比。
///
/// 预期结果：
///   - 正断言：数量、空态提示及实心主按钮文字的对比都达到 4.5:1。
///   - 负断言：不能变成不透明色块，也不能透明到让壁纸覆盖文字。
#[test]
fn desktop_home_card_text_remains_readable_over_wallpaper() {
    use super::super::home::home_theme;
    for id in ui::ThemeId::ALL.into_iter().chain([
        ui::ThemeId::PaperBlue,
        ui::ThemeId::LinenSage,
        ui::ThemeId::SilverViolet,
    ]) {
        let theme = home_theme(id);
        let surface = theme.panel;
        assert_eq!(surface, 0x10161ea8, "{id:?} 首页卡片遵循Combo深色玻璃设计");
        let alpha = surface & 0xff;
        assert!(alpha >= 167, "{id:?} 卡片材质需支撑亮壁纸上的文字");
        assert!(alpha <= 169, "{id:?} 卡片必须透出壁纸，不能成为不透明色块");
        assert!(
            home_text_contrast(theme.text_secondary, surface, 0xffffffff) >= 4.5,
            "{id:?} 项目数量必须在纯白壁纸上保持至少 4.5:1 对比"
        );
        assert!(
            home_text_contrast(theme.text_tertiary, surface, 0xffffffff) >= 4.5,
            "{id:?} 空态提示必须在纯白壁纸上保持至少 4.5:1 对比"
        );
        assert!(
            home_text_contrast(theme.text_secondary, surface, 0x000000ff) >= 4.5,
            "{id:?} 项目数量必须在纯黑壁纸上保持至少 4.5:1 对比"
        );
        assert!(
            home_text_contrast(theme.text_tertiary, surface, 0x000000ff) >= 4.5,
            "{id:?} 空态提示必须在纯黑壁纸上保持至少 4.5:1 对比"
        );
    }
    for id in ui::ThemeId::ALL {
        let theme = home_theme(id);
        assert!(
            home_text_contrast(
                super::super::surface_theme::on_accent(theme),
                theme.accent,
                0xffffffff
            ) >= 4.5,
            "{id:?} 实心主按钮应使用对比至少 4.5:1 的文字"
        );
    }
}

/// 顶部透明材质透出壁纸，同时保留工作空间、项目和状态文字的可读性。
///
/// 数据构造（含关键数值的推导过程）：
///   6 个主题，纯白/纯黑壁纸覆盖最亮/最暗背景；共 6×2 种组合。
///   顶栏必须取对应桌面表面的 title_bar/dock_line，三级文字跟随所选主题，Dock 单独保持深色材质。
///   材质不透明度范围 65%–85%：255×0.65≈166，255×0.85≈217。
///   最终色 = 材质×alpha + 壁纸×(1-alpha)；小字号 WCAG 对比下限为 4.5:1。
/// 执行过程：
///   1. 读取首页、项目总览和画布共用的顶部材质及三级文字。
///   2. 合成两端壁纸，分别计算主标题、空间/状态文字、分隔符的对比。
///
/// 预期结果：
///   - 正断言：三级文字在两端壁纸上均达到 4.5:1。
///   - 负断言：顶部不能恢复固定深色标题栏，也不能透明到削弱文字辨识。
#[test]
fn desktop_top_bar_translucent_text_remains_readable_over_wallpaper() {
    for id in ui::ThemeId::ALL {
        let theme = crate::app::top_bar_theme(ui::Theme::for_id(id));
        let original = ui::Theme::for_id(id);
        assert_eq!(
            theme.text_primary, original.text_primary,
            "{id:?} 顶栏文字应跟随主题"
        );
        assert_eq!(
            theme.title_bar, theme.panel_high,
            "{id:?} 顶栏应使用同一表面层级"
        );
        assert_ne!(
            theme.title_bar, original.dock,
            "{id:?} 顶栏不应回退为深色 Dock"
        );
        assert!(
            home_text_contrast(theme.text_primary, theme.title_bar, 0xffffffff) >= 4.5,
            "{id:?} 主标题必须在纯白壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_primary, theme.title_bar, 0x000000ff) >= 4.5,
            "{id:?} 主标题必须在纯黑壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_secondary, theme.title_bar, 0xffffffff) >= 4.5,
            "{id:?} 工作空间和保存状态必须在纯白壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_secondary, theme.title_bar, 0x000000ff) >= 4.5,
            "{id:?} 工作空间和保存状态必须在纯黑壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_tertiary, theme.title_bar, 0xffffffff) >= 4.5,
            "{id:?} 层级分隔符必须在纯白壁纸上达到 4.5:1"
        );
        assert!(
            home_text_contrast(theme.text_tertiary, theme.title_bar, 0x000000ff) >= 4.5,
            "{id:?} 层级分隔符必须在纯黑壁纸上达到 4.5:1"
        );
    }
}
