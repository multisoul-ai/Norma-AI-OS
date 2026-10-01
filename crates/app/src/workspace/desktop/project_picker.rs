use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, App, Context, Entity, Task, Window};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use ui::component::{
    button::{Button, ButtonVariants},
    list::{List, ListDelegate, ListEvent, ListState},
    Disableable, IndexPath, Sizable, StyledExt, WindowExt,
};
use ui::icons::{toolbar_icon, ToolbarIcon};
use workspace::WorkspaceRecord;

pub(super) struct ProjectPicker {
    records: Vec<WorkspaceRecord>,
    pub(super) matching: Vec<usize>,
    added: HashSet<PathBuf>,
    theme: ui::Theme,
}

impl ProjectPicker {
    pub(super) fn new(
        records: Vec<WorkspaceRecord>,
        added: HashSet<PathBuf>,
        theme: ui::Theme,
    ) -> Self {
        Self {
            matching: (0..records.len()).collect(),
            records,
            added,
            theme,
        }
    }

    pub(super) fn search(&mut self, query: &str) {
        let query = query.trim().to_lowercase();
        self.matching = self
            .records
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                (record.display_name.to_lowercase().contains(&query)
                    || record
                        .path
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&query))
                .then_some(index)
            })
            .collect();
    }

    pub(super) fn project_at(&self, row: usize) -> Option<&WorkspaceRecord> {
        self.matching
            .get(row)
            .and_then(|&index| self.records.get(index))
    }

    pub(super) fn is_added(&self, path: &Path) -> bool {
        self.added.contains(path)
    }
}

impl ListDelegate for ProjectPicker {
    type Item = Button;

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.matching.len()
    }

    fn perform_search(
        &mut self,
        query: &str,
        _: &mut Window,
        cx: &mut Context<ListState<Self>>,
    ) -> Task<()> {
        self.search(query);
        cx.notify();
        Task::ready(())
    }

    fn set_selected_index(
        &mut self,
        _: Option<IndexPath>,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) {
    }

    fn render_item(
        &mut self,
        ix: IndexPath,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> Option<Button> {
        let record = self.project_at(ix.row)?;
        let added = self.is_added(&record.path);
        let theme = self.theme;
        let index = self.matching[ix.row];
        Some(
            Button::new(("desktop-add-recent", index))
                .debug_selector(move || format!("desktop-add-recent-{index}"))
                .ghost()
                .w_full()
                .h(px(56.0))
                .px(px(10.0))
                .rounded(px(8.0))
                .disabled(added)
                .tooltip(record.path.display().to_string())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .w_full()
                        .gap(px(12.0))
                        .child(toolbar_icon(
                            ToolbarIcon::Workspaces,
                            theme.text_secondary,
                            20.0,
                        ))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .text_left()
                                .child(
                                    div()
                                        .text_size(px(13.0))
                                        .font_medium()
                                        .text_ellipsis()
                                        .child(record.display_name.clone()),
                                )
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgba(theme.text_secondary))
                                        .text_ellipsis()
                                        .child(workspace::compact_path(&record.path)),
                                ),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(px(11.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(if added { "Added" } else { "+" }),
                        ),
                ),
        )
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        _: &mut Context<ListState<Self>>,
    ) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .text_color(rgba(self.theme.text_secondary))
            .text_size(px(13.0))
            .child(if self.records.is_empty() {
                "No recent projects yet"
            } else {
                "No matching projects"
            })
            .child(
                div()
                    .text_size(px(11.0))
                    .child("Choose a folder or clone a repository above."),
            )
    }
}

