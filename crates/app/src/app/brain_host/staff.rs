//! Authenticated task commands run disk IO off the GPUI thread.
use super::*;
use norma::protocol::{StaffOp, Task};
use std::sync::Arc;

mod dispatch;
mod events;
mod input;

impl MultisoulApp {
    pub(crate) fn staff_store(&self) -> Result<Arc<brain::staff::TaskStore>, String> {
        self.staff_ready().map_err(str::to_owned)?;
        self.brain
            .client
            .as_ref()
            .and_then(|client| client.tasks())
            .ok_or_else(|| "island.staff_loading".to_owned())
    }

    pub(crate) fn start_staff_command(
        &mut self,
        token: &str,
        op: StaffOp,
        cx: &mut Context<Self>,
    ) -> Result<gpui::Task<Result<String, String>>, String> {
        if !self.brain.token_matches(token) {
            return Err("staff.invalid_source".to_owned());
        }
        if let StaffOp::Adopt {
            task_id,
            proposal_id,
        } = op
        {
            return self.dispatch_staff_worker(task_id, proposal_id, token.to_owned(), cx);
        }
        if let StaffOp::Propose { agent, .. } = &op {
            if !self
                .agent_roster_for_provider()
                .iter()
                .any(|entry| entry.name == *agent && entry.enabled)
            {
                return Err("staff.agent_unavailable".to_owned());
            }
        }
        self.write_staff_operation(op, false, cx)
    }

    pub(crate) fn write_staff_operation(
        &mut self,
        op: StaffOp,
        manual: bool,
        cx: &mut Context<Self>,
    ) -> Result<gpui::Task<Result<String, String>>, String> {
        let store = self.staff_store()?;
        let scope = self
            .brain_scope()
            .ok_or_else(|| "island.staff_no_workspace".to_owned())?;
        let wake = matches!(op, StaffOp::Message { .. })
            .then(|| op.task_id().unwrap_or_default().to_owned());
        let workspace = scope.workspace_id.clone();
        let work = cx.background_executor().spawn(async move {
            if manual {
                if let StaffOp::Phase { task_id, phase } = op {
                    return store
                        .update(&workspace, &task_id, |task| {
                            task.set_phase(phase, true);
                            Ok(())
                        })
                        .map(|task| vec![task]);
                }
            }
            store.execute(&workspace, op)
        });
        Ok(cx.spawn(async move |app, cx| {
            let tasks = work.await.map_err(|error| error.to_string())?;
            let output = serde_json::to_string(&tasks).map_err(|error| error.to_string())?;
            app.update(cx, |app, cx| {
                if app.brain_scope().as_ref() != Some(&scope) {
                    return;
                }
                app.receive_staff_tasks(tasks, cx);
                if let Some(task_id) = wake {
                    app.push_brain(BrainEventKind::TaskRequest { task_id });
                }
            })
            .map_err(|_| "staff.invalid_source".to_owned())?;
            Ok(output)
        }))
    }

    pub(crate) fn load_staff_tasks(&mut self, cx: &mut Context<Self>) {
        if self.staff.loaded || self.staff.loading || self.staff.error.is_some() {
            return;
        }
        match self.write_staff_operation(StaffOp::List, false, cx) {
            Ok(work) => {
                self.staff.loading = true;
                cx.spawn(async move |app, cx| {
                    let result = work.await;
                    let _ = app.update(cx, |app, cx| {
                        app.staff.loading = false;
                        app.staff.error = result.err().map(crate::app::staff::error_message);
                        app.notify_staff(cx);
                    });
                })
                .detach();
            }
            Err(error) => self.staff.error = Some(crate::app::staff::error_message(error)),
        }
    }

    pub(crate) fn staff_user_operation(
        &mut self,
        op: StaffOp,
        manual: bool,
        cx: &mut Context<Self>,
    ) {
        if self.staff.busy {
            return;
        }
        let creating = matches!(op, StaffOp::Create { .. });
        match self.write_staff_operation(op, manual, cx) {
            Ok(work) => {
                self.staff.busy = true;
                self.staff.error = None;
                cx.spawn(async move |app, cx| {
                    let result = work.await;
                    let _ = app.update(cx, |app, cx| {
                        app.staff.busy = false;
                        if creating {
                            if let Ok(output) = &result {
                                if let Ok(tasks) = serde_json::from_str::<Vec<Task>>(output) {
                                    app.staff.selected = tasks.first().map(|task| task.id.clone());
                                }
                            }
                        }
                        app.staff.error = result.err().map(crate::app::staff::error_message);
                        app.notify_staff(cx);
                    });
                })
                .detach();
            }
            Err(error) => self.staff.error = Some(crate::app::staff::error_message(error)),
        }
        self.notify_staff(cx);
    }
}
