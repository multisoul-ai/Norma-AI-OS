//! 终端渲染（占位；Task 15 实现 grid 渲染 + 键盘）。
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, Context, Window};
use terminal::TerminalSessionId;

/// 占位渲染：Task 15 将替换此函数为完整的 grid 渲染。
pub fn render_terminal_node(
    _app: &mut MultisoulApp,
    _session_id: TerminalSessionId,
    _zoom: f32,
    _window: &mut Window,
    _cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    div().size_full()
}
