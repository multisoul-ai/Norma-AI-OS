use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, AnyView, Context, Div, Focusable, MouseButton, Stateful,
    StyleRefinement, Subscription, Window,
};
use ui::component::button::{Button, ButtonVariants};
use ui::component::input::{Input, InputEvent, InputState};
use ui::component::{Disableable, StyledExt};

struct SearchInput {
    input: gpui::Entity<InputState>,
    _subscription: Subscription,
}

impl MultisoulApp {
    pub(crate) fn render_desktop_launchpad(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        #[cfg(target_os = "macos")]
        self.sync_video_wallpaper(None, cx);
        let theme = self.theme;
        let win = window.viewport_size();
        let width = f32::from(win.width);
        let columns = if width >= 1400.0 {
            4
        } else if width >= 940.0 {
            3
        } else if width >= 610.0 {
            2
        } else {
            1
        };
        let app = cx.weak_entity();
        let search = window.use_keyed_state("desktop-workspace-search", cx, move |window, cx| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search workspaces"));
            let subscription = cx.subscribe(&input, move |_, input, event: &InputEvent, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let query = input.read(cx).value().to_string();
                if let Some(app) = app.upgrade() {
                    app.update(cx, |app, cx| {
                        app.desktop_workspace.search_query = query;
                        cx.notify();
                    });
                }
            });
            SearchInput {
                input,
                _subscription: subscription,
            }
        });
        let input = search.read(cx).input.clone();
        let focus = input.focus_handle(cx);
        let workspaces: Vec<_> = self
            .desktop_workspace
            .filtered()
            .into_iter()
            .cloned()
            .collect();
        let is_empty = workspaces.is_empty();
        let has_any = !self.desktop_workspace.workspaces.is_empty();

        div()
            .id("desktop-workspace-launchpad")
            .debug_selector(|| "desktop-workspace-launchpad".to_string())
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .bg(rgba(theme.canvas_from))
            .child(
                div()
                    .flex()
                    .items_end()
                    .justify_between()
                    .gap(px(20.0))
                    .px(px(56.0))
                    .pt(px(66.0))
                    .pb(px(24.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(7.0))
                            .child(
                                div()
                                    .text_size(px(31.0))
                                    .font_semibold()
                                    .text_color(rgba(theme.text_primary))
                                    .child("Workspaces"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(rgba(theme.text_secondary))
                                    .child(
                                        "One canvas for the projects you are working on together.",
                                    ),
                            ),
                    )
                    .child(
                        Button::new("desktop-workspace-create")
                            .debug_selector(|| "desktop-workspace-create".to_string())
                            .label("New Workspace")
                            .primary()
                            .disabled(self.desktop_workspace.busy)
                            .on_click(
                                cx.listener(|app, _, _, cx| app.create_desktop_workspace(cx)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(56.0))
                    .pb(px(20.0))
                    .child(
                        div()
                            .id("desktop-workspace-search")
                            .debug_selector(|| "desktop-workspace-search".to_string())
                            .w(px(280.0))
                            .h(px(38.0))
                            .flex()
                            .items_center()
                            .px(px(10.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(rgba(theme.node_stroke))
                            .bg(rgba(theme.panel_high))
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                focus.focus(window);
                                cx.stop_propagation();
                            })
                            .child(Input::new(&input).appearance(false).cleanable(true)),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(theme.text_tertiary))
                            .child(format!(
                                "{} workspaces",
                                self.desktop_workspace.workspaces.len()
                            )),
                    ),
            )
            .child(
                div()
                    .id("desktop-workspace-scroll")
                    .debug_selector(|| "desktop-workspace-scroll".to_string())
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_scroll()
                    .px(px(56.0))
                    .pb(px(40.0))
                    .when(is_empty, |this| {
                        this.child(
                            div()
                                .py(px(92.0))
                                .flex()
                                .justify_center()
                                .text_color(rgba(theme.text_secondary))
                                .child(if has_any {
                                    "No workspaces match your search."
                                } else {
                                    "Create a workspace to start with an empty canvas."
                                }),
                        )
                    })
                    .when(!is_empty, |this| {
                        this.child(
                            div().grid().grid_cols(columns).gap(px(16.0)).children(
                                workspaces
                                    .into_iter()
                                    .enumerate()
                                    .map(|(index, workspace)| {
                                        self.desktop_workspace_card(index, workspace, cx)
                                    }),
                            ),
                        )
                    }),
            )
            .when_some(self.desktop_workspace.error.clone(), |this, error| {
                this.child(
                    div()
                        .px(px(56.0))
                        .pb(px(16.0))
                        .text_color(rgba(theme.destructive))
                        .child(error),
                )
            })
    }

    pub(crate) fn render_desktop_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        if let (Some(workspace_id), Some(area_id)) = (
            self.desktop_workspace.active_id.as_deref(),
            self.desktop_workspace.expanded_area_id.as_deref(),
        ) {
            if let Some(session) = self
                .desktop_workspace
                .sessions
                .get(&(workspace_id.to_owned(), area_id.to_owned()))
                .cloned()
            {
                return div()
                    .id("desktop-project-expanded")
                    .debug_selector(|| "desktop-project-expanded".to_string())
                    .absolute()
                    .inset_0()
                    .child(
                        AnyView::from(session)
                            .cached(StyleRefinement::default().absolute().size_full()),
                    )
                    .child(
                        div().absolute().top(px(18.0)).left(px(82.0)).child(
                            Button::new("desktop-project-back")
                                .debug_selector(|| "desktop-project-back".to_string())
                                .label("Workspace Overview")
                                .on_click(cx.listener(|app, _, window, cx| {
                                    app.collapse_desktop_project_area(window, cx)
                                })),
                        ),
                    );
            }
        }
        let name = self
            .desktop_workspace
            .active()
            .map(|item| item.name.clone())
            .unwrap_or_else(|| "Workspace".to_string());
        let areas = self.desktop_workspace.areas.clone();
        let workspace_id = self.desktop_workspace.active_id.clone().unwrap_or_default();
        let is_empty = areas.is_empty();
        let width = f32::from(window.viewport_size().width);
        let columns = if width >= 1200.0 {
            3
        } else if width >= 730.0 {
            2
        } else {
            1
        };
        let command_key =
            gpui::ElementId::Name(format!("desktop-norma-input-{workspace_id}").into());
        let command_input = window.use_keyed_state(command_key, cx, |window, cx| {
            InputState::new(window, cx).placeholder("Tell Norma which project and Agent to work on")
        });
        let command_focus = command_input.focus_handle(cx);
        let receipts = self
            .desktop_workspace
            .dispatch_receipts
            .get(&workspace_id)
            .cloned()
            .unwrap_or_default();
        div()
            .id("desktop-workspace-overview")
            .debug_selector(|| "desktop-workspace-overview".to_string())
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .bg(rgba(theme.canvas_from))
            .child(
                div()
                    .h(px(74.0))
                    .px(px(40.0))
                    .flex()
                    .items_center()
                    .gap(px(18.0))
                    .child(Button::new("desktop-workspace-back").debug_selector(|| "desktop-workspace-back".to_string()).label("All Workspaces").on_click(cx.listener(|app, _, _, cx| app.show_desktop_launchpad(cx))))
                    .child(div().flex_1().text_size(px(21.0)).font_semibold().text_color(rgba(theme.text_primary)).child(name))
                    .child(Button::new("desktop-workspace-add-project").debug_selector(|| "desktop-workspace-add-project".to_string()).label("Add Project").primary().disabled(self.desktop_workspace.busy).on_click(cx.listener(|app, _, window, cx| app.prompt_open_project(window, cx)))),
            )
            .child(div().px(px(40.0)).pb(px(12.0)).flex().items_center().gap(px(10.0))
                .child(div().text_size(px(12.0)).font_semibold().text_color(rgba(theme.text_secondary)).child("Norma"))
                .child(div().id("desktop-norma-input").debug_selector(|| "desktop-norma-input".to_string())
                    .flex_1().min_w(px(0.0)).h(px(36.0)).px(px(8.0)).flex().items_center()
                    .rounded(px(8.0)).border_1().border_color(rgba(theme.node_stroke))
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        command_focus.focus(window);
                        cx.stop_propagation();
                    })
                    .child(Input::new(&command_input).appearance(false)))
                .child(Button::new("desktop-norma-send").debug_selector(|| "desktop-norma-send".to_string())
                    .label("Dispatch").on_click(cx.listener(move |app, _, window, cx| {
                        app.send_desktop_workspace_command(command_input.clone(), window, cx);
                    }))))
            .when(!receipts.is_empty(), |this| this.child(div().id("desktop-norma-receipts")
                .px(px(40.0)).pb(px(12.0)).flex().gap(px(8.0)).overflow_scroll()
                .children(receipts.into_iter().enumerate().map(|(index, receipt)| {
                    div().id(("desktop-norma-receipt", index))
                        .debug_selector(move || format!("desktop-norma-receipt-{index}"))
                        .px(px(9.0)).py(px(5.0)).rounded(px(7.0))
                        .bg(rgba(theme.panel_high))
                        .text_size(px(11.0))
                        .text_color(rgba(if receipt.submitted { theme.text_secondary } else { theme.destructive }))
                        .child(format!("{} / {} · {}", receipt.project_name, receipt.agent_name, receipt.status))
                }))))
            .child(
                div()
                    .id("desktop-project-area-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_scroll()
                    .p(px(40.0))
                    .when(is_empty, |this| this.child(
                        div().h(px(500.0)).flex().flex_col().items_center().justify_center().gap(px(15.0))
                            .child(div().text_size(px(24.0)).font_semibold().text_color(rgba(theme.text_primary)).child("A canvas for your projects"))
                            .child(div().text_size(px(13.0)).text_color(rgba(theme.text_secondary)).child("Add a project area to start working with its agents.")),
                    ))
                    .when(!is_empty, |this| this.child(
                        div().grid().grid_cols(columns).gap(px(16.0)).children(areas.into_iter().enumerate().map(|(index, area)| {
                            let area_id = area.id.clone();
                            let open_area_id = area.id.clone();
                            let remove_id = area.id.clone();
                            let unavailable = self.desktop_workspace.area_errors.get(&area.id).cloned();
                            let agents = self.desktop_agent_summaries(&workspace_id, &area.id, cx);
                            let agent_rows = agents.into_iter().enumerate().map(|(agent_index, agent)| {
                                let input_key = gpui::ElementId::Name(format!("desktop-agent-input-{}-{}-{}", workspace_id, area.id, agent.node_id).into());
                                let input = window.use_keyed_state(input_key, cx, |window, cx| {
                                    InputState::new(window, cx).placeholder("Message Agent")
                                });
                                let focus = input.focus_handle(cx);
                                let target_workspace = workspace_id.clone();
                                let target_area = area.id.clone();
                                let node_id = agent.node_id;
                                div().id(gpui::ElementId::Name(format!("desktop-agent-row-{index}-{agent_index}").into()))
                                    .debug_selector(move || format!("desktop-agent-row-{index}-{agent_index}"))
                                    .flex().flex_col().gap(px(5.0)).p(px(8.0)).rounded(px(9.0))
                                    .bg(rgba(theme.canvas_to))
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                    .child(div().flex().justify_between()
                                        .child(div().font_semibold().text_color(rgba(theme.text_primary)).child(agent.name))
                                        .child(div().text_size(px(11.0)).text_color(rgba(theme.text_secondary)).child(agent.status)))
                                    .child(div().text_size(px(10.0)).text_color(rgba(theme.text_secondary)).child(agent.location))
                                    .child(div().text_size(px(11.0)).text_color(rgba(theme.text_tertiary))
                                        .child(if agent.last_line.is_empty() { "Waiting for output".to_string() } else { agent.last_line }))
                                    .child(div().flex().gap(px(6.0)).items_center()
                                        .child(div().id(gpui::ElementId::Name(format!("desktop-agent-input-{index}-{agent_index}").into()))
                                            .debug_selector(move || format!("desktop-agent-input-{index}-{agent_index}"))
                                            .flex_1().min_w(px(0.0)).h(px(29.0)).px(px(6.0))
                                            .flex().items_center().rounded(px(6.0)).border_1().border_color(rgba(theme.node_stroke))
                                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                                focus.focus(window);
                                                cx.stop_propagation();
                                            })
                                            .child(Input::new(&input).appearance(false)))
                                        .child(Button::new(gpui::ElementId::Name(format!("desktop-agent-send-{index}-{agent_index}").into()))
                                            .debug_selector(move || format!("desktop-agent-send-{index}-{agent_index}"))
                                            .label("Send").compact()
                                            .on_click(cx.listener(move |app, _, window, cx| {
                                                cx.stop_propagation();
                                                app.send_desktop_agent_input(target_workspace.clone(), target_area.clone(), node_id, input.clone(), window, cx);
                                            }))))
                            }).collect::<Vec<_>>();
                            div()
                                .id(("desktop-project-area", index))
                                .debug_selector(move || format!("desktop-project-area-{index}"))
                                .h(px(320.0))
                                .flex()
                                .flex_col()
                                .justify_between()
                                .p(px(18.0))
                                .rounded(px(16.0))
                                .border_1()
                                .border_color(rgba(theme.node_stroke))
                                .bg(rgba(theme.panel_high))
                                .cursor_pointer()
                                .hover(move |style| style.border_color(rgba(theme.accent_blue)))
                                .active(move |style| style.bg(rgba(theme.canvas_to)))
                                .on_click(cx.listener(move |app, _, window, cx| app.expand_desktop_project_area(area_id.clone(), window, cx)))
                                .child(div().flex().items_center().justify_between()
                                    .child(div().text_size(px(17.0)).font_semibold().text_color(rgba(theme.text_primary)).child(area.display_name))
                                    .child(div().flex().gap(px(5.0))
                                    .child(Button::new(("desktop-project-open", index))
                                        .debug_selector(move || format!("desktop-project-open-{index}"))
                                        .label("Open").compact()
                                        .on_click(cx.listener(move |app, _, window, cx| {
                                            cx.stop_propagation();
                                            app.expand_desktop_project_area(open_area_id.clone(), window, cx);
                                        })))
                                    .child(Button::new(("desktop-project-remove", index))
                                        .debug_selector(move || format!("desktop-project-remove-{index}"))
                                        .label("Remove")
                                        .compact()
                                        .on_click(cx.listener(move |app, _, _, cx| {
                                            cx.stop_propagation();
                                            app.begin_desktop_area_remove(remove_id.clone(), cx);
                                        })))))
                                .child(div().text_size(px(11.0)).text_color(rgba(theme.text_secondary)).child(area.root.display().to_string()))
                                .child(div().id(("desktop-agent-list", index)).flex_1().min_h(px(0.0)).overflow_scroll()
                                    .flex().flex_col().gap(px(7.0))
                                    .when(agent_rows.is_empty(), |this| this.child(
                                        div().py(px(34.0)).flex().justify_center().text_size(px(12.0))
                                            .text_color(rgba(theme.text_tertiary)).child("No Agent cards yet. Open this project to add one."),
                                    ))
                                    .children(agent_rows))
                                .when_some(unavailable, |this, error| this.child(
                                    div().id(("desktop-project-unavailable", index))
                                        .debug_selector(move || format!("desktop-project-unavailable-{index}"))
                                        .text_size(px(12.0)).text_color(rgba(theme.destructive)).child(error),
                                ))
                        })),
                    )),
            )
            .when_some(self.desktop_workspace.error.clone(), |this, error| {
                this.child(div().px(px(40.0)).pb(px(12.0)).text_color(rgba(theme.destructive)).child(error))
            })
            .child(
                div().absolute().bottom(px(22.0)).left(px(0.0)).right(px(0.0)).flex().justify_center()
                    .child(Button::new("desktop-workspace-switch-button")
                        .debug_selector(|| "desktop-workspace-switch-button".to_string())
                        .label("Switch Workspace  ⌘⇧K")
                        .on_click(cx.listener(|app, _, window, cx| app.open_desktop_switcher(window, cx)))),
            )
    }
}
