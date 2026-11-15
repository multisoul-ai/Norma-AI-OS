use super::*;
use gpui::{ClickEvent, Focusable, KeyDownEvent, Window};
use norma::protocol::StaffOp;

impl MultisoulApp {
    pub(crate) fn set_staff_mode(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !enabled && self.staff.fullscreen {
            self.toggle_staff_fullscreen(window, cx);
        }
        self.staff.enabled = enabled;
        self.staff.sidebar_open = false;
        self.focus_handle.focus(window);
        if let Some(project) = self
            .staff_project()
            .or_else(|| self.expanded_desktop_project())
        {
            project.update(cx, |project, cx| {
                project.runtime.focus.exit_to_canvas();
                project.load_staff_tasks(cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn toggle_staff_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.staff.sidebar_open = false;
        // The task search may disappear here; move focus before its dispatch path is removed.
        self.focus_handle.focus(window);
        self.staff.fullscreen = !self.staff.fullscreen;
        if self.staff.fullscreen != window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    pub(crate) fn capture_staff_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.staff.enabled && self.staff.sidebar_open && event.keystroke.key == "escape" {
            self.staff.sidebar_open = false;
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if self.staff.enabled
            && self.staff.fullscreen
            && event.keystroke.key == "escape"
            && !event.keystroke.modifiers.modified()
        {
            self.toggle_staff_fullscreen(window, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn select_staff_task(
        &mut self,
        id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.desktop_host.is_none()
            && matches!(self.screen, crate::app::AppScreen::DesktopWorkspace)
        {
            self.select_staff_workspace_task(id, window, cx);
            return;
        }
        if self.staff.busy {
            return;
        }
        self.stash_staff_images(id.clone());
        if let Some(input) = &self.staff.input {
            self.staff.drafts.insert(
                self.staff.selected.clone().unwrap_or_default(),
                input.read(cx).value().to_string(),
            );
            let draft = self
                .staff
                .drafts
                .get(&id.clone().unwrap_or_default())
                .cloned()
                .unwrap_or_default();
            input.update(cx, |input, cx| input.set_value(draft, window, cx));
            input.focus_handle(cx).focus(window);
        }
        if let Some(task) = id.as_ref().and_then(|id| self.staff.tasks.get(id)) {
            self.staff
                .scroll
                .scroll_to_item(task.messages.len().saturating_sub(1));
        }
        self.staff.selected = id;
        self.staff.error = None;
        self.notify_staff(cx);
    }

    pub(super) fn edit_staff_proposal(
        &mut self,
        draft: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = &self.staff.input {
            input.update(cx, |input, cx| {
                let current = input.value();
                let combined = if current.is_empty() {
                    draft.to_owned()
                } else {
                    format!("{current}\n\n{draft}")
                };
                input.set_value(combined, window, cx);
            });
            input.focus_handle(cx).focus(window);
        }
    }

    pub(super) fn send_staff_message_click(
        &mut self,
        _: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.send_staff_message(window, cx);
    }

    pub(super) fn send_staff_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.staff.busy {
            return;
        }
        let Some(input) = self.staff.input.clone() else {
            return;
        };
        let visible = input.read(cx).value().trim().to_owned();
        let images = self.staff.images.clone();
        if visible.is_empty() && images.is_empty() {
            return;
        }
        let (text, title) = match super::images::staff_outgoing(&visible, &images) {
            Ok(message) => message,
            Err("empty") => return,
            Err(error) => {
                self.staff.error = Some(super::error_message(error.to_owned()));
                self.notify_staff(cx);
                return;
            }
        };
        let selected = self.staff.selected.clone();
        let op = match &selected {
            Some(id) => StaffOp::Message {
                task_id: id.clone(),
                text: text.clone(),
            },
            None => StaffOp::Create { title },
        };
        let work = match self.write_staff_operation(op, false, cx) {
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
            let mut result = work.await;
            if selected.is_none() {
                if let Ok(output) = &result {
                    if let Some(task) = serde_json::from_str::<Vec<Task>>(output)
                        .ok()
                        .and_then(|tasks| tasks.into_iter().next())
                    {
                        let next = app.update(cx, |app, cx| {
                            app.staff.selected = Some(task.id.clone());
                            app.write_staff_operation(
                                StaffOp::Message {
                                    task_id: task.id,
                                    text,
                                },
                                false,
                                cx,
                            )
                        });
                        result = match next {
                            Ok(Ok(work)) => work.await,
                            Ok(Err(error)) => Err(error),
                            Err(error) => Err(error.to_string()),
                        };
                    }
                }
            }
            let _ = app.update_in(cx, |app, window, cx| {
                app.staff.busy = false;
                if result.is_ok() {
                    input.update(cx, |input, cx| input.set_value(String::new(), window, cx));
                    let key = selected.unwrap_or_default();
                    app.staff.drafts.remove(&key);
                    app.staff.images.clear();
                    app.staff.image_drafts.remove(&key);
                }
                app.staff.error = result.err().map(crate::app::staff::error_message);
                app.notify_staff(cx);
            });
        })
        .detach();
        self.notify_staff(cx);
    }

    pub(super) fn locate_staff_worker(
        &mut self,
        node: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.runtime.selection.select_only(node);
        self.runtime.focus.focus_node(node);
        if let Some(host) = self.desktop_host.clone() {
            let project_id = cx.entity_id();
            let window = window.window_handle();
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, window, cx| {
                    let _ = host.update(cx, |host, cx| {
                        let area = host.desktop_workspace.sessions.iter().find_map(
                            |((workspace, area), project)| {
                                (host.desktop_workspace.active_id.as_ref() == Some(workspace)
                                    && project.entity_id() == project_id)
                                    .then(|| area.clone())
                            },
                        );
                        host.set_staff_mode(false, window, cx);
                        if let Some(area) = area {
                            host.locate_desktop_agent(&area, node, window, cx);
                        }
                    });
                });
            });
        }
        self.notify_staff(cx);
    }
}
