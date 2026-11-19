//! 用户从任务里移除 Worker：胶囊菜单入口、执行中先确认、8 秒内可撤销。
//! 只改任务记录；画布卡片默认保留，执行中的或用户选「同时关闭」时才关卡片。
use super::*;
use gpui::{div, prelude::*, px, rgba, Div, Window};
use norma::protocol::StaffOp;
use ui::component::{
    button::{Button, ButtonVariants},
    StyledExt,
};
use ui::i18n::text as t;

#[derive(Clone)]
pub(super) struct RemoveRequest {
    pub task_id: String,
    pub dispatch_id: String,
    pub close_card: bool,
    /// 执行中的 Worker 已经过确认。
    pub confirmed: bool,
}

impl MultisoulApp {
    /// 提示条和确认框在桌面宿主里，任务记录与画布卡片属于拥有这个任务的项目。
    fn route_to_task_project(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
        run: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>),
    ) {
        if self.desktop_host.is_none()
            && matches!(self.screen, crate::app::AppScreen::DesktopWorkspace)
        {
            let Some(area) = self
                .staff
                .task_projects
                .get(task_id)
                .map(|(area, _)| area.clone())
            else {
                return;
            };
            self.staff.target_project = Some(area);
            if let Some(project) = self.staff_project() {
                project.update(cx, |project, cx| run(project, window, cx));
            }
            return;
        }
        run(self, window, cx);
    }

    pub(super) fn request_remove_worker(
        &mut self,
        request: RemoveRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task_id = request.task_id.clone();
        self.route_to_task_project(&task_id, window, cx, |app, window, cx| {
            app.begin_remove_worker(request, window, cx)
        });
    }

    pub(super) fn restore_staff_worker(
        &mut self,
        task_id: String,
        dispatch_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = task_id.clone();
        self.route_to_task_project(&id, window, cx, |app, window, cx| {
            let agent = app.removed_agent(&task_id, &dispatch_id);
            app.write_worker_op(
                StaffOp::RestoreWorker {
                    task_id,
                    worker: dispatch_id,
                },
                window,
                cx,
                move |app, _, cx| {
                    let text = ui::i18n::message("staff.worker_restored", &[("agent", agent)]);
                    app.show_staff_toast_with(text, None, None, cx);
                },
            );
        });
    }

    fn removed_agent(&self, task_id: &str, dispatch_id: &str) -> String {
        self.staff
            .tasks
            .get(task_id)
            .and_then(|task| {
                task.removed_workers
                    .iter()
                    .find(|removed| removed.worker.dispatch_id == dispatch_id)
            })
            .map(|removed| removed.worker.agent.clone())
            .unwrap_or_default()
    }

    fn begin_remove_worker(
        &mut self,
        request: RemoveRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(worker) = self
            .staff
            .tasks
            .get(&request.task_id)
            .and_then(|task| task.worker_for_dispatch(&request.dispatch_id))
            .cloned()
        else {
            return;
        };
        let running = worker.outcome().is_none();
        if running && !request.confirmed {
            let ask = WorkerRemoveAsk {
                task_id: request.task_id,
                dispatch_id: request.dispatch_id,
                agent: worker.agent,
            };
            self.set_remove_ask(Some(ask), cx);
            return;
        }
        self.set_remove_ask(None, cx);
        let close = running || request.close_card;
        let (task_id, dispatch_id) = (request.task_id.clone(), request.dispatch_id.clone());
        self.write_worker_op(
            StaffOp::RemoveWorker {
                task_id: task_id.clone(),
                worker: request.dispatch_id,
            },
            window,
            cx,
            move |app, window, cx| {
                // 先写盘再关卡片：关卡片带来的收尾事件已找不到这个 Worker，会被忽略。
                if let Some(node) = worker.card_id.filter(|_| close) {
                    app.staff.clocks.remove(&node);
                    if app.runtime.document.node(node).is_some() {
                        app.remove_node_and_reflow(node, window);
                        app.mark_session_dirty();
                    }
                }
                let key = if close {
                    "staff.worker_removed_closed"
                } else {
                    "staff.worker_removed"
                };
                // 卡片已关掉时撤销不了它，所以只有保留卡片才给「撤销」。
                let undo = (!close).then_some((task_id, dispatch_id));
                let text = ui::i18n::message(key, &[("agent", worker.agent)]);
                app.show_staff_toast_with(text, None, undo, cx);
            },
        );
    }

    /// 写入任务操作；成功后在界面线程里执行 `done`，失败显示错误。
    fn write_worker_op(
        &mut self,
        op: StaffOp,
        window: &mut Window,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let work = match self.write_staff_operation(op, false, cx) {
            Ok(work) => work,
            Err(error) => {
                self.staff.error = Some(crate::app::staff::error_message(error));
                self.notify_staff(cx);
                return;
            }
        };
        cx.spawn_in(window, async move |app, cx| {
            let result = work.await;
            let _ = app.update_in(cx, |app, window, cx| {
                match result {
                    Ok(_) => done(app, window, cx),
                    Err(error) => app.staff.error = Some(crate::app::staff::error_message(error)),
                }
                app.notify_staff(cx);
            });
        })
        .detach();
    }

    /// 确认框和提示条一样放在桌面宿主里渲染。
    pub(super) fn set_remove_ask(&mut self, ask: Option<WorkerRemoveAsk>, cx: &mut Context<Self>) {
        if let Some(host) = self.desktop_host.clone() {
            let _ = host.update(cx, |host, cx| host.set_remove_ask(ask, cx));
            return;
        }
        self.staff.remove_ask = ask;
        self.notify_staff(cx);
    }

    pub(super) fn staff_worker_remove_confirm(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(ask) = self.staff.remove_ask.clone() else {
            return div();
        };
        let theme = self.staff_theme();
        let request = RemoveRequest {
            task_id: ask.task_id.clone(),
            dispatch_id: ask.dispatch_id.clone(),
            close_card: true,
            confirmed: true,
        };
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000080))
            .occlude()
            .child(
                div()
                    .id("staff-worker-remove-confirm")
                    .debug_selector(|| "staff-worker-remove-confirm".into())
                    .w(px(380.))
                    .p(px(20.))
                    .rounded(px(12.))
                    .bg(rgba(theme.panel_high))
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(15.))
                            .font_semibold()
                            .child(ui::i18n::format(
                                "staff.worker_remove_title",
                                &[("agent", ask.agent)],
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(rgba(theme.text_secondary))
                            .child(t("staff.worker_remove_body")),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.))
                            .pt(px(8.))
                            .child(
                                Button::new("staff-worker-remove-cancel")
                                    .custom(self.staff_button_style(false, cx))
                                    .label(t("common.cancel"))
                                    .on_click(
                                        cx.listener(|app, _, _, cx| app.set_remove_ask(None, cx)),
                                    ),
                            )
                            .child(
                                Button::new("staff-worker-remove-confirm-button")
                                    .debug_selector(|| "staff-worker-remove-confirm-button".into())
                                    .custom(self.staff_button_style(true, cx))
                                    .label(t("staff.worker_stop_remove"))
                                    .on_click(cx.listener(move |app, _, window, cx| {
                                        app.request_remove_worker(request.clone(), window, cx)
                                    })),
                            ),
                    ),
            )
    }
}
