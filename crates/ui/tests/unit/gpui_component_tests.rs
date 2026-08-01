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
