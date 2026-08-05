//! ui — 壳层（依赖 gpui）：可复用组件与主题。
//!
//! GUI 控件优先使用 [`component`]（`gpui-component`），非必要不要用 `div()` 重复造轮子。
pub mod icons;
pub mod theme;

pub use theme::{Theme, ThemeId};

/// Longbridge [`gpui-component`](https://longbridge.github.io/gpui-component/) 组件库。
///
/// 常用入口：`component::button::Button`、`component::input`、`component::dialog` 等。
pub use gpui_component as component;

use gpui::{rgba, App, Global};

struct GpuiComponentInitialized;

impl Global for GpuiComponentInitialized {}

/// 初始化 `gpui-component`（主题、全局状态、组件 actions）。
///
/// 在 `Application::run` / `TestAppContext` 入口尽早调用；可重复调用（幂等）。
pub fn init_components(cx: &mut App) {
    if cx.has_global::<GpuiComponentInitialized>() {
        return;
    }
    gpui_component::init(cx);
    cx.set_global(GpuiComponentInitialized);
}

/// 让 gpui-component 使用 Norma 当前主题，而不是 macOS 的系统明暗外观。
///
/// gpui-component 的 Input 会直接读取全局 `foreground` / `muted_foreground`，
/// 不会继承调用方容器的文字颜色；因此自定义浅色主题必须在构造窗口内容前同步。
pub fn sync_component_theme(id: ThemeId, cx: &mut App) {
    init_components(cx);

    let theme = Theme::for_id(id);
    let mode = if id.is_dark() {
        gpui_component::ThemeMode::Dark
    } else {
        gpui_component::ThemeMode::Light
    };
    // 先把其余组件 token 切到同一明暗基线，避免 danger / success 等未覆盖色仍残留系统主题。
    gpui_component::Theme::change(mode, None, cx);
    let component_theme = gpui_component::Theme::global_mut(cx);

    let colors = &mut component_theme.colors;
    colors.background = rgba(theme.canvas_from).into();
    colors.foreground = rgba(theme.text_primary).into();
    colors.muted = rgba(with_alpha(theme.text_primary, 0x0d)).into();
    colors.muted_foreground = rgba(theme.text_secondary).into();
    colors.border = rgba(theme.node_stroke).into();
    colors.input = rgba(theme.node_stroke).into();
    colors.caret = rgba(theme.accent).into();
    // 0x52 / 0xff ≈ 32%：保持 macOS 选择高亮可辨，同时不遮住选中文本。
    colors.selection = rgba(with_alpha(theme.accent, 0x52)).into();
    colors.ring = rgba(theme.focused_stroke).into();
    colors.link = rgba(theme.accent).into();
    colors.link_hover = rgba(with_alpha(theme.accent, 0xe8)).into();
    colors.link_active = rgba(with_alpha(theme.accent, 0xc8)).into();
    colors.primary = rgba(theme.accent).into();
    colors.primary_hover = rgba(with_alpha(theme.accent, 0xe8)).into();
    colors.primary_active = rgba(with_alpha(theme.accent, 0xc8)).into();
    colors.primary_foreground = rgba(0xffffffff).into();
    colors.secondary = rgba(with_alpha(theme.text_primary, 0x14)).into();
    colors.secondary_hover = rgba(with_alpha(theme.text_primary, 0x20)).into();
    colors.secondary_active = rgba(with_alpha(theme.text_primary, 0x2c)).into();
    colors.secondary_foreground = rgba(theme.text_primary).into();
    colors.popover = rgba(theme.panel_high).into();
    colors.popover_foreground = rgba(theme.text_primary).into();
}

const fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | alpha as u32
}

#[cfg(test)]
#[path = "../tests/unit/gpui_component_tests.rs"]
mod gpui_component_tests;
