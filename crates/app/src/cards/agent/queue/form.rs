//! Agent queue footer and inline task entry.

use crate::app::MultisoulApp;
use canvas::NodeId;
use gpui::{div, prelude::*, px, rgba, Context, Div, Entity, Focusable, Window};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::component::input::{Input, InputState};
use ui::Theme;

use super::with_alpha;

pub(super) fn render_footer(node_id: NodeId, theme: Theme, cx: &mut Context<MultisoulApp>) -> Div {
    let variant = primary_variant(theme, cx);
    div()
        .h(px(58.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_end()
        .px(px(16.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x48)))
        .child(
            Button::new(("agent-queue-add", node_id))
                .debug_selector(|| "agent-queue-add".to_string())
                .label("添加任务")
                .custom(variant)
                .h(px(34.0))
                .on_click(cx.listener(move |app, _, _window, cx| {
                    app.agent_queue_ui.begin_add_task(node_id);
                    cx.notify();
                })),
        )
}

struct AddTaskInputState {
    input: Entity<InputState>,
}

pub(super) fn render_add_task_form(
    node_id: NodeId,
    theme: Theme,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let state = window.use_keyed_state(
        ("agent-queue-add-input-state", node_id),
        cx,
        |window, cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("描述要排队的任务")
                    .multi_line(false)
            });
            let focus = input.focus_handle(cx);
            window.defer(cx, move |window, _cx| focus.focus(window));
            AddTaskInputState { input }
        },
    );
    let input = state.read(cx).input.clone();
    let submit_input = input.clone();
    div()
        .min_h(px(76.0))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.0))
        .p(px(12.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x48)))
        .child(
            div()
                .id(("agent-queue-add-input", node_id))
                .debug_selector(|| "agent-queue-add-input".to_string())
                .h(px(36.0))
                .flex_1()
                .rounded(px(8.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x78)))
                .bg(rgba(with_alpha(theme.panel_high, 0x68)))
                .child(Input::new(&input).appearance(false).h(px(34.0))),
        )
        .child(
            Button::new(("agent-queue-add-cancel", node_id))
                .label("取消")
                .ghost()
                .compact()
                .on_click(cx.listener(|app, _, _window, cx| {
                    app.agent_queue_ui.finish_add_task();
                    cx.notify();
                })),
        )
        .child(
            Button::new(("agent-queue-add-submit", node_id))
                .debug_selector(|| "agent-queue-add-submit".to_string())
                .label("加入队列")
                .custom(primary_variant(theme, cx))
                .h(px(34.0))
                .on_click(cx.listener(move |app, _, window, cx| {
                    let task = submit_input.read(cx).value().to_string();
                    app.add_agent_queue_task(node_id, task, window, cx);
                })),
        )
}

fn primary_variant(theme: Theme, cx: &mut Context<MultisoulApp>) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .color(rgba(theme.accent_blue).into())
        .foreground(rgba(0xffffffff).into())
        .border(rgba(theme.accent_blue).into())
        .hover(rgba(with_alpha(theme.accent_blue, 0xe8)).into())
        .active(rgba(with_alpha(theme.accent_blue, 0xc8)).into())
}
