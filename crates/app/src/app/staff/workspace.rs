//! Workspace task navigation; execution always stays on the owning project entity.
use super::*;
#[cfg(test)]
#[path = "../../../tests/unit/staff_workspace_tests.rs"]
mod tests;
use gpui::{div, prelude::*, px, rgba, Div, Window};
use ui::component::{
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
    Disableable, Sizable,
};
use ui::i18n::text as t;

impl MultisoulApp {
    pub(super) fn sync_staff_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.staff.workspace_id != self.desktop_workspace.active_id {
            self.staff.workspace_id = self.desktop_workspace.active_id.clone();
            self.staff.selected = None;
            self.staff.project_filter = None;
            self.staff.target_project = None;
            self.staff.query.clear();
            self.staff.archive_view = false;
            self.staff.toast = None;
            if let Some(search) = &self.staff.search {
                search.update(cx, |input, cx| input.set_value("", window, cx));
            }
        }
        self.staff.tasks.clear();
        self.staff.task_projects.clear();
        self.staff.busy = false;
        let Some(workspace) = self.desktop_workspace.active_id.clone() else {
            return;
        };
        for area in &self.desktop_workspace.areas {
            if area.desktop_workspace_id != workspace {
                continue;
            }
            let Some(project) = self
                .desktop_workspace
                .sessions
                .get(&(workspace.clone(), area.id.clone()))
            else {
                continue;
            };
            let tasks = project.update(cx, |project, cx| {
                project.load_staff_tasks(cx);
                project.staff.tasks.values().cloned().collect::<Vec<_>>()
            });
            self.staff.busy |= project.read(cx).staff.busy;
            for task in tasks {
                self.staff.task_projects.insert(
                    task.id.clone(),
                    (area.id.clone(), area.display_name.clone()),
                );
                self.staff.tasks.insert(task.id.clone(), task);
            }
        }
        if !self
            .desktop_workspace
            .areas
            .iter()
            .any(|a| Some(&a.id) == self.staff.target_project.as_ref())
        {
            self.staff.target_project =
                self.desktop_workspace.expanded_area_id.clone().or_else(|| {
                    self.desktop_workspace
                        .areas
                        .first()
                        .map(|area| area.id.clone())
                });
        }
        if self
            .staff
            .selected
            .as_ref()
            .is_some_and(|id| !self.staff.tasks.contains_key(id))
        {
            self.staff.selected = None;
        }
        if self.staff.selected.is_none() {
            if let Some(project) = self.staff_project() {
                self.staff.selected = project
                    .read(cx)
                    .staff
                    .selected
                    .clone()
                    .filter(|id| self.staff.tasks.contains_key(id));
            }
        }
    }

    pub(crate) fn staff_area_id(&self) -> Option<&str> {
        self.staff
            .selected
            .as_ref()
            .and_then(|id| self.staff.task_projects.get(id))
            .map(|(area, _)| area.as_str())
            .or(self.staff.target_project.as_deref())
    }

    pub(crate) fn staff_project(&self) -> Option<Entity<Self>> {
        let workspace = self.desktop_workspace.active_id.as_ref()?;
        let area = self.staff_area_id()?;
        self.desktop_workspace
            .sessions
            .get(&(workspace.clone(), area.to_owned()))
            .cloned()
    }

    pub(super) fn select_staff_workspace_task(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy {
            return;
        }
        self.stash_staff_images(id.clone());
        if let Some((area, _)) = id.as_ref().and_then(|id| self.staff.task_projects.get(id)) {
            self.staff.target_project = Some(area.clone());
        }
        self.staff.sidebar_open = false;
        self.staff.selected = id.clone();
        if let Some(project) = self.staff_project() {
            project.update(cx, |project, cx| project.select_staff_task(id, window, cx));
        }
        cx.notify();
    }

    pub(super) fn select_staff_project(
        &mut self,
        area: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy {
            return;
        }
        self.staff.project_filter = area.clone();
        if let Some(area) = area {
            self.staff.target_project = Some(area.clone());
            let selected = self
                .staff
                .task_projects
                .iter()
                .find(|(_, (owner, _))| *owner == area)
                .map(|(id, _)| id.clone());
            self.select_staff_workspace_task(selected, window, cx);
        }
        cx.notify();
    }

    pub(super) fn staff_project_picker(
        &self,
        target: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let current = if target {
            self.staff.target_project.clone()
        } else {
            self.staff.project_filter.clone()
        };
        let label = self
            .desktop_workspace
            .areas
            .iter()
            .find(|area| Some(&area.id) == current.as_ref())
            .map(|area| area.display_name.clone())
            .unwrap_or_else(|| t("workspace.all_projects").to_owned());
        let areas = self.desktop_workspace.areas.clone();
        let entity = cx.weak_entity();
        Button::new(if target {
            "staff-target-project"
        } else {
            "staff-project-filter"
        })
        .debug_selector(move || {
            if target {
                "staff-target-project".into()
            } else {
                "staff-project-filter".into()
            }
        })
        .custom(self.staff_button_style(false, cx))
        .small()
        .when(target, |button| button.h(px(36.)).rounded(px(8.)))
        .when(!target, |button| {
            button.h(px(32.)).rounded_full().max_w(px(120.))
        })
        .child(
            div()
                .max_w(px(80.))
                .truncate()
                .text_size(px(13.))
                .child(label),
        )
        .dropdown_caret(true)
        .disabled(areas.is_empty() || self.staff.busy)
        .dropdown_menu(move |mut menu, _, _| {
            if !target {
                let entity = entity.clone();
                menu = menu.item(
                    PopupMenuItem::new(t("workspace.all_projects"))
                        .checked(current.is_none())
                        .on_click(move |_, window, cx| {
                            let _ = entity
                                .update(cx, |app, cx| app.select_staff_project(None, window, cx));
                        }),
                );
            }
            for area in &areas {
                let id = area.id.clone();
                let entity = entity.clone();
                menu = menu.item(
                    PopupMenuItem::new(area.display_name.clone())
                        .checked(current.as_ref() == Some(&id))
                        .on_click(move |_, window, cx| {
                            let _ = entity.update(cx, |app, cx| {
                                if target {
                                    app.staff.target_project = Some(id.clone());
                                    app.select_staff_workspace_task(None, window, cx);
                                } else {
                                    app.select_staff_project(Some(id.clone()), window, cx);
                                }
                            });
                        }),
                );
            }
            menu
        })
        .into_any_element()
    }

    pub(crate) fn staff_workspace_picker(
        &self,
        max_width: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let current = self.desktop_workspace.active_id.clone();
        let workspaces = self.desktop_workspace.workspaces.clone();
        let label = self
            .desktop_workspace
            .active()
            .map(|w| w.name.clone())
            .unwrap_or_default();
        let entity = cx.weak_entity();
        Button::new("staff-workspace-picker")
            .custom(self.staff_button_style(false, cx))
            .small()
            .h(px(30.))
            .max_w(px(max_width))
            .tooltip(label.clone())
            .rounded(px(8.))
            .child(
                div()
                    .max_w(px((max_width - 40.).max(24.)))
                    .truncate()
                    .text_size(px(12.))
                    .child(label),
            )
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for workspace in &workspaces {
                    let id = workspace.id.clone();
                    let entity = entity.clone();
                    menu = menu.item(
                        PopupMenuItem::new(workspace.name.clone())
                            .checked(current.as_ref() == Some(&id))
                            .on_click(move |_, window, cx| {
                                let _ = entity.update(cx, |app, cx| {
                                    app.open_desktop_workspace(id.clone(), window, cx)
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }

    pub(super) fn staff_conversation_area(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let target = self
            .staff_area_id()
            .and_then(|id| {
                self.desktop_workspace
                    .areas
                    .iter()
                    .find(|area| area.id == id)
            })
            .map(|area| area.display_name.clone())
            .unwrap_or_default();
        let mut center = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .rounded(px(20.))
            .bg(rgba(self.staff_theme().panel));
        if self.staff.selected.is_none() {
            center = center.child(
                div()
                    .px(px(28.))
                    .pt(px(16.))
                    .flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().text_size(px(12.)).child(t("staff.new_task_project")))
                    .child(self.staff_project_picker(true, cx)),
            );
        }
        if let Some(project) = self.staff_project() {
            center.child(project.update(cx, |app, cx| {
                app.render_staff_conversation(&target, window, cx)
            }))
        } else {
            center.child(
                div()
                    .p_6()
                    .child(if self.desktop_workspace.areas.is_empty() {
                        t("staff.no_projects")
                    } else {
                        t("staff.projects_loading")
                    }),
            )
        }
    }
}
