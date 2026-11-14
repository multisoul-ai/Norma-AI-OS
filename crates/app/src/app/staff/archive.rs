//! 归档入口、提示和写回。列表投影在 norma，这里只做界面和宿主路由。
use super::*;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Div, Window};
use norma::protocol::{StaffOp, Task};
use ui::component::{
    button::{Button, ButtonVariants},
    Disableable, Icon, StyledExt,
};
use ui::i18n::text as t;

pub(super) enum StaffArchiveNotice {
    Archived { title: String, active: bool },
    Undone,
    MovedBack,
}

impl MultisoulApp {
    pub(super) fn request_staff_archive(
        &mut self,
        id: String,
        archived: bool,
        notice: StaffArchiveNotice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_host.is_none()
            && matches!(self.screen, crate::app::AppScreen::DesktopWorkspace)
        {
            let Some(area) = self
                .staff
                .task_projects
                .get(&id)
                .map(|(area, _)| area.clone())
            else {
                return;
            };
            self.staff.target_project = Some(area);
            if let Some(project) = self.staff_project() {
                project.update(cx, |project, cx| {
                    project.persist_staff_archive(id, archived, notice, window, cx);
                });
            }
            return;
        }
        self.persist_staff_archive(id, archived, notice, window, cx);
    }

    fn persist_staff_archive(
        &mut self,
        id: String,
        archived: bool,
        notice: StaffArchiveNotice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy {
            return;
        }
        let work = match self.write_staff_operation(
            StaffOp::Archive {
                task_id: id.clone(),
                archived,
            },
            false,
            cx,
        ) {
            Ok(work) => work,
            Err(error) => {
                self.staff.error = Some(crate::app::staff::error_message(error));
                self.notify_staff(cx);
                return;
            }
        };
        self.staff.busy = true;
        self.staff.error = None;
        cx.spawn_in(window, async move |app, cx| {
            let result = work.await;
            let _ = app.update_in(cx, |app, window, cx| {
                app.staff.busy = false;
                match result {
                    Ok(_) => app.finish_staff_archive(id, archived, notice, window, cx),
                    Err(error) => {
                        app.staff.error = Some(crate::app::staff::error_message(error));
                        app.notify_staff(cx);
                    }
                }
            });
        })
        .detach();
        self.notify_staff(cx);
    }

