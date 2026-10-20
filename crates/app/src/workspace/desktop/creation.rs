use crate::app::{AppScreen, MultisoulApp};
use gpui::{div, prelude::*, px, rgba, Context, EntityInputHandler, Focusable, Window};
use storage::DesktopWorkspaceStore;
use ui::component::{
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    Disableable, StyledExt, WindowExt,
};

impl MultisoulApp {
    pub(super) fn create_desktop_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.desktop_workspace.busy || self.desktop_workspace.create_input.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx).placeholder(ui::i18n::input_placeholder(
                "workspace.e_g_website_redesign",
                window,
                cx,
            ))
        });
        cx.subscribe_in(&input, window, |app, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => app.submit_desktop_workspace_name(window, cx),
            InputEvent::Change => {
                app.desktop_workspace.error = None;
                cx.notify();
            }
            _ => {}
        })
        .detach();
        self.desktop_workspace.create_input = Some(input.clone());
        self.desktop_workspace.error = None;
        let host = cx.weak_entity();
        let view = cx.new(|_| WorkspaceCreationForm { host });
        let focus = input.focus_handle(cx);
        let theme = self.theme;
        window.open_dialog(cx, move |dialog, window, _| {
            dialog
                .width(px(460.0).min(window.viewport_size().width - px(40.0)))
                .margin_top((window.viewport_size().height / 2.0 - px(160.0)).max(px(24.0)))
                .p(px(24.0))
                .rounded(px(18.0))
                .bg(rgba(ui::icons::with_alpha(theme.panel_high, 0xff)))
                .border_color(rgba(theme.node_stroke))
                .close_button(false)
                .overlay_closable(false)
                // The form owns Cancel/Confirm so Escape cannot close a pending save.
                .keyboard(false)
                .child(view.clone())
        });
        focus.focus(window);
        cx.notify();
    }

    fn render_workspace_creation_form(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let busy = self.desktop_workspace.busy;
        let input = self.desktop_workspace.create_input.clone();
        let valid = input.as_ref().is_some_and(|input| {
            DesktopWorkspaceStore::validate_name(&input.read(cx).value()).is_ok()
        });
        let theme = self.theme;
        let count = input
            .as_ref()
            .map_or(0, |input| input.read(cx).value().trim().chars().count());
        div()
            .id("desktop-workspace-create-form")
            .key_context("Dialog")
            .flex()
            .flex_col()
            .gap(px(18.0))
            .text_color(rgba(theme.text_primary))
            .on_action(
                cx.listener(|app, _: &ui::component::input::Enter, window, cx| {
                    // Consume the single-line input's propagated action so Enter cannot insert a newline.
                    app.submit_desktop_workspace_name(window, cx);
                }),
            )
            .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    app.cancel_desktop_workspace_creation(window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(super::dialogs::desktop_form_header(
                ui::i18n::text("shortcuts.new_workspace"),
                ui::i18n::text("workspace.keep_related_projects_and_agents_together").into(),
                theme,
            ))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_medium()
                            .child(ui::i18n::text("workspace.workspace_name")),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(if count > 80 {
                                theme.destructive
                            } else {
                                theme.text_secondary
                            }))
                            .child(format!("{count}/80")),
                    ),
            )
            .when_some(input, |this, input| {
                this.child(
                    div()
                        .debug_selector(|| "desktop-workspace-create-name".into())
                        .child(Input::new(&input).disabled(busy)),
                )
            })
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(rgba(theme.text_secondary))
                    .child(if count > 80 {
                        ui::i18n::text("workspace.use_80_characters_or_fewer")
                    } else {
                        ui::i18n::text("workspace.start_with_an_empty_canvas_add_projects_whenever_you_re_ready")
                    }),
            )
            .when_some(self.desktop_workspace.error.clone(), |this, error| {
                this.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.destructive))
                        .child(error),
                )
            })
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    .child(
                        Button::new("desktop-workspace-create-cancel")
                            .debug_selector(|| "desktop-workspace-create-cancel".into())
                            .label(ui::i18n::text("common.cancel"))
                            .disabled(busy)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.cancel_desktop_workspace_creation(window, cx)
                            })),
                    )
                    .child(
                        Button::new("desktop-workspace-create-confirm")
                            .debug_selector(|| "desktop-workspace-create-confirm".into())
                            .label(if busy {
                                ui::i18n::text("workspace.creating")
                            } else {
                                ui::i18n::text("workspace.create_workspace")
                            })
                            .primary()
                            .disabled(busy || !valid)
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.submit_desktop_workspace_name(window, cx)
                            })),
                    ),
            )
    }

    pub(super) fn cancel_desktop_workspace_creation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.busy {
            return;
        }
        self.desktop_workspace.create_input = None;
        self.desktop_workspace.error = None;
        window.close_dialog(cx);
        cx.notify();
    }

    pub(super) fn submit_desktop_workspace_name(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_workspace.busy {
            return;
        }
        let Some(input) = self.desktop_workspace.create_input.clone() else {
            return;
        };
        let composing = input.update(cx, |input, cx| {
            input.marked_text_range(window, cx).is_some()
        });
        if composing {
            return;
        }
        let value = input.read(cx).value();
        let Ok(name) = DesktopWorkspaceStore::validate_name(&value) else {
            return;
        };
        let name = name.to_owned();
        self.desktop_workspace.busy = true;
        self.desktop_workspace.error = None;
        cx.notify();
        let store = self.desktop_workspace.store.clone();
        let wallpaper = if self.wallpaper_settings.scoped.context.is_some() {
            self.wallpaper_settings.scoped.workspace_selection()
        } else {
            self.wallpaper_settings.scoped.global.clone()
        }
        .reference();
        let work = cx
            .background_executor()
            .spawn(async move { store.create_with_wallpaper(&name, &wallpaper) });
        cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            this.update_in(cx, |app, window, cx| {
                app.desktop_workspace.busy = false;
                match result {
                    Ok(created) => {
                        let id = created.id.clone();
                        app.desktop_workspace.workspaces.push(created);
                        app.desktop_workspace.create_input = None;
                        window.close_dialog(cx);
                        if app.desktop_workspace.pending_project_path.is_some() {
                            app.choose_desktop_project_destination(Some(id), window, cx);
                        } else {
                            app.detach_visible_desktop_area(cx);
                            app.close_desktop_switcher(cx);
                            app.desktop_workspace.finish_motion_log(
                                "workspace_created",
                                cx.background_executor().now(),
                            );
                            app.desktop_workspace.transition = None;
                            app.desktop_workspace.pending_navigation = None;
                            app.activate_desktop_workspace_view(&id, cx);
                            app.desktop_workspace.areas.clear();
                            app.desktop_workspace.active_area_id = None;
                            app.desktop_workspace.expanded_area_id = None;
                            app.screen = AppScreen::DesktopWorkspace;
                            app.bind_desktop_wallpaper(cx);
                            app.start_desktop_status_refresh(cx);
                        }
                    }
                    Err(error) => {
                        app.desktop_workspace.error = Some(ui::i18n::message(
                            "workspace.could_not_create_workspace_error",
                            &[("error", (error).to_string())],
                        ))
                    }
                }
                cx.notify();
            })?;
            Ok::<(), anyhow::Error>(())
        })
        .detach();
    }
}

// Render after the host's render borrow has ended; the native dialog builder runs inside it.
struct WorkspaceCreationForm {
    host: gpui::WeakEntity<MultisoulApp>,
}
impl gpui::Render for WorkspaceCreationForm {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.host.upgrade() {
            Some(host) => host.update(cx, |app, cx| app.render_workspace_creation_form(cx)),
            None => div().id("desktop-workspace-create-form"),
        }
    }
}
