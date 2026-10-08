use super::{DesktopDeleteTarget, DesktopRenameTarget};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful};
use ui::component::button::{Button, ButtonVariants};
use ui::component::input::Input;
use ui::component::{Disableable, StyledExt};

impl MultisoulApp {
    pub(crate) fn render_desktop_delete_confirmation(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let (title, name, scope) = match self
            .desktop_workspace
            .pending_delete
            .as_ref()
            .expect("confirmation target must exist")
        {
            DesktopDeleteTarget::Workspace { name, .. } => (
                ui::i18n::text("workspace.delete_workspace"),
                name.as_str(),
                ui::i18n::text("workspace.its_project_areas_and_cards"),
            ),
            DesktopDeleteTarget::Area { name, .. } => (
                ui::i18n::text("workspace.remove_project_area"),
                name.as_str(),
                ui::i18n::text("workspace.its_canvas_and_cards"),
            ),
        };
        let running = self.desktop_delete_running_count(cx);
        div().id("desktop-delete-confirmation")
            .debug_selector(|| "desktop-delete-confirmation".to_string())
            .absolute().inset_0().occlude().flex().items_center().justify_center().bg(rgba(0x00000091))
            .child(div().w(px(450.0)).flex().flex_col().gap(px(16.0)).p(px(24.0))
                .rounded(px(16.0)).bg(rgba(theme.panel_high))
                .child(div().text_size(px(19.0)).font_semibold().text_color(rgba(theme.text_primary)).child(title))
                .child(div().text_color(rgba(theme.text_secondary)).child(ui::i18n::format("workspace.remove_name_and_scope_the_project_folder_and_git_worktrees_stay", &[("name", (name).to_string()), ("scope", (scope).to_string())])))
                .child(div().text_color(rgba(if running > 0 { theme.destructive } else { theme.text_tertiary }))
                    .child(ui::i18n::format("workspace.running_running_agent_terminal_sessions_will_stop", &[("running", (running).to_string())])))
                .when_some(self.desktop_workspace.error.clone(), |this, error| this.child(
                    div().text_size(px(12.0)).text_color(rgba(theme.destructive)).child(error)
                ))
                .child(div().flex().justify_end().gap(px(8.0))
                    .child(Button::new("desktop-delete-cancel")
                        .debug_selector(|| "desktop-delete-cancel".to_string()).label(ui::i18n::text("common.cancel"))
                        .on_click(cx.listener(|app, _, _, cx| app.cancel_desktop_delete(cx))))
                    .child(Button::new("desktop-delete-confirm")
                        .debug_selector(|| "desktop-delete-confirm".to_string()).label(ui::i18n::text("common.delete"))
                        .disabled(self.desktop_workspace.busy)
                        .on_click(cx.listener(|app, _, _, cx| app.confirm_desktop_delete(cx))))))
    }

    pub(crate) fn render_desktop_project_destination(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let path = self
            .desktop_workspace
            .pending_project_path
            .as_ref()
            .expect("project destination requires a path");
        let workspaces = self.desktop_workspace.workspaces.clone();
        let busy = self.desktop_workspace.busy;
        div()
            .id("desktop-project-destination")
            .debug_selector(|| "desktop-project-destination".to_string())
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000091))
            .child(
                div()
                    .w(px(520.0))
                    .max_h(px(620.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .p(px(22.0))
                    .rounded(px(18.0))
                    .bg(rgba(theme.panel_high))
                    .child(
                        div()
                            .text_size(px(19.0))
                            .font_semibold()
                            .text_color(rgba(theme.text_primary))
                            .child(ui::i18n::text("workspace.add_project_to_workspace")),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(theme.text_secondary))
                            .child(path.display().to_string()),
                    )
                    .child(
                        div()
                            .id("desktop-project-destination-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_scroll()
                            .flex()
                            .flex_col()
                            .gap(px(7.0))
                            .children(workspaces.into_iter().enumerate().map(
                                |(index, workspace)| {
                                    let id = workspace.id.clone();
                                    Button::new(("desktop-project-destination", index))
                                        .debug_selector(move || {
                                            format!("desktop-project-destination-{index}")
                                        })
                                        .label(workspace.name)
                                        .disabled(busy)
                                        .on_click(cx.listener(move |app, _, window, cx| {
                                            app.choose_desktop_project_destination(
                                                Some(id.clone()),
                                                window,
                                                cx,
                                            )
                                        }))
                                },
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(
                                Button::new("desktop-project-destination-new")
                                    .debug_selector(|| {
                                        "desktop-project-destination-new".to_string()
                                    })
                                    .label(ui::i18n::text("shortcuts.new_workspace"))
                                    .disabled(busy)
                                    .on_click(cx.listener(|app, _, window, cx| {
                                        app.choose_desktop_project_destination(None, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("desktop-project-destination-cancel")
                                    .debug_selector(|| {
                                        "desktop-project-destination-cancel".to_string()
                                    })
                                    .label(ui::i18n::text("common.cancel"))
                                    .disabled(busy)
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        app.cancel_desktop_project_destination(cx)
                                    })),
                            ),
                    ),
            )
    }

    pub(crate) fn render_desktop_rename_dialog(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        let input = self.desktop_workspace.rename_input.clone();
        let area = matches!(
            self.desktop_workspace.rename_target,
            Some(DesktopRenameTarget::Area { .. })
        );
        div()
            .id("desktop-workspace-rename-dialog")
            .debug_selector(|| "desktop-workspace-rename-dialog".to_string())
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .occlude()
            .bg(rgba(0x00000080))
            .on_action(cx.listener(|app, _: &ui::component::input::Enter, _, cx| {
                app.save_desktop_rename(cx);
            }))
            .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    app.cancel_desktop_rename(cx);
                    cx.stop_propagation();
                }
            }))
            .child(
                div()
                    .w(px(420.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p(px(22.0))
                    .rounded(px(16.0))
                    .bg(rgba(theme.panel_high))
                    .text_color(rgba(theme.text_primary))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_semibold()
                            .child(ui::i18n::text(if area {
                                "workspace.rename_canvas"
                            } else {
                                "workspace.rename_workspace"
                            })),
                    )
                    .when_some(input, |this, input| this.child(Input::new(&input)))
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
                            .child(
                                Button::new("desktop-workspace-rename-cancel")
                                    .debug_selector(|| {
                                        "desktop-workspace-rename-cancel".to_string()
                                    })
                                    .label(ui::i18n::text("common.cancel"))
                                    .disabled(self.desktop_workspace.busy)
                                    .on_click(
                                        cx.listener(|app, _, _, cx| app.cancel_desktop_rename(cx)),
                                    ),
                            )
                            .child(
                                Button::new("desktop-workspace-rename-save")
                                    .debug_selector(|| "desktop-workspace-rename-save".to_string())
                                    .label(ui::i18n::text("common.save"))
                                    .primary()
                                    .disabled(self.desktop_workspace.busy)
                                    .on_click(
                                        cx.listener(|app, _, _, cx| app.save_desktop_rename(cx)),
                                    ),
                            ),
                    ),
            )
    }
}
