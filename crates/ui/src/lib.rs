//! ui — 壳层（依赖 gpui）：可复用组件与主题。
//!
//! GUI 控件优先使用 [`component`]（`gpui-component`），非必要不要用 `div()` 重复造轮子。
pub mod icons;
pub mod theme;

pub use theme::Theme;

/// Longbridge [`gpui-component`](https://longbridge.github.io/gpui-component/) 组件库。
///
/// 常用入口：`component::button::Button`、`component::input`、`component::dialog` 等。
pub use gpui_component as component;

use gpui::{App, Global};

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

#[cfg(test)]
#[path = "../tests/unit/gpui_component_tests.rs"]
mod gpui_component_tests;
