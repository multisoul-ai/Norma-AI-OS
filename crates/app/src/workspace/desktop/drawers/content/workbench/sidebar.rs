use super::*;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, uniform_list, Context, Div, ElementId};
use ui::component::{button::Button, input::Input, Disableable, Selectable, Sizable};
use workspace::content::GitAction;

impl MultisoulApp {
    pub(super) fn render_workbench_sidebar(&self, width: f32, cx: &mut Context<Self>) -> Div {
        let theme = self.desktop_theme();
        let s = &self.desktop_workspace.workbench;
        let mut pane = div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .p(px(8.0))
            .border_l_1()
            .border_color(rgba(theme.node_stroke))
            .bg(rgba(theme.panel_high));
        if let Some(picker) = &s.project_picker {
            pane = pane.child(
                div().h(px(30.0)).flex_none().w_full().child(
                    ui::component::select::Select::new(picker)
                        .small()
                        .menu_width(px(width - 16.0))
                        .w_full()
                        .placeholder("选择项目")
                        .disabled(s.busy),
                ),
            );
        }
        if let Some(input) = &s.filter {
            pane = pane.child(Input::new(input).small());
        }
        pane = pane.child(
            div()
                .flex()
                .gap(px(4.0))
                .child(
                    Button::new("files-hidden")
                        .label("隐藏")
                        .small()
                        .selected(s.saved.hidden)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.desktop_workspace.workbench.saved.hidden ^= true;
                            app.refresh_file_workbench(cx);
                        })),
                )
                .child(
                    Button::new("files-ignored")
                        .label("忽略")
                        .small()
                        .selected(s.saved.ignored)
                        .on_click(cx.listener(|app, _, _, cx| {
                            app.desktop_workspace.workbench.saved.ignored ^= true;
                            app.refresh_file_workbench(cx);
                        })),
                )
                .child(
                    Button::new("files-refresh")
                        .label("刷新")
                        .small()
                        .on_click(cx.listener(|app, _, _, cx| app.refresh_file_workbench(cx))),
                ),
        );
        pane = pane.child(self.render_workbench_changes(cx));
        let count = s.rows.len();
        pane.child(
            div()
                .text_size(px(11.0))
                .child(format!("全部文件 · {count}")),
        )
        .child(
            uniform_list(
                "files-tree",
                count,
                cx.processor(|app, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| app.render_workbench_file_row(index, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1()
            .min_h(px(0.0)),
        )
    }

    fn render_workbench_file_row(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let s = &self.desktop_workspace.workbench;
        let row = &s.rows[index];
        let path = row.path.clone();
        let directory = row.directory;
        let expanded = s.saved.expanded.contains(&path);
        let selected = s.saved.selected.as_ref() == Some(&path);
        let glyph = if directory {
            if expanded {
                "▾"
            } else {
                "▸"
            }
        } else {
            "·"
        };
        div()
            .h(px(28.0))
            .pl(px((row.depth.min(12) * 12) as f32))
            .child(
                Button::new(ElementId::Name(format!("files-path-{path}").into()))
                    .label(format!("{glyph} {}", row.label))
                    .small()
                    .w_full()
                    .selected(selected)
                    .tooltip(path.clone())
                    .on_click(cx.listener(move |app, _, _, cx| {
                        if directory {
                            let expanded = &mut app.desktop_workspace.workbench.saved.expanded;
                            if !expanded.remove(&path) {
                                expanded.insert(path.clone());
                            }
                            app.refresh_file_workbench(cx);
                        } else {
                            app.select_workbench_path(path.clone(), false, false, cx);
                        }
                    })),
            )
    }

    fn render_workbench_changes(&self, cx: &mut Context<Self>) -> Div {
        let s = &self.desktop_workspace.workbench;
        let local = s.saved.comparison == Comparison::Local;
        let mut pane = div().flex().flex_col().gap(px(4.0)).max_h(px(300.0));
        pane = pane.child(
            div()
                .flex()
                .gap(px(4.0))
                .child(
                    Button::new("files-local")
                        .label("本地变更")
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
                        .label("分支变更")
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
                                s.error = Some("请选择比较分支".into());
                            }

                            app.refresh_file_workbench(cx);
                        })),
                ),
        );
        let Some(git) = &s.git else {
            return pane.child(
                div().text_size(px(10.0)).child(
                    s.git_error
                        .clone()
                        .unwrap_or_else(|| "正在读取 Git…".into()),
                ),
            );
        };
        pane = pane.child(div().text_size(px(10.0)).child(format!(
            "{} · {} 个文件",
            git.branch,
            git.changes.len()
        )));
        if !local {
            if let Some(picker) = &s.branch_picker {
                pane = pane.child(
                    div().h(px(30.0)).flex_none().w_full().child(
                        ui::component::select::Select::new(picker)
                            .small()
                            .w_full()
                            .placeholder("选择比较分支"),
                    ),
                );
            }
        }
        pane = pane.child(
            uniform_list(
                "files-changes",
                git.changes.len(),
                cx.processor(|app, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|index| app.render_workbench_change(index, cx))
                        .collect::<Vec<_>>()
                }),
            )
            .h(px((git.changes.len() as f32 * 30.0).clamp(1.0, 145.0))),
        );
        if local {
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
            pane = pane.child(
                div()
                    .flex()
                    .gap(px(4.0))
                    .child(
                        Button::new("files-stage-all")
                            .label("全部暂存")
                            .small()
                            .disabled(disabled || stage.is_empty())
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.workbench_git_action(GitAction::Stage(stage.clone()), cx)
                            })),
                    )
                    .child(
                        Button::new("files-unstage-all")
                            .label("取消暂存")
                            .small()
                            .disabled(disabled || unstage.is_empty())
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.workbench_git_action(GitAction::Unstage(unstage.clone()), cx)
                            })),
                    ),
            );
            if let Some(input) = &s.commit {
                pane = pane.child(Input::new(input).small());
            }
            pane = pane.child(
                Button::new("files-commit")
                    .label("提交已暂存文件")
                    .small()
                    .disabled(disabled || !git.changes.iter().any(|c| c.staged))
                    .on_click(cx.listener(|app, _, _, cx| {
                        let message = app
                            .desktop_workspace
                            .workbench
                            .commit
                            .as_ref()
                            .map(|i| i.read(cx).value().to_string())
                            .unwrap_or_default();
                        app.workbench_git_action(GitAction::Commit(message), cx);
                    })),
            );
        }
        pane
    }

    fn render_workbench_change(&self, index: usize, cx: &mut Context<Self>) -> Div {
        let s = &self.desktop_workspace.workbench;
        let change = &s.git.as_ref().expect("changes require snapshot").changes[index];
        let path = change.path.clone();
        let selected = s.saved.selected.as_ref() == Some(&path) && s.diff.is_some();
        let local = s.saved.comparison == Comparison::Local;
        let staged = change.staged;
        let disabled = s.busy || !s.git.as_ref().is_some_and(|g| g.writable);
        let mut row = div().h(px(30.0)).flex().gap(px(2.0)).child(
            Button::new(ElementId::Name(format!("files-diff-{index}").into()))
                .label(format!(
                    "{} {}{}",
                    change.status,
                    if staged { "● " } else { "" },
                    change.path
                ))
                .small()
                .flex_1()
                .min_w(px(0.0))
                .selected(selected)
                .tooltip(change.path.clone())
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.select_workbench_path(path.clone(), true, staged, cx)
                })),
        );
        if local {
            let path = change.path.clone();
            row = row.child(
                Button::new(ElementId::Name(format!("files-stage-{index}").into()))
                    .label(if staged { "−" } else { "+" })
                    .small()
                    .disabled(disabled)
                    .tooltip(if staged { "取消暂存" } else { "暂存" })
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.workbench_git_action(
                            if staged {
                                GitAction::Unstage(vec![path.clone()])
                            } else {
                                GitAction::Stage(vec![path.clone()])
                            },
                            cx,
                        )
                    })),
            );
            if change.unstaged && !change.untracked {
                let path = change.path.clone();
                row = row.child(
                    Button::new(ElementId::Name(format!("files-discard-{index}").into()))
                        .label("↶")
                        .small()
                        .disabled(disabled)
                        .tooltip("撤销未暂存修改")
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.desktop_workspace.workbench.confirm_discard =
                                Some(vec![path.clone()]);
                            app.hide_desktop_content_browsers();
                            cx.notify();
                        })),
                );
            }
        }
        row
    }
}