impl MultisoulApp {
    pub(crate) fn open_desktop_add_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.desktop_workspace.busy || self.desktop_workspace.add_project_open {
            return;
        }
        self.desktop_workspace.add_project_open = true;
        let records = self.workspace_state.recent.records.clone();
        let added = self
            .desktop_workspace
            .areas
            .iter()
            .map(|area| area.root.clone())
            .collect();
        let delegate = ProjectPicker::new(records, added, self.theme);
        let list = cx.new(|cx| ListState::new(delegate, window, cx).searchable(true));
        cx.subscribe_in(&list, window, |app, list, event, window, cx| {
            if let ListEvent::Confirm(ix) = event {
                let picker = list.read(cx).delegate();
                let record = picker
                    .project_at(ix.row)
                    .filter(|record| !picker.is_added(&record.path))
                    .cloned();
                if let Some(record) = record {
                    app.close_desktop_add_project(window, cx);
                    app.add_project_to_desktop_workspace(record.path, window, cx);
                }
            }
        })
        .detach();
        let host = cx.weak_entity();
        let form = cx.new(|_| ProjectPickerForm {
            host: host.clone(),
            list: list.clone(),
        });
        let theme = self.theme;
        window.open_dialog(cx, move |dialog, window, _| {
            let host = host.clone();
            dialog
                .width(px(560.0).min(window.viewport_size().width - px(40.0)))
                .margin_top(px(24.0))
                .rounded(px(18.0))
                .bg(rgba(ui::icons::with_alpha(theme.panel_high, 0xff)))
                .border_color(rgba(theme.node_stroke))
                .p(px(24.0))
                .close_button(false)
                .overlay_closable(false)
                .on_close(move |_, _, cx| {
                    let _ = host.update(cx, |app, cx| {
                        app.desktop_workspace.add_project_open = false;
                        cx.notify();
                    });
                })
                .child(form.clone())
        });
        list.update(cx, |list, cx| list.focus(window, cx));
        cx.notify();
    }

    pub(super) fn close_desktop_add_project(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.desktop_workspace.add_project_open = false;
        window.close_dialog(cx);
        cx.notify();
    }

    fn render_project_picker(
        &mut self,
        list: &Entity<ListState<ProjectPicker>>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        let name = self
            .desktop_workspace
            .active()
            .map(|workspace| workspace.name.as_str())
            .unwrap_or("this workspace");
        let count = list.read(cx).delegate().records.len();
        // Header/actions/footer use 278px; keep the list inside the remaining window height.
        let list_height = (f32::from(window.viewport_size().height) - 340.0).clamp(100.0, 340.0);
        div()
            .id("desktop-add-project-dialog")
            .debug_selector(|| "desktop-add-project-dialog".into())
            .flex()
            .flex_col()
            .gap(px(18.0))
            .text_color(rgba(theme.text_primary))
            .child(super::dialogs::desktop_form_header(
                "Add Project",
                format!("Add to {name}"),
                theme,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(10.0))
                    .child(
                        Button::new("desktop-add-local")
                            .debug_selector(|| "desktop-add-local".into())
                            .flex_1()
                            .h(px(54.0))
                            .rounded(px(10.0))
                            .label("Choose Folder…")
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.close_desktop_add_project(window, cx);
                                app.prompt_open_project(window, cx);
                            })),
                    )
                    .child(
                        Button::new("desktop-add-clone")
                            .debug_selector(|| "desktop-add-clone".into())
                            .flex_1()
                            .h(px(54.0))
                            .rounded(px(10.0))
                            .label("Clone Repository…")
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.close_desktop_add_project(window, cx);
                                app.open_clone_project_dialog(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(11.0))
                    .text_color(rgba(theme.text_secondary))
                    .child("RECENT PROJECTS")
                    .child(format!("{count} projects")),
            )
            .child(
                div()
                    .id("desktop-add-recent-scroll")
                    .debug_selector(|| "desktop-add-recent-scroll".into())
                    .h(px(list_height))
                    .min_h(px(0.0))
                    .border_1()
                    .border_color(rgba(theme.node_stroke))
                    .rounded(px(10.0))
                    .child(
                        List::new(list)
                            .search_placeholder("Search by name or folder…")
                            .small(),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgba(theme.text_secondary))
                            .child("↑ ↓ Select    ↵ Add    Esc Close"),
                    )
                    .child(
                        Button::new("desktop-add-project-cancel")
                            .debug_selector(|| "desktop-add-project-cancel".into())
                            .label("Cancel")
                            .on_click(cx.listener(|app, _, window, cx| {
                                app.close_desktop_add_project(window, cx)
                            })),
                    ),
            )
    }
}

struct ProjectPickerForm {
    host: gpui::WeakEntity<MultisoulApp>,
    list: Entity<ListState<ProjectPicker>>,
}
impl gpui::Render for ProjectPickerForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.host.upgrade() {
            Some(host) => host.update(cx, |app, cx| {
                app.render_project_picker(&self.list, window, cx)
            }),
            None => div().id("desktop-add-project-dialog"),
        }
    }
}
