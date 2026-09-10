//! 终端内容区与初始化覆盖层的组合渲染。

use super::{
    process_exit_label, terminal_content_mode, terminal_render_metrics, terminal_surface_bg,
    TerminalNodeRenderOptions, TERM_FONT_PX, TERM_LINE_PX, TERM_PADDING_PX,
};
use crate::cards::intro::render_intro_overlay;
use crate::cards::term::cursor::{cursor_animation_running, TerminalCursorAnimation};
use crate::cards::term::paint::{render_terminal_canvas, TerminalCanvasOptions};
use gpui::{div, prelude::*, px, rgba, AnyElement, IntoElement};
use runtime::NodeView;
use terminal::TerminalStatus;

/// 渲染一个终端节点内容区：退出态保留最后一帧输出，并叠加退出状态。
pub(crate) fn render_terminal_node(
    view: Option<NodeView>,
    options: TerminalNodeRenderOptions,
) -> AnyElement {
    let TerminalNodeRenderOptions {
        node_id,
        terminal_render_cache,
        theme,
        zoom,
        bottom_radius_px,
        selection,
        focused,
        reduce_motion,
        initialization,
        cursor_phase,
        cursor_task_running,
    } = options;
    let content_mode = terminal_content_mode(initialization);
    let content_opacity = content_mode.content_opacity();
    let terminal = if content_mode.requires_live_view() {
        Some(match view {
            Some(NodeView::Term {
                snapshot,
                revision,
                status,
            }) => {
                let cursor_animation = TerminalCursorAnimation {
                    phase: cursor_phase,
                    focused,
                    reduce_motion,
                    running: cursor_animation_running(
                        matches!(status, TerminalStatus::Running),
                        cursor_task_running,
                    ),
                    display_offset: snapshot.display_offset,
                };
                let terminal_canvas = render_terminal_canvas(TerminalCanvasOptions {
                    node_id,
                    snapshot,
                    revision,
                    cache: terminal_render_cache,
                    theme,
                    metrics: terminal_render_metrics(zoom),
                    selection,
                    cursor_animation,
                });
                let mut terminal = div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_full()
                    .relative()
                    .overflow_hidden()
                    .rounded_b(px(bottom_radius_px))
                    .opacity(content_opacity)
                    .bg(rgba(terminal_surface_bg(theme)))
                    .text_color(rgba(theme.term_fg))
                    .font_family("Menlo")
                    .text_size(px(TERM_FONT_PX * zoom))
                    .line_height(px(TERM_LINE_PX * zoom))
                    .p(px(TERM_PADDING_PX))
                    .child(terminal_canvas);

                if let TerminalStatus::Exited { code } = status {
                    terminal = terminal.child(
                        div()
                            .absolute()
                            .left(px(TERM_PADDING_PX))
                            .right(px(TERM_PADDING_PX))
                            .bottom(px(TERM_PADDING_PX))
                            .bg(rgba(terminal_surface_bg(theme)))
                            .text_color(rgba(theme.term_exit_fg))
                            .child(process_exit_label(code)),
                    );
                }
                terminal.into_any_element()
            }
            _ => div()
                .id(("terminal-session-missing", node_id))
                .debug_selector(|| "terminal-session-missing".into())
                .flex_1()
                .w_full()
                .rounded_b(px(bottom_radius_px))
                .opacity(content_opacity)
                .overflow_hidden()
                .child("(session gone)")
                .into_any_element(),
        })
    } else {
        None
    };

    let mut body = div()
        .flex()
        .flex_col()
        .flex_1()
        .w_full()
        .relative()
        .overflow_hidden()
        .rounded_b(px(bottom_radius_px));
    if let Some(terminal) = terminal {
        body = body.child(terminal);
    }
    if let Some(presentation) = initialization {
        body = body.child(render_intro_overlay(presentation, theme, bottom_radius_px));
    }
    body.into_any_element()
}
