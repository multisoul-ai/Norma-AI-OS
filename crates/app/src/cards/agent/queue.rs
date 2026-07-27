//! Agent card task queue badge and anchored sidecar.

use crate::app::MultisoulApp;
use agent::{AgentQueueTask, AgentQueueTaskSource};
use canvas::{CanvasNodeKind, NodeId};
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, FontWeight, MouseButton, Window};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

mod decor;
mod form;

use decor::anchor_pointer;
use form::{render_add_task_form, render_footer};

const PANEL_WIDTH: f32 = 414.0;
const PANEL_GAP: f32 = 22.0;
const PANEL_MARGIN: f32 = 16.0;
const PANEL_MAX_HEIGHT: f32 = 520.0;

pub(crate) fn waiting_count(app: &MultisoulApp, node_id: NodeId) -> usize {
    agent_definition_id(app, node_id)
        .and_then(|definition_id| {
            app.agent_task_coordinator
                .as_ref()
                .map(|coordinator| coordinator.queued_task_count(definition_id))
        })
        .unwrap_or(0)
}

pub(crate) fn render_badge(
    node_id: NodeId,
    count: usize,
    zoom: f32,
    theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selector = format!("agent-queue-badge-count-{count}");
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(with_alpha(theme.panel_high, 0x68)).into())
        .foreground(rgba(theme.text_secondary).into())
        .border(rgba(with_alpha(theme.edge_highlight, 0x48)).into())
        .hover(rgba(with_alpha(theme.accent_blue, 0x18)).into())
        .active(rgba(with_alpha(theme.accent_blue, 0x28)).into());
    div()
        .flex_none()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            Button::new(("agent-queue-badge", node_id))
                .debug_selector(move || selector.clone())
                .label(format!("QUEUE {count}"))
                .tooltip(format!("{count} 个等待任务"))
                .compact()
                .custom(variant)
                .h(px((20.0 * zoom).clamp(18.0, 24.0)))
                .px(px((7.0 * zoom).clamp(6.0, 9.0)))
                .text_size(px((9.0 * zoom).clamp(8.0, 10.0)))
                .on_click(cx.listener(move |app, _, _window, cx| {
                    app.agent_queue_ui.toggle(node_id);
                    cx.notify();
                })),
        )
}

