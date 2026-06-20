//! 画布渲染（占位；Task 14 实现 pan/zoom/拖拽）。
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, Context, Window};

#[derive(Default)]
pub struct InteractionState;

pub fn render_canvas(
    _app: &mut MultisoulApp,
    _window: &mut Window,
    _cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    div().size_full().child("canvas placeholder — Cmd+N to spawn terminal")
}
