//! Right column of the file workbench: workspace picker, filter and two collapsible trees.
//!
//! Structure follows the prototype's `.ex-side`: left-aligned tree rows (chevron, glyph, name),
//! a green changed-file tree above the all-files tree, and low-frequency Git operations kept
//! behind the section's "⋯" control. Both trees share one virtualized list.

use super::document::{tooltip_view, GREEN, GUTTER, LINE, PANEL, TEXT_MUTED, TEXT_SECONDARY};
use super::tree::{change_rows, row_is_staged, ChangeRow};
use super::tree_row::{
    activates, chevron, glyph_element, glyph_for, name_label, Action, FOCUS, ROW_HEIGHT,
};
use super::*;
use crate::app::MultisoulApp;
use gpui::{
    div, prelude::*, px, rgba, svg, uniform_list, Context, Div, ElementId, KeyDownEvent, Stateful,
};
use std::rc::Rc;
use ui::component::{input::Input, Sizable};
use workspace::content::GitAction;

enum Item {
    ChangesHead {
        count: usize,
    },
    ChangeDir {
        path: String,
        label: String,
        depth: usize,
        open: bool,
    },
    Change {
        index: usize,
        depth: usize,
        name: String,
    },
    Note(String),
    FilesHead,
    File(usize),
}

impl MultisoulApp {
    pub(super) fn render_workbench_sidebar(
        &self,
        width: f32,
        close_button: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let s = &self.desktop_workspace.workbench;
        let mut pane = div()
            .id("files-sidebar")
            .debug_selector(|| "files-sidebar".to_string())
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(12.0))
            .bg(rgba(PANEL))
            .border_l_1()
            .border_color(rgba(LINE))
            .text_color(rgba(TEXT_SECONDARY));
        let mut top = div().flex().items_center().gap(px(6.0));
        if let Some(picker) = &s.project_picker {
            top = top.child(
                div().h(px(32.0)).flex_1().min_w(px(0.0)).child(
                    ui::component::select::Select::new(picker)
                        .small()
                        .menu_width(px((width - 20.0).max(160.0)))
                        .w_full()
                        .placeholder(ui::i18n::text("drawers.select_project"))
                        .disabled(s.busy),
                ),
            );
        }
        if close_button {
            top = top.child(self.sidebar_icon_button(
                "files-close-tree",
                "×",
                ui::i18n::text("drawers.back_to_files"),
                Rc::new(|app, _, cx| {
                    app.desktop_workspace.workbench.sidebar_open = false;
                    app.hide_desktop_content_browsers();
                    cx.notify();
                }),
                cx,
            ));
        }
        pane = pane.child(top);
        if let Some(input) = &s.filter {
            pane = pane.child(
                div().h(px(32.0)).flex_none().child(
                    Input::new(input)
                        .small()
                        .prefix(
                            svg()
                                .path("icons/search.svg")
                                .w(px(13.0))
                                .h(px(13.0))
                                .text_color(rgba(TEXT_MUTED)),
                        )
                        .cleanable(true),
                ),
            );
        }
        if s.ops_open {
            pane = pane.child(self.render_workbench_ops(cx));
        }
        let items = Rc::new(self.sidebar_items(cx));
        let count = items.len();
        pane.child(
            uniform_list(
                "files-tree",
                count,
                cx.processor(move |app, range: std::ops::Range<usize>, _, cx| {
                    range
                        .map(|i| app.render_sidebar_item(&items[i], cx))
                        .collect::<Vec<_>>()
                }),
            )
            .flex_1()
            .min_h(px(0.0)),
        )
    }

    fn sidebar_items(&self, cx: &Context<Self>) -> Vec<Item> {
        let s = &self.desktop_workspace.workbench;
        let query = s
            .filter
            .as_ref()
            .map(|i| i.read(cx).value().to_string())
            .unwrap_or_default();
        let allow = s.turn_paths.clone();
        let paths = s
            .git
            .as_ref()
            .map(|g| g.changes.iter().map(|c| c.path.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        let total = paths
            .iter()
            .filter(|path| {
                allow
                    .as_ref()
                    .is_none_or(|allow| allow.iter().any(|item| item == *path))
            })
            .count();
        let mut items = vec![Item::ChangesHead { count: total }];
        if !s.changes_section_closed {
            let rows = change_rows(&paths, &s.changes_dirs_closed, &query, allow.as_deref());
            if rows.is_empty() {
                items.push(Item::Note(match (&s.git, &s.git_error) {
                    (None, Some(e)) => e.to_string(),
                    (None, None) => ui::i18n::text("drawers.loading_git").into(),
                    _ if total > 0 => ui::i18n::text("drawers.no_matching_changes").into(),
                    _ if s.turn_paths.is_some() => ui::i18n::text("drawers.no_turn_changes").into(),
                    _ => ui::i18n::text("drawers.no_changes").into(),
                }));
            }
            items.extend(rows.into_iter().map(|row| match row {
                ChangeRow::Dir {
                    path,
                    label,
                    depth,
                    open,
                } => Item::ChangeDir {
                    path,
                    label,
                    depth,
                    open,
                },
                ChangeRow::File { index, depth, name } => Item::Change { index, depth, name },
            }));
        }
        items.push(Item::FilesHead);
        if !s.files_section_closed {
            if s.rows.is_empty() {
                items.push(Item::Note(if s.project.is_none() {
                    s.error
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| ui::i18n::text("drawers.loading_directory").into())
                } else if query.trim().is_empty() {
                    ui::i18n::text("drawers.empty_directory").into()
                } else {
                    ui::i18n::text("drawers.no_matching_files").into()
                }));
            }
            items.extend((0..s.rows.len()).map(Item::File));
        }
        items
    }

    fn render_sidebar_item(&self, item: &Item, cx: &mut Context<Self>) -> Stateful<Div> {
        let s = &self.desktop_workspace.workbench;
        match item {
            Item::ChangesHead { count } => {
                let mut head = self.section_head(
                    "files-section-changes",
                    ui::i18n::text("drawers.changed_files"),
                    !s.changes_section_closed,
                    false,
                    Rc::new(|app, _, cx| {
                        app.desktop_workspace.workbench.changes_section_closed ^= true;
                        cx.notify();
                    }),
                    cx,
                );
                if *count > 0 {
                    head = head.child(
                        div()
                            .ml_auto()
                            .w(px(5.0))
                            .h(px(5.0))
                            .rounded_full()
                            .bg(rgba(0x4a86e8ff)),
                    );
                }
                head.child(div().when(*count == 0, |d| d.ml_auto()).child(
                    self.sidebar_icon_button(
                        "files-ops-toggle",
                        "⋯",
                        ui::i18n::text("drawers.git_options"),
                        Rc::new(|app, _, cx| {
                            app.desktop_workspace.workbench.ops_open ^= true;
                            cx.notify();
                        }),
                        cx,
                    ),
                ))
            }
            Item::FilesHead => self.section_head(
                "files-section-all",
                ui::i18n::text("drawers.all_files"),
                !s.files_section_closed,
                true,
                Rc::new(|app, _, cx| {
                    app.desktop_workspace.workbench.files_section_closed ^= true;
                    cx.notify();
                }),
                cx,
            ),
            Item::Note(text) => div()
                .id(ElementId::Name(format!("files-note-{text}").into()))
                .h(px(ROW_HEIGHT))
                .flex()
                .items_center()
                .px(px(24.0))
                .text_size(px(11.0))
                .text_color(rgba(TEXT_MUTED))
                .child(text.clone()),
            Item::ChangeDir {
                path,
                label,
                depth,
                open,
            } => {
                let toggle_path = path.clone();
                let toggle: Action = Rc::new(move |app, _, cx| {
                    // Searching forces every ancestor open, so toggling would be a dead click.
                    if !app.filter_is_empty(cx) {
                        return;
                    }
                    let closed = &mut app.desktop_workspace.workbench.changes_dirs_closed;
                    if !closed.remove(&toggle_path) {
                        closed.insert(toggle_path.clone());
                    }
                    cx.notify();
                });
                self.tree_row(
                    ElementId::Name(format!("files-change-dir-{path}").into()),
                    *depth,
                    false,
                    Some((*open, toggle.clone())),
                    toggle,
                    cx,
                )
                .child(chevron(*open))
                .child(name_label(label, TEXT_SECONDARY, path))
            }
            Item::Change { index, depth, name } => self.render_change_row(*index, *depth, name, cx),
            Item::File(index) => self.render_file_row(*index, cx),
        }
    }

    fn section_head(
        &self,
        id: &'static str,
        title: &'static str,
        open: bool,
        separated: bool,
        toggle: Action,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.tree_row(
            ElementId::Name(id.into()),
            0,
            false,
            Some((open, toggle.clone())),
            toggle,
            cx,
        )
        .debug_selector(move || id.to_string())
        .when(separated, |row| {
            row.mt(px(6.0)).border_t_1().border_color(rgba(0xffffff12))
        })
        .text_color(rgba(TEXT_SECONDARY))
        .child(chevron(open))
        .child(name_label(title, TEXT_SECONDARY, title))
    }

    fn render_file_row(&self, index: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let s = &self.desktop_workspace.workbench;
        let row = &s.rows[index];
        let path = row.path.clone();
        let directory = row.directory;
        let expanded = s.saved.expanded.contains(&path) || !self.filter_is_empty(cx);
        let selected = !directory && s.saved.selected.as_ref() == Some(&path);
        let toggle_path = path.clone();
        let activate: Action = Rc::new(move |app, _, cx| {
            if directory {
                if !app.filter_is_empty(cx) {
                    return;
                }
                let expanded = &mut app.desktop_workspace.workbench.saved.expanded;
                if !expanded.remove(&toggle_path) {
                    expanded.insert(toggle_path.clone());
                }
                app.refresh_file_workbench(cx);
            } else {
                app.select_workbench_path(toggle_path.clone(), false, false, cx);
            }
        });
        let base = self.tree_row(
            ElementId::Name(format!("files-path-{path}").into()),
            row.depth,
            selected,
            directory.then(|| (expanded, activate.clone())),
            activate,
            cx,
        );
        if directory {
            base.child(chevron(expanded))
        } else {
            base.child(div().w(px(11.0)).flex_none())
                .child(glyph_element(glyph_for(&row.label)))
        }
        .child(name_label(&row.label, TEXT_SECONDARY, &path))
    }

    fn filter_is_empty(&self, cx: &Context<Self>) -> bool {
        self.desktop_workspace
            .workbench
            .filter
            .as_ref()
            .is_none_or(|i| i.read(cx).value().trim().is_empty())
    }

    fn render_change_row(
        &self,
        index: usize,
        depth: usize,
        name: &str,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let s = &self.desktop_workspace.workbench;
        let Some(change) = s.git.as_ref().and_then(|g| g.changes.get(index)) else {
            return div().id(ElementId::Name(format!("files-change-gone-{index}").into()));
        };
        let path = change.path.clone();
        let staged = row_is_staged(change.staged, change.unstaged);
        let local = s.saved.comparison == Comparison::Local;
        let selected = s.saved.selected.as_ref() == Some(&path) && s.diff.is_some();
        let disabled = s.busy || !s.git.as_ref().is_some_and(|g| g.writable);
        let open_path = path.clone();
        let open: Action = Rc::new(move |app, _, cx| {
            app.select_workbench_path(open_path.clone(), true, staged, cx)
        });
        let mut row = self
            .tree_row(
                ElementId::Name(format!("files-diff-{index}").into()),
                depth,
                selected,
                None,
                open,
                cx,
            )
            .child(div().w(px(11.0)).flex_none())
            .child(glyph_element(glyph_for(name)))
            .child(name_label(name, GREEN, &path));
        if !local {
            return row.child(
                div()
                    .font_family("Menlo")
                    .text_size(px(10.0))
                    .text_color(rgba(GREEN))
                    .child(change_status(change.status)),
            );
        }
        let stage_path = path.clone();
        let tip = if staged {
            ui::i18n::text("drawers.unstage")
        } else {
            ui::i18n::text("drawers.stage")
        };
        let key_path = path.clone();
        let stage = move |app: &mut MultisoulApp, path: &str, cx: &mut Context<MultisoulApp>| {
            app.workbench_git_action(
                if staged {
                    GitAction::Unstage(vec![path.to_string()])
                } else {
                    GitAction::Stage(vec![path.to_string()])
                },
                cx,
            )
        };
        let key_stage = stage.clone();
        let mut plus = div()
            .id(ElementId::Name(format!("files-stage-{index}").into()))
            .debug_selector(move || format!("files-stage-{index}"))
            .w(px(15.0))
            .h(px(15.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .border_1()
            .border_color(rgba(if disabled { GUTTER } else { GREEN }))
            .text_size(px(11.0))
            .line_height(px(11.0))
            .text_color(rgba(if disabled { GUTTER } else { GREEN }))
            .child(if staged { "−" } else { "+" });
        if !disabled {
            plus = plus
                .cursor_pointer()
                .tab_index(0)
                .tooltip(tooltip_view(tip))
                .hover(|s| s.bg(rgba(0x4cd37a33)))
                .active(|s| s.bg(rgba(0x4cd37a55)))
                .focus(|s| s.border_color(rgba(FOCUS)))
                .on_click(cx.listener(move |app, _, _, cx| {
                    cx.stop_propagation();
                    stage(app, &stage_path, cx);
                }))
                .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
                    if activates(event) {
                        cx.stop_propagation();
                        key_stage(app, &key_path, cx);
                    }
                }));
        }
        row = row.child(plus);
        row
    }
}
