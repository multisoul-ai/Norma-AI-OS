use super::glass_button_variant;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, Context, Div, Focusable, MouseButton, Stateful, Subscription, Window,
};
use ui::component::button::{Button, ButtonVariants};
use ui::component::input::{Input, InputEvent, InputState};
use ui::component::Disableable;

struct SearchInput {
    input: gpui::Entity<InputState>,
    _subscription: Subscription,
}

#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_tests.rs"]
mod home_tests;

#[cfg(test)]
#[path = "../../../tests/unit/desktop_home_contrast_tests.rs"]
mod contrast_tests;

impl MultisoulApp {
    pub(crate) fn render_desktop_launchpad(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.sync_desktop_input_theme(self.desktop_workspace.home_directory, cx);
        #[cfg(target_os = "macos")]
        self.sync_canvas_video_wallpaper(cx);
        let theme = self.desktop_theme();
        let win = window.viewport_size();
        let width = f32::from(win.width);
        let scene = self.desktop_scene_frame(
            canvas::CanvasRect::new(0.0, 0.0, width, f32::from(win.height)),
            true,
        );
        let scale = scene.size.x / width;
        let capture_bounds = self.desktop_workspace.transition.is_none();
        let app = cx.weak_entity();
        let search = window.use_keyed_state("desktop-workspace-search", cx, move |window, cx| {
            let input = cx.new(|cx| {
                InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                    "workspace.search_workspaces",
                    window,
                    cx,
                ))
            });
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
        if input.read(cx).value().as_ref() != self.desktop_workspace.search_query.as_str() {
            let query = self.desktop_workspace.search_query.clone();
            input.update(cx, |input, cx| input.set_value(query, window, cx));
        }
        let focus = input.focus_handle(cx);
        if self.desktop_workspace.home_directory {
            return self.render_desktop_home(input, window, cx);
        }
        let workspaces: Vec<_> = self
            .desktop_workspace
            .filtered()
            .into_iter()
            .cloned()
            .collect();
        let layout =
            super::layout::workspace_layout(width, f32::from(win.height), workspaces.len());
        let content_height = layout.last().map_or(0.0, |rect| rect.max().y + 42.0);
        let workspace_ids: Vec<_> = workspaces.iter().map(|item| item.id.clone()).collect();
        let zoomed_workspace_id = self.desktop_directory_workspace_id();
        let workspace_rects = self.desktop_workspace.workspace_rects.clone();
        if zoomed_workspace_id.is_some() {
            return div()
                .id("desktop-workspace-launchpad")
                .debug_selector(|| "desktop-workspace-launchpad".to_string())
                .absolute()
                .left(px(scene.origin.x))
                .top(px(scene.origin.y))
                .w(px(scene.size.x))
                .h(px(scene.size.y))
                .children(
                    self.desktop_workspace
                        .workspaces
                        .iter()
                        .enumerate()
                        .filter_map(|(index, workspace)| {
                            let source = self
                                .desktop_workspace
                                .workspace_motion_rects
                                .get(&workspace.id)?;
                            let slot = div()
                                .absolute()
                                .left(px(source.origin.x * scale))
                                .top(px(source.origin.y * scale))
                                .w(px(source.size.x * scale));
                            let slot = slot.child(self.desktop_workspace_card(
                                index,
                                workspace.clone(),
                                source.size.x * scale,
                                win,
                                scale,
                                cx,
                            ));
                            Some(slot)
                        }),
                );
        }
        workspace_rects
            .lock()
            .expect("workspace bounds mutex poisoned")
            .retain(|id, _| workspace_ids.contains(id));
        let is_empty = workspaces.is_empty();
        let has_any = !self.desktop_workspace.workspaces.is_empty();

        div()
            .id("desktop-workspace-launchpad")
            .debug_selector(|| "desktop-workspace-launchpad".to_string())
            .absolute()
            .left(px(scene.origin.x))
            .top(px(scene.origin.y))
            .w(px(scene.size.x))
            .h(px(scene.size.y))
            .flex()
            .flex_col()
            .when(!self.desktop_workspace_zoom_active(), |this| {
                this.bg(rgba(theme.graphite))
                    .child(self.render_workspace_backdrop())
            })
            .child(
                div()
                    .id("desktop-workspace-scroll")
                    .debug_selector(|| "desktop-workspace-scroll".to_string())
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_scroll()
                    .track_scroll(&self.desktop_directory_scroll)
                    .when(is_empty, |this| {
                        this.px(px(56.0)).pt(px(118.0)).pb(px(42.0)).child(
                            div()
                                .pt(px(36.0))
                                .pb(px(92.0))
                                .flex()
                                .justify_center()
                                .text_color(rgba(theme.text_secondary))
                                .child(if has_any {
                                    ui::i18n::text("workspace.no_workspaces_match_your_search")
                                } else {
                                    ui::i18n::text("workspace.create_a_workspace_to_start_with_an_empty_canvas")
                                }),
                        )
                    })
                    .when(!is_empty, |this| {
                        this.child(
                            div()
                                .relative()
                                .w(px(width))
                                .h(px(content_height))
                                .children(workspaces.into_iter().zip(layout).enumerate().map(
                                    |(index, (workspace, rect))| {
                                        div()
                                            .absolute()
                                            .left(px(rect.origin.x))
                                            .top(px(rect.origin.y))
                                            .w(px(rect.size.x))
                                            .h(px(rect.size.y))
                                            .child(self.desktop_workspace_card(
                                                index,
                                                workspace,
                                                rect.size.x,
                                                win,
                                                scale,
                                                cx,
                                            ))
                                    },
                                ))
                                .on_children_prepainted(move |bounds, _, _| {
                                    if !capture_bounds {
                                        return;
                                    }
                                    let mut rects = workspace_rects
                                        .lock()
                                        .expect("workspace bounds mutex poisoned");
                                    for (id, bounds) in workspace_ids.iter().zip(bounds) {
                                        rects.insert(
                                            id.clone(),
                                            super::camera::canvas_rect_from_bounds(bounds),
                                        );
                                    }
                                }),
                        )
                    }),
            )
            .when(!self.desktop_workspace.switcher_open, |this| {
                this.child(
                    div()
                        .id("desktop-canvas-command-bar")
                        .debug_selector(|| "desktop-canvas-command-bar".to_string())
                        .mb(px(24.0))
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .w(px(620.0))
                                .h(px(56.0))
                                .flex()
                                .items_center()
                                .gap(px(12.0))
                                .px(px(12.0))
                                .rounded(px(16.0))
                                .border_1()
                                .border_color(rgba(theme.dock_line))
                                .bg(rgba(theme.dock))
                                .shadow_lg()
                                .child(
                                    div()
                                        .id("desktop-workspace-search")
                                        .debug_selector(|| "desktop-workspace-search".to_string())
                                        .relative()
                                        .w(px(300.0))
                                        .h(px(36.0))
                                        .px(px(10.0))
                                        .flex()
                                        .items_center()
                                        .rounded(px(8.0))
                                        .border_1()
                                        .border_color(rgba(theme.dock_line))
                                        .bg(rgba(self.desktop_input_fill()))
                                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                            focus.focus(window);
                                            cx.stop_propagation();
                                        })
                                        .child(
                                            Input::new(&input).appearance(false).cleanable(true),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgba(theme.text_secondary))
                                        .child(ui::i18n::format("workspace.workspace_count", &[("count", self.desktop_workspace.workspaces.len().to_string())])),
                                )
                                .child(div().flex_1())
                                .child(
                                    Button::new("desktop-workspace-create")
                                        .debug_selector(|| "desktop-workspace-create".to_string())
                                        .label(ui::i18n::text("workspace.new_workspace"))
                                        .custom(glass_button_variant(theme, cx))
                                        .disabled(self.desktop_workspace.busy)
                                        .on_click(cx.listener(|app, _, window, cx| {
                                            app.create_desktop_workspace(window, cx)
                                        })),
                                ),
                        ),
                )
            })
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
}
