use super::layout::explorer_layout;
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, Window};
use storage::DesktopContentTab;
use ui::component::{button::Button, Disableable};
use workspace::content::GitAction;

impl MultisoulApp {
    /// Chromeless file workbench: the tab strip sits directly above document + tree.
    pub(crate) fn render_file_workbench(
        &mut self,
        tab: &DesktopContentTab,
        width: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.desktop_theme();
        self.ensure_file_workbench(window, cx);
        let layout = explorer_layout(width);
        if !layout.compact {
            self.desktop_workspace.workbench.sidebar_open = false;
        }
        let s = &self.desktop_workspace.workbench;
        let show_tree = !layout.compact || s.sidebar_open || s.saved.selected.is_none();
        let can_close_tree = layout.compact && s.saved.selected.is_some();
        let mut root = div()
            .id("files-workbench")
            .debug_selector(|| "files-workbench".to_string())
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
        let toast = self.render_workbench_toast(cx);
        let mut body = div().relative().flex_1().min_h(px(0.0)).flex();
        if !layout.compact || !show_tree {
            let doc_width = if layout.compact {
                width
            } else {
                layout.document
            };
            body = body.child(self.render_workbench_document(tab, doc_width, layout.compact, cx));
        }
        if show_tree {
            // The sidebar's own 1px left border is the divider, so it owns one extra pixel.
            let tree_width = if layout.compact {
                width
            } else {
                layout.sidebar + 1.0
            };
            body = body.child(self.render_workbench_sidebar(tree_width, can_close_tree, cx));
        }
        root.child(body.children(toast)).into_any_element()
    }

    /// Transient feedback floats over the document so it never resizes the preview.
    fn render_workbench_toast(&self, cx: &mut Context<Self>) -> Option<Div> {
        let theme = self.desktop_theme();
        let s = &self.desktop_workspace.workbench;
        let (text, color) = match (&s.error, &s.notice) {
            (Some(error), _) => (error.clone(), theme.destructive),
            (None, Some(notice)) => (notice.clone(), theme.text_secondary),
            _ => return None,
        };
        Some(
            div()
                .absolute()
                .left(px(10.0))
                .bottom(px(10.0))
                .max_w(px(320.0))
                .child(
                    div()
                        .id("files-toast")
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(9.0))
                        .bg(rgba(ui::icons::with_alpha(
                            self.desktop_theme().panel,
                            0xfa,
                        )))
                        .border_1()
                        .border_color(rgba(theme.node_stroke))
                        .text_size(px(12.0))
                        .text_color(rgba(color))
                        .cursor_pointer()
                        .on_click(cx.listener(|app, _, _, cx| {
                            let s = &mut app.desktop_workspace.workbench;
                            s.error = None;
                            s.notice = None;
                            cx.notify();
                        }))
                        .child(text),
                ),
        )
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
