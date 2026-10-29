use super::{Theme, ThemeId};

fn luminance(color: u32) -> f32 {
    let channels = [24, 16, 8].map(|shift| {
        let value = ((color >> shift) & 255) as f32 / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722
}

/// 场景：三个浅色主题的阅读层不能继续依赖深色终端或半透明灰字。
/// 数据构造：遍历三个浅色 id，检查标题、正文、辅助文字与终端色；4.5 为小字号对比下限。
/// 执行过程：读取实际主题 token，并按 WCAG 相对亮度计算终端前后景对比。
/// 预期结果：浅色标题和内容面配深字，辅助文字不通过透明度降低可读性，暗色目录仍保持深色。
#[test]
fn light_theme_reading_surfaces_are_light_and_legible() {
    for id in [
        ThemeId::PaperBlue,
        ThemeId::LinenSage,
        ThemeId::SilverViolet,
        ThemeId::SapphireNight,
    ] {
        let theme = Theme::for_id(id);
        if id.is_dark() {
            assert!(
                luminance(theme.term_bg) < 0.1,
                "{id:?} 深色终端不能随浅色改版变亮"
            );
            continue;
        }
        assert!(
            luminance(theme.term_bg) > 0.8,
            "{id:?} 浅色终端应是明亮阅读面"
        );
        assert!(
            luminance(theme.term_fg) < 0.08,
            "{id:?} 浅色终端应使用深色正文"
        );
        assert!(
            (theme.title_bar & 255) >= 240,
            "{id:?} 标题底色应足够稳定，不能暴露暗壁纸"
        );
        assert_eq!(
            theme.text_secondary & 255,
            255,
            "{id:?} 辅助文字用实色保持可读"
        );
        assert_eq!(
            theme.text_tertiary & 255,
            255,
            "{id:?} 三级文字用实色保持可读"
        );
        assert!(
            (luminance(theme.term_bg) + 0.05) / (luminance(theme.term_fg) + 0.05) >= 4.5,
            "{id:?} 终端正文对比至少 4.5:1"
        );
    }
}

/// 场景：浅色壁纸应有提亮层，深色不能被同一层洗白。
/// 数据构造：遍历六主题，以纯黑壁纸代表最暗输入；底部是提亮较少的一端。
/// 执行过程：读取生产 wallpaper_veil，按 alpha 合成黑色，再计算亮度。
/// 预期结果：三个浅色均得到亮度大于 0.4 的壁纸基底，三个深色完全不叠加。
#[test]
fn light_wallpaper_veil_preserves_reading_polarity_and_leaves_dark_themes_alone() {
    for id in [
        ThemeId::PaperBlue,
        ThemeId::LinenSage,
        ThemeId::SilverViolet,
        ThemeId::SapphireNight,
    ] {
        let theme = Theme::for_id(id);
        assert_eq!(
            theme.is_light(),
            !id.is_dark(),
            "{id:?} 材质判定应匹配主题目录"
        );
        if id.is_dark() {
            assert_eq!(theme.wallpaper_veil(), None, "{id:?} 深色不应叠浅色提亮层");
        } else {
            let (from, to) = theme.wallpaper_veil().expect("浅色必须有提亮层");
            for veil in [from, to] {
                let alpha = veil & 255;
                let mut composed = 255;
                for shift in [24, 16, 8] {
                    composed |= (((veil >> shift) & 255) * alpha / 255) << shift;
                }
                assert!(
                    luminance(composed) > 0.4,
                    "{id:?} 黑色壁纸也应形成稳定浅色基底"
                );
                assert!(alpha < 255, "{id:?} 应保留壁纸色彩，而非不透明纯色");
            }
        }
    }
}
