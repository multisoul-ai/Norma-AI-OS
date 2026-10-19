use super::*;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, Window};
use storage::DesktopContentTab;
use ui::component::{button::Button, input::Input, Disableable, Sizable};
use workspace::content::GitAction;

impl MultisoulApp {
    pub(crate) fn render_file_workbench(
        &mut self,
        tab: &DesktopContentTab,
        width: f32,
        height: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_file_workbench(window, cx);
        let narrow = width < 520.0;
        let s = &self.desktop_workspace.workbench;
        let sidebar = !narrow || s.sidebar_open || s.saved.selected.is_none();
        let sidebar_width = if narrow {
            width
        } else {
            (width * 0.4).clamp(190.0, 260.0)
        };
        let content_width = if sidebar {
            width - sidebar_width
        } else {
            width
        };
        let theme = self.desktop_theme();
        let mut root = div()
            .size_full()
            .flex()
            .flex_col()
            .text_color(rgba(theme.text_primary))
            .bg(rgba(theme.canvas_from));
        if let Some(paths) = &s.confirm_discard {
            let paths = paths.clone();
            return root
                .child(
                    div()
                        .p(px(16.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .child("撤销这些文件的未暂存修改？已暂存内容会保留。")
                        .child(paths.join("\n"))
                        .child(
                            Button::new("files-discard-confirm")
                                .label("确认撤销")
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.desktop_workspace.workbench.confirm_discard = None;
                                    app.workbench_git_action(GitAction::Discard(paths.clone()), cx);
                                })),
                        )
                        .child(Button::new("files-discard-cancel").label("取消").on_click(
                            cx.listener(|app, _, _, cx| {
                                app.desktop_workspace.workbench.confirm_discard = None;
                                cx.notify();
                            }),
                        )),
                )
                .into_any_element();
        }
        if s.pending_close.is_some() {
            root = root.child(self.render_workbench_unsaved(cx));
        }
        if let Some(error) = &s.error {
            root = root.child(
                div()
                    .p(px(8.0))
                    .text_size(px(12.0))
                    .text_color(rgba(theme.destructive))
                    .child(error.clone()),
            );
        } else if let Some(notice) = &s.notice {
            root = root.child(div().px(px(8.0)).text_size(px(11.0)).child(notice.clone()));
        }
        let body = div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .when(content_width > 1.0, |body| {
                body.child(self.render_workbench_document(tab, content_width, height, cx))
            })
            .when(sidebar, |body| {
                body.child(self.render_workbench_sidebar(sidebar_width, cx))
            });
        root.child(body).into_any_element()
    }

    fn render_workbench_document(
        &self,
        tab: &DesktopContentTab,
        width: f32,
        height: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let s = &self.desktop_workspace.workbench;
        let theme = self.desktop_theme();
        let selected = s.saved.selected.clone();
        let key = s.draft_key();
        let draft = key.as_ref().and_then(|key| s.drafts.get(key));
        let title = format!(
            "{}{}",
            selected.as_deref().unwrap_or("打开文件"),
            if draft.is_some_and(|d| d.dirty) {
                " ●"
            } else {
                ""
            }
        );
        let toolbar = div()
            .h(px(36.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(6.0))
            .child(
                Button::new("files-toggle-tree")
                    .label("目录")
                    .small()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.desktop_workspace.workbench.sidebar_open ^= true;
                        app.hide_desktop_content_browsers();
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(11.0))
                    .child(title),
            )
            .when(s.file.is_some(), |bar| {
                bar.child(Button::new("files-edit").label("编辑").small().on_click(
                    cx.listener(|app, _, window, cx| app.edit_workbench_file(window, cx)),
                ))
            })
            .when(draft.is_some(), |bar| {
                bar.child(
                    Button::new("files-save")
                        .label("保存")
                        .small()
                        .disabled(draft.is_none_or(|d| !d.dirty || d.saving))
                        .on_click(cx.listener(|app, _, _, cx| app.save_workbench_file(cx))),
                )
            })
            .when(selected.is_some(), |bar| {
                bar.child(
                    Button::new("files-open-system")
                        .label("↗")
                        .tooltip("使用系统应用打开")
                        .small()
                        .on_click(cx.listener(|app, _, _, cx| {
                            let s = &app.desktop_workspace.workbench;
                            if let (Some(project), Some(path)) = (&s.project, &s.saved.selected) {
                                if let Ok(url) = url::Url::from_file_path(project.root().join(path))
                                {
                                    cx.open_url(url.as_str());
                                }
                            }
                        })),
                )
            });
        let mut content = div()
            .w(px(width))
            .h_full()
            .flex()
            .flex_col()
            .min_w(px(0.0))
            .child(toolbar);
        if draft.is_some() && s.preview.is_some() {
            content = content.child(
                Button::new("files-preview-mode")
                    .label(if s.show_source { "预览" } else { "源码" })
                    .small()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.desktop_workspace.workbench.show_source ^= true;
                        app.hide_desktop_content_browsers();
                        cx.notify();
                    })),
            );
        }
        if s.diff.is_some() {
            let patch = s.diff.as_deref().unwrap_or_default();
            let added = patch
                .lines()
                .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
                .count();
            let removed = patch
                .lines()
                .filter(|l| l.starts_with('-') && !l.starts_with("---"))
                .count();
            content = content.child(
                div()
                    .px(px(8.0))
                    .text_size(px(11.0))
                    .child(format!("+{added} −{removed}")),
            );
            let other_side = !s.diff_staged;
            content = content.child(
                div()
                    .flex()
                    .gap(px(4.0))
                    .p(px(4.0))
                    .child(
                        Button::new("files-diff-other")
                            .label(if other_side {
                                "查看已暂存"
                            } else {
                                "查看未暂存"
                            })
                            .small()
                            .disabled(s.saved.comparison != Comparison::Local)
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.load_workbench_file(true, other_side, cx)
                            })),
                    )
                    .child(
                        Button::new("files-current-content")
                            .label("当前文件")
                            .small()
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.load_workbench_file(false, false, cx)
                            })),
                    ),
            );
            return content.child(
                div()
                    .id("files-diff-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_scroll()
                    .p(px(10.0))
                    .font_family("Menlo")
                    .text_size(px(11.0))
                    .children(
                        s.diff
                            .as_deref()
                            .unwrap_or_default()
                            .lines()
                            .enumerate()
                            .map(|(i, line)| {
                                let color = if line.starts_with('+') && !line.starts_with("+++") {
                                    theme.accent_green
                                } else if line.starts_with('-') && !line.starts_with("---") {
                                    theme.destructive
                                } else {
                                    theme.text_primary
                                };
                                div()
                                    .id(("files-diff-line", i))
                                    .text_color(rgba(color))
                                    .child(line.to_string())
                            }),
                    ),
            );
        }
        if let Some(draft) = draft.filter(|_| s.show_source || s.preview.is_none()) {
            content = content.child(Input::new(&draft.input).h_full().w_full());
        } else if let Some(preview) = &s.preview {
            content = content.child(div().relative().flex_1().min_h(px(0.0)).child(
                self.browser_scene_element(
                    super::super::super::content_node_id(&tab.id),
                    preview.url.clone(),
                    crate::app::browser::BrowserViewBounds {
                        x: 0.0,
                        y: 0.0,
                        width: f64::from(width.max(1.0)),
                        height: f64::from((height - 38.0).max(50.0)),
                        bottom_corner_radius_px: 0.0,
                    },
                    cx,
                ),
            ));
        } else if let Some(file) = &s.file {
            content = content.child(
                div()
                    .id("files-text-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_scroll()
                    .p(px(12.0))
                    .text_size(px(12.0))
                    .font_family("Menlo")
                    .child(file.text.clone()),
            );
        } else {
            content = content.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .child(if s.loading {
                        "正在打开文件…"
                    } else {
                        "从项目目录树中选择文件"
                    }),
            );
        }
        content
    }

    fn render_workbench_unsaved(&self, cx: &mut Context<Self>) -> Div {
        div()
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child("有未保存的修改，请保存或放弃后关闭。")
            .child(
                Button::new("files-close-save")
                    .label("保存全部")
                    .disabled(self.desktop_workspace.workbench.busy)
                    .on_click(cx.listener(|app, _, _, cx| app.save_all_workbench_drafts(cx))),
            )
            .child(
                Button::new("files-close-discard")
                    .label("放弃草稿并继续")
                    .disabled(
                        self.desktop_workspace
                            .workbench
                            .drafts
                            .values()
                            .any(|d| d.saving)
                            || self.desktop_workspace.workbench.busy,
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.desktop_workspace.workbench.drafts.clear();
                        app.finish_workbench_leaving(cx);
                    })),
            )
            .child(
                Button::new("files-close-cancel")
                    .label("取消")
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.desktop_workspace.workbench.pending_close = None;
                        cx.notify();
                    })),
            )
    }
}
