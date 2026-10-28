//! gpui-component 集成冒烟：确认 init 与 Button 构造在钉死的 gpui 0.2.2 下可用。

use gpui::{rgba, TestAppContext};
use gpui_component::button::Button;

use crate::{init_components, sync_component_theme, Theme, ThemeId};

/// 场景：在 headless TestAppContext 中初始化组件库并构造带 label 的 Button。
///
/// 数据构造：空 TestAppContext；Button id `"smoke"`，label `"OK"`。
/// 执行过程：`init_components` → 再次 `init_components`（幂等）→ `Button::new(...).label(...)`。
/// 预期结果：
///   - 正断言：两次 init 不 panic；Button 可构造
///   - 负断言：init 标记全局已存在（第二次走早退，不会二次 `set_global` panic）
#[test]
fn init_components_is_idempotent_and_button_constructs() {
    let cx = TestAppContext::single();
    cx.update(|cx| {
        init_components(cx);
        assert!(
            cx.has_global::<crate::component::Theme>(),
            "首次 init 后应安装 gpui-component Theme 全局状态"
        );
        init_components(cx);
        let _button = Button::new("smoke").label("OK");
    });
}

/// 克隆输入框主题同步：浅色 Norma 主题必须覆盖组件库残留的深色正文色。
///
/// 数据构造（含关键数值的推导过程）：
///   stale foreground = `0xffffffff`（深色系统主题使用的白色正文）
///   stale muted      = `0xffffff80`（白色、50% alpha 的占位文字）
///   Paper Blue input = `Theme::paper_blue()`（浅色克隆弹窗实际使用的主题）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 初始化 gpui-component 全局主题。
///   2. 注入深色系统主题残留的白色正文与占位色，复现浅色弹窗中的低对比度前置状态。
///   3. 调用 Norma 主题同步入口，再读取组件库实际用于 Input 绘制的颜色。
///
/// 预期结果：
///   - 正断言：Input 正文色等于 Norma 的 `text_primary`，占位色等于 `text_secondary`。
///   - 正断言：光标色等于 Norma 的强调色，选择色使用同一强调色。
///   - 负断言：Input 正文色不再是复现用的纯白色。
#[test]
fn sync_component_theme_aligns_input_text_with_norma_theme() {
    let cx = TestAppContext::single();
    cx.update(|cx| {
        init_components(cx);
        let norma_theme = Theme::paper_blue();
        let component_theme = crate::component::Theme::global_mut(cx);
        component_theme.colors.foreground = rgba(0xffffffff).into();
        component_theme.colors.muted_foreground = rgba(0xffffff80).into();

        sync_component_theme(ThemeId::PaperBlue, cx);

        let component_theme = crate::component::Theme::global(cx);
        assert_eq!(
            component_theme.colors.foreground,
            rgba(norma_theme.text_primary).into(),
            "克隆输入框的非空正文必须使用 Norma 当前主题的 text_primary"
        );
        assert_eq!(
            component_theme.colors.muted_foreground,
            rgba(norma_theme.text_secondary).into(),
            "克隆输入框的占位文字必须使用 Norma 当前主题的 text_secondary"
        );
        assert_eq!(
            component_theme.colors.caret,
            rgba(norma_theme.accent).into(),
            "克隆输入框的光标必须使用 Norma 当前主题的强调色"
        );
        assert_eq!(
            component_theme.colors.selection,
            rgba((norma_theme.accent & 0xffffff00) | 0x52).into(),
            "克隆输入框的选择色必须使用约 32% alpha 的 Norma 强调色"
        );
        assert_eq!(
            component_theme.mode,
            crate::component::ThemeMode::Light,
            "Paper Blue 必须把组件主题切换到 Light，不能继续沿用 macOS 深色外观"
        );
        assert_ne!(
            component_theme.colors.foreground,
            rgba(0xffffffff).into(),
            "浅色克隆弹窗的正文色不得继续沿用深色系统主题的纯白色"
        );
    });
}

/// 场景：深色主题的浅亮强调按钮不能继续叠白色文字。
/// 数据构造：遍历六套产品主题，取组件实际 primary 与 primary_foreground；4.5 为小字对比下限。
/// 执行过程：同步主题，再以 WCAG sRGB 亮度计算最终不透明前后景对比。
/// 预期结果：六套主题均达到 4.5:1，尤其墨黑银灰按钮不出现白字。
#[test]
fn dark_accent_button_foregrounds_remain_readable() {
    fn luminance(color: gpui::Hsla) -> f32 {
        let rgba: gpui::Rgba = color.into();
        let [r, g, b] = [rgba.r, rgba.g, rgba.b].map(|v| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        r * 0.2126 + g * 0.7152 + b * 0.0722
    }
    let cx = TestAppContext::single();
    cx.update(|cx| {
        for id in ThemeId::ALL {
            sync_component_theme(id, cx);
            let colors = &crate::component::Theme::global(cx).colors;
            let foreground = luminance(colors.primary_foreground);
            for background_color in [colors.primary, colors.primary_hover, colors.primary_active] {
                let background = luminance(background_color);
                let ratio =
                    (background.max(foreground) + 0.05) / (background.min(foreground) + 0.05);
                assert!(
                    ratio >= 4.5,
                    "{id:?} 强调按钮对比应至少 4.5:1，实际 {ratio}"
                );
            }
            assert_ne!(
                colors.primary, colors.primary_foreground,
                "{id:?} 文字不能融入强调底色"
            );
        }
    });
}
