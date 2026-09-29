use super::DesktopDeleteTarget;
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
                "Delete Workspace",
                name.as_str(),
                "its project areas and cards",
            ),
            DesktopDeleteTarget::Area { name, .. } => {
                ("Remove Project Area", name.as_str(), "its canvas and cards")
            }
        };
        let running = self.desktop_delete_running_count(cx);
        div().id("desktop-delete-confirmation")
            .debug_selector(|| "desktop-delete-confirmation".to_string())
            .absolute().inset_0().occlude().flex().items_center().justify_center().bg(rgba(0x00000091))
            .child(div().w(px(450.0)).flex().flex_col().gap(px(16.0)).p(px(24.0))
                .rounded(px(16.0)).bg(rgba(theme.panel_high))
                .child(div().text_size(px(19.0)).font_semibold().text_color(rgba(theme.text_primary)).child(title))
                .child(div().text_color(rgba(theme.text_secondary)).child(format!("Remove {name} and {scope}? The project folder and Git worktrees stay on disk.")))
                .child(div().text_color(rgba(if running > 0 { theme.destructive } else { theme.text_tertiary }))
                    .child(format!("{running} running Agent / terminal sessions will stop.")))
                .child(div().flex().justify_end().gap(px(8.0))
                    .child(Button::new("desktop-delete-cancel")
                        .debug_selector(|| "desktop-delete-cancel".to_string()).label("Cancel")
                        .on_click(cx.listener(|app, _, _, cx| app.cancel_desktop_delete(cx))))
                    .child(Button::new("desktop-delete-confirm")
                        .debug_selector(|| "desktop-delete-confirm".to_string()).label("Delete")
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
                            .child("Add Project to Workspace"),
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
                                    .label("New Workspace")
                                    .on_click(cx.listener(|app, _, window, cx| {
                                        app.choose_desktop_project_destination(None, window, cx)
                                    })),
                            )
                            .child(
                                Button::new("desktop-project-destination-cancel")
                                    .debug_selector(|| {
                                        "desktop-project-destination-cancel".to_string()
                                    })
                                    .label("Cancel")
                                    .on_click(cx.listener(|app, _, _, cx| {
                                        app.cancel_desktop_project_destination(cx)
                                    })),
                            ),
                    ),
            )
    }

    pub(crate) fn render_desktop_agent_worktree_picker(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = self.theme;
        let pending = self
            .desktop_workspace
            .agent_creation
            .as_ref()
            .expect("Agent creation requires a project area");
        let area_name = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| area.id == pending.area_id)
            .map(|area| area.display_name.as_str())
            .unwrap_or("Project");
        let choices = pending.choices.clone();
        div().id("desktop-agent-worktree-picker")
            .debug_selector(|| "desktop-agent-worktree-picker".to_string())
            .absolute().inset_0().occlude().flex().items_center().justify_center().bg(rgba(0x00000091))
            .child(div().w(px(560.0)).max_h(px(620.0)).flex().flex_col().gap(px(14.0)).p(px(22.0))
                .rounded(px(18.0)).bg(rgba(theme.panel_high))
                .child(div().text_size(px(19.0)).font_semibold().text_color(rgba(theme.text_primary))
                    .child("Choose Agent Worktree"))
                .child(div().text_size(px(12.0)).text_color(rgba(theme.text_secondary))
                    .child(format!("Project: {area_name}. Each Agent can use any worktree, including one already in use.")))
                .child(div().id("desktop-agent-worktree-scroll").flex_1().min_h(px(0.0)).overflow_scroll()
                    .flex().flex_col().gap(px(7.0))
                    .when(pending.loading, |this| this.child(div().text_color(rgba(theme.text_secondary)).child("Loading worktrees…")))
                    .children(choices.into_iter().enumerate().map(|(index, path)| {
                        let label = path.display().to_string();
                        Button::new(("desktop-agent-worktree", index))
                            .debug_selector(move || format!("desktop-agent-worktree-{index}"))
                            .label(label)
                            .disabled(pending.loading)
                            .on_click(cx.listener(move |app, _, window, cx| app.choose_desktop_agent_worktree(path.clone(), window, cx)))
                    })))
                .child(div().flex().justify_end().child(Button::new("desktop-agent-worktree-cancel")
                    .debug_selector(|| "desktop-agent-worktree-cancel".to_string())
                    .label("Cancel")
                    .on_click(cx.listener(|app, _, _, cx| app.cancel_desktop_agent_creation(cx)))))
            )
    }

    pub(crate) fn render_desktop_rename_dialog(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = self.theme;
        let input = self.desktop_workspace.rename_input.clone();
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
            .child(
                div()
                    .w(px(420.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .p(px(22.0))
                    .rounded(px(16.0))
                    .bg(rgba(theme.panel_high))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_semibold()
                            .child("Rename Workspace"),
                    )
                    .when_some(input, |this, input| this.child(Input::new(&input)))
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
                                    .label("Cancel")
                                    .on_click(
                                        cx.listener(|app, _, _, cx| app.cancel_desktop_rename(cx)),
                                    ),
                            )
                            .child(
                                Button::new("desktop-workspace-rename-save")
                                    .debug_selector(|| "desktop-workspace-rename-save".to_string())
                                    .label("Save")
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