pub(crate) fn render_panel(
    app: &mut MultisoulApp,
    node_id: NodeId,
    card_left: f32,
    card_top: f32,
    card_width: f32,
    window_width: f32,
    window_height: f32,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Option<AnyElement> {
    let definition_id = agent_definition_id(app, node_id)?;
    let snapshot = app
        .agent_task_coordinator
        .as_ref()?
        .queue_snapshot(definition_id)?;
    let agent_name = app
        .agent_definitions
        .definitions()
        .iter()
        .find(|definition| definition.id == definition_id)
        .map(|definition| definition.name.clone())
        .unwrap_or_else(|| "Agent".to_string());
    let waiting_count = snapshot.waiting.len();
    let running_count = usize::from(snapshot.current.is_some());
    let selected_task_ref = app.agent_queue_ui.selected_task_ref().map(str::to_string);
    let adding_task = app.agent_queue_ui.adding_task();
    let right_left = card_left + card_width + PANEL_GAP;
    let panel_on_right = right_left + PANEL_WIDTH <= window_width - PANEL_MARGIN;
    let left = if panel_on_right {
        right_left
    } else {
        (card_left - PANEL_GAP - PANEL_WIDTH).max(PANEL_MARGIN)
    };
    let top = card_top.clamp(
        PANEL_MARGIN,
        (window_height - PANEL_MAX_HEIGHT - PANEL_MARGIN).max(PANEL_MARGIN),
    );
    let theme = app.theme;
    let current = snapshot.current.as_ref().map(|task| {
        render_current(
            app,
            node_id,
            task,
            selected_task_ref.as_deref() == Some(task.task_ref.as_str()),
            theme,
            cx,
        )
    });
    let waiting = snapshot
        .waiting
        .iter()
        .enumerate()
        .map(|(index, task)| {
            render_waiting_row(
                app,
                node_id,
                index,
                task,
                selected_task_ref.as_deref() == Some(task.task_ref.as_str()),
                theme,
                cx,
            )
        })
        .collect::<Vec<_>>();

    Some(
        div()
            .id(("agent-queue-panel", node_id))
            .debug_selector(|| "agent-queue-panel".to_string())
            .absolute()
            .left(px(left))
            .top(px(top))
            .w(px(PANEL_WIDTH))
            .max_h(px(PANEL_MAX_HEIGHT))
            .flex()
            .flex_col()
            .rounded(px(16.0))
            .border_1()
            .border_color(rgba(with_alpha(theme.edge_highlight, 0x78)))
            .bg(rgba(with_alpha(theme.panel, 0xf4)))
            .shadow_lg()
            .text_color(rgba(theme.text_primary))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(anchor_pointer(panel_on_right, theme))
            .child(render_header(
                node_id,
                agent_name,
                running_count,
                waiting_count,
                theme,
                cx,
            ))
            .child(
                div()
                    .id(("agent-queue-scroll", node_id))
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .p(px(16.0))
                    .gap(px(14.0))
                    .overflow_y_scroll()
                    .when_some(current, |body, current| body.child(current))
                    .when(!waiting.is_empty(), |body| {
                        body.child(section_label("接下来", theme)).child(
                            div()
                                .flex()
                                .flex_col()
                                .rounded(px(10.0))
                                .border_1()
                                .border_color(rgba(with_alpha(theme.edge_highlight, 0x54)))
                                .overflow_hidden()
                                .children(waiting),
                        )
                    })
                    .when(
                        snapshot.current.is_none() && snapshot.waiting.is_empty(),
                        |body| {
                            body.child(
                                div()
                                    .py(px(22.0))
                                    .text_center()
                                    .text_size(px(12.0))
                                    .text_color(rgba(theme.text_tertiary))
                                    .child("暂无任务"),
                            )
                        },
                    ),
            )
            .child(if adding_task {
                render_add_task_form(node_id, theme, window, cx)
            } else {
                render_footer(node_id, theme, cx)
            })
            .into_any_element(),
    )
}

fn render_header(
    node_id: NodeId,
    agent_name: String,
    running_count: usize,
    waiting_count: usize,
    theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .h(px(80.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .px(px(18.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x48)))
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(15.0))
                        .font_weight(FontWeight::BOLD)
                        .child(format!("{agent_name} 的任务队列")),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(format!("{running_count} 个进行中 · {waiting_count} 个等待")),
                ),
        )
        .child(
            Button::new(("agent-queue-close", node_id))
                .debug_selector(|| "agent-queue-close".to_string())
                .label("×")
                .tooltip("关闭任务队列")
                .ghost()
                .compact()
                .on_click(cx.listener(|app, _, _window, cx| {
                    app.agent_queue_ui.close();
                    cx.notify();
                })),
        )
}

fn render_current(
    app: &MultisoulApp,
    node_id: NodeId,
    task: &AgentQueueTask,
    expanded: bool,
    theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let task_ref = task.task_ref.clone();
    div()
        .flex()
        .flex_col()
        .gap(px(7.0))
        .child(section_label("正在执行", theme))
        .child(
            div()
                .id(("agent-queue-current", node_id))
                .debug_selector(|| "agent-queue-current".to_string())
                .min_h(px(48.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(10.0))
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.accent_blue, 0x54)))
                .bg(rgba(with_alpha(theme.accent_blue, 0x12)))
                .cursor_pointer()
                .on_click(cx.listener(move |app, _, _window, cx| {
                    app.agent_queue_ui.select_task(node_id, task_ref.clone());
                    cx.notify();
                }))
                .child(
                    div()
                        .w(px(28.0))
                        .h(px(28.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(14.0))
                        .bg(rgba(with_alpha(theme.accent_blue, 0x24)))
                        .text_color(rgba(theme.accent_blue))
                        .child("▶"),
                )
                .child(task_summary(&task.task, theme))
                .child(source_pill(app, task.task_id, None, task.source, theme)),
        )
        .when(expanded, |row| row.child(task_detail(task, theme)))
}

