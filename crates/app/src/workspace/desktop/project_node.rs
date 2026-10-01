use super::{glass_button_variant, project_area::project_identity_label};
use crate::app::MultisoulApp;
use canvas::{CanvasNodeKind, CanvasNodeSnapshot};
use gpui::{div, prelude::*, px, rgba, Context, Div, Focusable, MouseButton, Stateful, Window};
use ui::component::button::{Button, ButtonVariants};
use ui::component::input::{Input, InputState};

impl MultisoulApp {
    pub(super) fn desktop_project_area_node(
        &self,
        area_index: usize,
        node_index: usize,
        node: CanvasNodeSnapshot,
        workspace_id: &str,
        area_id: &str,
        project_name: &str,
        width: f32,
        height: f32,
        selected: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.desktop_theme();
        let node_id = node.id;
        let is_agent = matches!(node.kind, CanvasNodeKind::Agent(_));
        let click_workspace = workspace_id.to_owned();
        let click_area = area_id.to_owned();
        let session = self
            .desktop_workspace
            .sessions
            .get(&(workspace_id.to_owned(), area_id.to_owned()))
            .map(|session| session.read(cx));
        let worktree = session
            .as_deref()
            .and_then(|session| session.runtime.agent_worktree(node_id))
            .and_then(|path| path.file_name().and_then(|name| name.to_str()));
        let project_label = project_identity_label(project_name, worktree);
        let mut card = div()
            .id(gpui::ElementId::Name(
                format!("desktop-agent-preview-{area_index}-{node_index}").into(),
            ))
            .debug_selector(move || format!("desktop-agent-preview-{area_index}-{node_index}"))
            .relative()
            .w(px(width))
            .h(px(height))
            .rounded(px(7.0))
            .cursor_pointer()
            .hover(move |style| style.border_1().border_color(rgba(theme.accent_cyan)))
            .active(move |style| style.bg(rgba(theme.node_fill)))
            .tab_index(0)
            .focus(move |style| style.border_1().border_color(rgba(theme.focused_stroke)))
            .on_click(cx.listener(move |app, _, window, cx| {
                cx.stop_propagation();
                if is_agent {
                    let target = (click_workspace.clone(), click_area.clone(), node_id);
                    app.desktop_workspace.enlarged_agent =
                        (app.desktop_workspace.enlarged_agent.as_ref() != Some(&target))
                            .then_some(target);
                    app.desktop_workspace.pending_agent_reveal = app
                        .desktop_workspace
                        .enlarged_agent
                        .as_ref()
                        .map(|_| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)));
                    cx.notify();
                } else {
                    app.expand_desktop_project_area(click_area.clone(), window, cx);
                }
            }))
            .child(self.render_desktop_preview_card(
                &node,
                session.as_deref(),
                width,
                if selected && is_agent {
                    height - 38.0
                } else {
                    height
                },
                selected,
                format!("desktop-agent-live-{area_index}-{node_index}"),
                selected.then_some(project_label),
            ));

        if selected && is_agent && session.is_some() {
            let input_key = gpui::ElementId::Name(
                format!("desktop-agent-input-{workspace_id}-{area_id}-{node_id}").into(),
            );
            let input = window.use_keyed_state(input_key, cx, |window, cx| {
                InputState::new(window, cx).placeholder("Message Agent")
            });
            let focus = input.focus_handle(cx);
            let send_workspace = workspace_id.to_owned();
            let send_area = area_id.to_owned();
            card = card.child(
                div()
                    .id(("desktop-agent-expanded", area_index))
                    .debug_selector(move || {
                        format!("desktop-agent-expanded-{area_index}-{node_index}")
                    })
                    .absolute()
                    .left(px(0.0))
                    .right(px(0.0))
                    .bottom(px(0.0))
                    .h(px(38.0))
                    .flex()
                    .gap(px(8.0))
                    .p(px(4.0))
                    .bg(rgba(theme.dock))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .id(("desktop-agent-input", area_index))
                            .debug_selector(move || {
                                format!("desktop-agent-input-{area_index}-{node_index}")
                            })
                            .flex_1()
                            .min_w(px(0.0))
                            .px(px(6.0))
                            .flex()
                            .items_center()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(rgba(theme.focused_stroke))
                            .bg(rgba(self.desktop_input_fill()))
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                focus.focus(window);
                                cx.stop_propagation();
                            })
                            .child(Input::new(&input).appearance(false)),
                    )
                    .child(
                        Button::new(("desktop-agent-send", area_index))
                            .debug_selector(move || {
                                format!("desktop-agent-send-{area_index}-{node_index}")
                            })
                            .label("Send")
                            .compact()
                            .custom(glass_button_variant(theme, cx))
                            .on_click(cx.listener(move |app, _, window, cx| {
                                cx.stop_propagation();
                                app.send_desktop_agent_input(
                                    send_workspace.clone(),
                                    send_area.clone(),
                                    node_id,
                                    input.clone(),
                                    window,
                                    cx,
                                );
                            })),
                    ),
            );
        }
        card
    }
}