    fn finish_staff_archive(
        &mut self,
        id: String,
        archived: bool,
        notice: StaffArchiveNotice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, undo) = match &notice {
            StaffArchiveNotice::Archived { title, active } => {
                // 以提交时的任务状态为准。问题会在两分钟后超时，点击瞬间读到的状态可能已经变掉。
                let active = self
                    .staff
                    .tasks
                    .get(&id)
                    .map(|task| task.archive_note_active())
                    .unwrap_or(*active);
                (
                    ui::i18n::message(
                        if active {
                            "staff.archive_toast_active"
                        } else {
                            "staff.archive_toast"
                        },
                        &[("title", title.clone())],
                    ),
                    Some(id.clone()),
                )
            }
            StaffArchiveNotice::Undone => (ui::i18n::message("staff.archive_undone", &[]), None),
            StaffArchiveNotice::MovedBack => {
                (ui::i18n::message("staff.unarchived_toast", &[]), None)
            }
        };
        if let Some(host) = self.desktop_host.clone() {
            let _ = host.update(cx, |host, cx| {
                if let Some(task) = host.staff.tasks.get_mut(&id) {
                    task.archived = archived;
                }
                host.show_staff_toast(text, undo, cx);
                host.focus_after_archive(&id, &notice, window, cx);
            });
        } else {
            if let Some(task) = self.staff.tasks.get_mut(&id) {
                task.archived = archived;
            }
            self.show_staff_toast(text, undo, cx);
            self.focus_after_archive(&id, &notice, window, cx);
        }
        self.notify_staff(cx);
    }

    fn focus_after_archive(
        &mut self,
        id: &str,
        notice: &StaffArchiveNotice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.staff.busy = false;
        let next = match notice {
            StaffArchiveNotice::Archived { .. } | StaffArchiveNotice::MovedBack
                if self.staff.selected.as_deref() == Some(id) =>
            {
                self.staff_visible_ids().into_iter().find(|next| next != id)
            }
            StaffArchiveNotice::Undone => {
                self.staff.archive_view = false;
                Some(id.to_owned())
            }
            StaffArchiveNotice::Archived { .. } | StaffArchiveNotice::MovedBack => return,
        };
        if let Some(area) = next
            .as_ref()
            .and_then(|next| self.staff.task_projects.get(next))
            .map(|(area, _)| area.clone())
        {
            self.staff.target_project = Some(area);
        }
        self.staff.selected = next.clone();
        let Some(project) = self.staff_project() else {
            return;
        };
        // 当前调用栈还在项目实体的 update 里，延后一拍再改它的选中任务。
        cx.spawn_in(window, async move |app, cx| {
            let _ = app.update_in(cx, |_app, window, cx| {
                project.update(cx, |project, cx| {
                    project.staff.busy = false;
                    project.select_staff_task(next, window, cx);
                });
            });
        })
        .detach();
    }

    pub(super) fn staff_visible_ids(&self) -> Vec<String> {
        let tasks: Vec<_> = self.staff.tasks.values().cloned().collect();
        let project_filter = self.staff.project_filter.clone();
        let task_projects = self.staff.task_projects.clone();
        norma::protocol::staff_visible_tasks(
            &tasks,
            self.staff.archive_view,
            self.staff.filter,
            &self.staff.query,
            |task| {
                project_filter.as_ref().is_none_or(|area| {
                    task_projects
                        .get(&task.id)
                        .is_some_and(|(owner, _)| owner == area)
                })
            },
        )
        .into_iter()
        .map(|task| task.id.clone())
        .collect()
    }

    pub(super) fn show_staff_toast(
        &mut self,
        text: ui::i18n::LocalizedText,
        undo: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if let Some(host) = self.desktop_host.clone() {
            let _ = host.update(cx, |host, cx| host.show_staff_toast(text, undo, cx));
            return;
        }
        self.staff.toast_generation = self.staff.toast_generation.wrapping_add(1);
        let generation = self.staff.toast_generation;
        let seconds = if undo.is_some() { 6 } else { 3 };
        self.staff.toast = Some(StaffToast {
            text,
            undo_task: undo,
            generation,
        });
        self.notify_staff(cx);
        cx.spawn(async move |app, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(seconds))
                .await;
            let _ = app.update(cx, |app, cx| {
                if app
                    .staff
                    .toast
                    .as_ref()
                    .is_some_and(|toast| toast.generation == generation)
                {
                    app.staff.toast = None;
                    app.notify_staff(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn set_staff_archive_view(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.staff.archive_view = open;
        self.staff.filter = None;
        self.staff.query.clear();
        if let Some(search) = &self.staff.search {
            search.update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.notify_staff(cx);
    }

    pub(super) fn staff_archive_toast(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(toast) = self.staff.toast.clone() else {
            return div();
        };
        let undo = toast.undo_task.clone();
        let (background, foreground, action) = if self.theme.is_light() {
            (0x313033ff, 0xf4eff4ff, 0xd3e3fdff)
        } else {
            (0xe3e3e3ff, 0x1f1f1fff, 0x0842a0ff)
        };
        div()
            .absolute()
            .bottom(px(24.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("staff-archive-toast")
                    .debug_selector(|| "staff-archive-toast".into())
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .max_w(px(640.))
                    .px(px(16.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .bg(rgba(background))
                    .text_color(rgba(foreground))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_size(px(13.))
                            .whitespace_normal()
                            .child(toast.text),
                    )
                    .when(undo.is_some(), |row| {
                        let undo_id = undo.clone();
                        row.child(
                            Button::new("staff-archive-undo")
                                .debug_selector(|| "staff-archive-undo".into())
                                .custom(
                                    ui::component::button::ButtonCustomVariant::new(cx)
                                        .color(rgba(0x00000000).into())
                                        .foreground(rgba(action).into())
                                        .hover(rgba(ui::icons::with_alpha(action, 36)).into())
                                        .active(rgba(ui::icons::with_alpha(action, 56)).into()),
                                )
                                .h(px(32.))
                                .px(px(12.))
                                .rounded(px(16.))
                                .text_size(px(13.))
                                .label(t("staff.undo"))
                                .on_click(cx.listener(move |app, _: &ClickEvent, window, cx| {
                                    let Some(id) = undo_id.clone() else {
                                        return;
                                    };
                                    app.staff.toast = None;
                                    app.request_staff_archive(
                                        id,
                                        false,
                                        StaffArchiveNotice::Undone,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                    }),
            )
    }

    pub(super) fn staff_archive_header(&self, task: &Task, cx: &mut Context<Self>) -> Div {
        let archived = task.archived;
        let id = task.id.clone();
        let title = task.title.clone();
        let on_container = self.staff_tonal_colors().1;
        div().flex_none().child(
            Button::new(if archived {
                "staff-unarchive"
            } else {
                "staff-archive"
            })
            .debug_selector(move || {
                if archived {
                    "staff-unarchive".into()
                } else {
                    "staff-archive".into()
                }
            })
            .custom(self.staff_tonal_style(cx))
            .h(px(36.))
            .px(px(14.))
            .rounded(px(18.))
            .icon(Icon::empty().path(if archived {
                "icons/unarchive.svg"
            } else {
                "icons/archive.svg"
            }))
            .disabled(self.staff.busy)
            .child(
                div()
                    .text_size(px(13.))
                    .font_medium()
                    .text_color(rgba(on_container))
                    .child(t(if archived {
                        "staff.unarchive"
                    } else {
                        "staff.archive"
                    })),
            )
            .on_click(cx.listener(move |app, _, window, cx| {
                if archived {
                    app.request_staff_archive(
                        id.clone(),
                        false,
                        StaffArchiveNotice::MovedBack,
                        window,
                        cx,
                    );
                } else {
                    let task = app.staff.tasks.get(&id);
                    let active = task.is_some_and(|task| task.archive_note_active());
                    let title = task
                        .map(|task| task.title.clone())
                        .unwrap_or_else(|| title.clone());
                    app.request_staff_archive(
                        id.clone(),
                        true,
                        StaffArchiveNotice::Archived { title, active },
                        window,
                        cx,
                    );
                }
            })),
        )
    }

    pub(super) fn staff_archived_banner(&self) -> Div {
        let theme = self.staff_theme();
        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(20.))
            .py(px(10.))
            .bg(rgba(theme.panel_high))
            .text_size(px(13.))
            .text_color(rgba(theme.text_secondary))
            .child(t("staff.archived_note"))
    }

    pub(super) fn staff_archive_bar(&mut self, count: usize, cx: &mut Context<Self>) -> Div {
        let theme = self.staff_theme();
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(8.))
            .pb(px(12.))
            .child(
                Button::new("staff-archive-back")
                    .debug_selector(|| "staff-archive-back".into())
                    .custom(self.staff_button_style(false, cx))
                    .h(px(32.))
                    .px(px(10.))
                    .rounded(px(16.))
                    .text_size(px(13.))
                    .label(t("staff.back_to_tasks"))
                    .disabled(self.staff.busy)
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.set_staff_archive_view(false, window, cx);
                    })),
            )
            .child(
                div()
                    .text_size(px(14.))
                    .font_medium()
                    .child(t("staff.archived")),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(rgba(theme.text_secondary))
                    .child(count.to_string()),
            )
    }

    pub(super) fn staff_archive_link(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = self.staff_theme();
        let count = self
            .staff
            .tasks
            .values()
            .filter(|task| task.archived)
            .count();
        div().px(px(8.)).pb(px(12.)).flex_none().child(
            Button::new("staff-archived-link")
                .debug_selector(|| "staff-archived-link".into())
                .custom(self.staff_button_style(false, cx))
                .w_full()
                .h(px(44.))
                .px(px(14.))
                .rounded(px(22.))
                .justify_start()
                .gap(px(12.))
                .icon(Icon::empty().path("icons/archive.svg"))
                .disabled(self.staff.busy)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.))
                        .text_color(rgba(theme.text_secondary))
                        .child(t("staff.archived")),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_medium()
                        .child(count.to_string()),
                )
                .child(
                    div()
                        .text_size(px(16.))
                        .text_color(rgba(theme.text_tertiary))
                        .child("›"),
                )
                .on_click(cx.listener(|app, _, window, cx| {
                    app.set_staff_archive_view(true, window, cx);
                })),
        )
    }
}