fn render_waiting_row(
    app: &MultisoulApp,
    node_id: NodeId,
    index: usize,
    task: &AgentQueueTask,
    expanded: bool,
    theme: Theme,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let task_ref = task.task_ref.clone();
    let cancel_task_ref = task.task_ref.clone();
    let row_selector = format!("agent-queue-row-{index}");
    let detail_selector = format!("agent-queue-detail-{index}");
    div()
        .flex()
        .flex_col()
        .when(index > 0, |row| {
            row.border_t_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x38)))
        })
        .child(
            div()
                .id(("agent-queue-row", task.task_id))
                .debug_selector(move || row_selector.clone())
                .min_h(px(48.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .px(px(10.0))
                .cursor_pointer()
                .hover(move |style| style.bg(rgba(with_alpha(theme.accent_blue, 0x0e))))
                .on_click(cx.listener(move |app, _, _window, cx| {
                    app.agent_queue_ui.select_task(node_id, task_ref.clone());
                    cx.notify();
                }))
                .child(
                    div()
                        .w(px(15.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(13.0))
                        .text_color(rgba(with_alpha(theme.text_tertiary, 0x88)))
                        .child("⠿"),
                )
                .child(
                    div()
                        .w(px(16.0))
                        .flex_none()
                        .text_center()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_secondary))
                        .child((index + 1).to_string()),
                )
                .child(task_summary(&task.task, theme))
                .child(source_pill(
                    app,
                    task.task_id,
                    Some(index),
                    task.source,
                    theme,
                ))
                .child(
                    div()
                        .flex_none()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            Button::new(("agent-queue-cancel", task.task_id))
                                .debug_selector(move || format!("agent-queue-cancel-{index}"))
                                .label("×")
                                .tooltip("撤回这条等待任务")
                                .ghost()
                                .compact()
                                .text_color(rgba(theme.destructive))
                                .on_click(cx.listener(move |app, _, window, cx| {
                                    app.withdraw_agent_queue_task(
                                        node_id,
                                        cancel_task_ref.clone(),
                                        window,
                                        cx,
                                    );
                                })),
                        ),
                ),
        )
        .when(expanded, |row| {
            row.child(
                div()
                    .id(("agent-queue-waiting-detail", task.task_id))
                    .debug_selector(move || detail_selector.clone())
                    .child(task_detail(task, theme)),
            )
        })
}

fn task_summary(task: &str, theme: Theme) -> Div {
    div()
        .flex_1()
        .min_w(px(0.0))
        .overflow_hidden()
        .truncate()
        .text_size(px(12.0))
        .text_color(rgba(theme.text_primary))
        .child(task.to_string())
}

fn task_detail(task: &AgentQueueTask, theme: Theme) -> Div {
    div()
        .mx(px(10.0))
        .mb(px(10.0))
        .p(px(10.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .rounded(px(8.0))
        .bg(rgba(with_alpha(theme.panel_high, 0x78)))
        .text_size(px(11.0))
        .text_color(rgba(theme.text_secondary))
        .child(task.task.clone())
        .child(
            div()
                .text_size(px(9.0))
                .text_color(rgba(theme.text_tertiary))
                .child(format!("任务引用 · {}", task.task_ref)),
        )
}

fn source_pill(
    app: &MultisoulApp,
    task_id: u64,
    waiting_index: Option<usize>,
    source: AgentQueueTaskSource,
    theme: Theme,
) -> impl IntoElement {
    let (label, slug) = match source {
        AgentQueueTaskSource::Norma => ("Norma".to_string(), "norma".to_string()),
        AgentQueueTaskSource::Manual => ("手动".to_string(), "manual".to_string()),
        AgentQueueTaskSource::Agent { definition_id } => {
            let name = app
                .agent_definitions
                .definitions()
                .iter()
                .find(|definition| definition.id == definition_id)
                .map(|definition| definition.name.clone())
                .unwrap_or_else(|| "Agent".to_string());
            (name, "agent".to_string())
        }
    };
    let selector = waiting_index.map(|index| format!("agent-queue-source-{index}-{slug}"));
    div()
        .id(("agent-queue-source", task_id))
        .when_some(selector, |pill, selector| {
            pill.debug_selector(move || selector.clone())
        })
        .flex_none()
        .px(px(7.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .rounded(px(11.0))
        .bg(rgba(with_alpha(theme.panel_high, 0x88)))
        .text_size(px(9.0))
        .text_color(rgba(theme.text_tertiary))
        .child(label)
}

fn section_label(label: &'static str, theme: Theme) -> Div {
    div()
        .text_size(px(11.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(theme.text_secondary))
        .child(label)
}

fn agent_definition_id(app: &MultisoulApp, node_id: NodeId) -> Option<i64> {
    let node = app.runtime.document.node(node_id)?;
    let CanvasNodeKind::Agent(spec) = &node.kind else {
        return None;
    };
    spec.definition_id
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
