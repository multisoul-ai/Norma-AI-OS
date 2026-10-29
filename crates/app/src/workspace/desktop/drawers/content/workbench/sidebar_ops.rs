//! On-demand Git and listing options panel for the explorer sidebar.

use super::*;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div};
use ui::component::{button::Button, input::Input, Disableable, Selectable, Sizable};
use workspace::content::GitAction;

impl MultisoulApp {
    /// On-demand panel: comparison mode, listing options and Git batch actions.
    pub(super) fn render_workbench_ops(&self, cx: &mut Context<Self>) -> Div {
        let theme = self.desktop_theme();
        let s = &self.desktop_workspace.workbench;
        let local = s.saved.comparison == Comparison::Local;
        let mut panel = div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(8.0))
            .rounded(px(9.0))
            .bg(rgba(ui::icons::with_alpha(theme.text_primary, 0x0a)))
            .border_1()
            .border_color(rgba(theme.node_stroke));
        panel = panel.child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(4.0))
                .child(
                    Button::new("files-local")
                        .label(ui::i18n::text("drawers.local_changes"))
                        .small()
                        .selected(local)
                        .disabled(s.busy)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.desktop_workspace.workbench.saved.comparison = Comparison::Local;
                            app.refresh_file_workbench(cx);
                        })),
                )
                .child(
                    Button::new("files-branch")
                        .label(ui::i18n::text("drawers.branch_changes"))
                        .small()
                        .selected(!local)
                        .disabled(s.busy)
                        .on_click(cx.listener(|app, _, _, cx| {
                            let s = &mut app.desktop_workspace.workbench;
                            if let Some(target) =
                                s.git.as_ref().and_then(|g| g.default_target.clone())
                            {
                                s.saved.comparison = Comparison::Branch(target);
                            } else {
                                s.saved.comparison = Comparison::Branch(String::new());
                                s.error = Some(ui::i18n::LocalizedText::new(
                                    "drawers.select_branch_first",
                                ));
                            }
                            app.refresh_file_workbench(cx);
                        })),
                ),
        );
        if !local {
            if let Some(picker) = &s.branch_picker {
                panel = panel.child(
                    div().h(px(30.0)).w_full().child(
                        ui::component::select::Select::new(picker)
                            .small()
                            .w_full()
                            .placeholder(ui::i18n::text("drawers.select_branch")),
                    ),
                );
            }
        }
        panel = panel.child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(4.0))
                .child(
                    Button::new("files-hidden")
                        .label(ui::i18n::text("drawers.hidden_files"))
                        .small()
                        .selected(s.saved.hidden)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.desktop_workspace.workbench.saved.hidden ^= true;
                            app.refresh_file_workbench(cx);
                        })),
                )
                .child(
                    Button::new("files-ignored")
                        .label(ui::i18n::text("drawers.ignored_files"))
                        .small()
                        .selected(s.saved.ignored)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.desktop_workspace.workbench.saved.ignored ^= true;
                            app.refresh_file_workbench(cx);
                        })),
                )
                .child(
                    Button::new("files-refresh")
                        .label(ui::i18n::text("drawers.refresh"))
                        .small()
                        .on_click(cx.listener(|app, _, _, cx| app.refresh_file_workbench(cx))),
                ),
        );
        let Some(git) = s.git.as_ref().filter(|_| local) else {
            return panel;
        };
        let stage = git
            .changes
            .iter()
            .filter(|c| c.unstaged)
            .map(|c| c.path.clone())
            .collect::<Vec<_>>();
        let unstage = git
            .changes
            .iter()
            .filter(|c| c.staged)
            .map(|c| c.path.clone())
            .collect::<Vec<_>>();
        let disabled = s.busy || !git.writable;
        let has_staged = !unstage.is_empty();
        panel = panel.child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(4.0))
                .child(
                    Button::new("files-stage-all")
                        .label(ui::i18n::text("drawers.stage_all"))
                        .small()
                        .disabled(disabled || stage.is_empty())
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.workbench_git_action(GitAction::Stage(stage.clone()), cx)
                        })),
                )
                .child(
                    Button::new("files-unstage-all")
                        .label(ui::i18n::text("drawers.unstage"))
                        .small()
                        .disabled(disabled || !has_staged)
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.workbench_git_action(GitAction::Unstage(unstage.clone()), cx)
                        })),
                )
                .child(
                    Button::new("files-commit-toggle")
                        .label(ui::i18n::text("drawers.commit"))
                        .small()
                        .selected(s.commit_open)
                        .disabled(disabled || !has_staged)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.desktop_workspace.workbench.commit_open ^= true;
                            cx.notify();
                        })),
                ),
        );
        if s.commit_open {
            if let Some(input) = &s.commit {
                panel = panel.child(Input::new(input).small());
            }
            panel = panel.child(
                Button::new("files-commit")
                    .label(ui::i18n::text("drawers.commit_staged"))
                    .small()
                    .disabled(disabled || !has_staged)
                    .on_click(cx.listener(|app, _, _, cx| {
                        let message = app
                            .desktop_workspace
                            .workbench
                            .commit
                            .as_ref()
                            .map(|i| i.read(cx).value().to_string())
                            .unwrap_or_default();
                        let s = &mut app.desktop_workspace.workbench;
                        s.commit_open = false;
                        s.ops_open = false;
                        app.workbench_git_action(GitAction::Commit(message), cx);
                    })),
            );
        }
        panel
    }
}
