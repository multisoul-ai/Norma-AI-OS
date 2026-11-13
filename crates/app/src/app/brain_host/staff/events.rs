//! Task settlement must survive disabling/restarting the model service.
use super::*;
use brain::staff::{TaskEventResult, TaskStore};

impl MultisoulApp {
    pub(crate) fn flush_staff_events(&mut self, cx: &mut Context<Self>) {
        self.flush_staff_notifications();
        if self.staff.saving_event || self.staff.events.borrow().is_empty() {
            return;
        }
        let Some(root) = config::state_dir() else {
            return;
        };
        self.save_staff_events(TaskStore::shared(root), cx);
    }

    fn flush_staff_notifications(&self) {
        if !self
            .brain
            .settings
            .as_ref()
            .is_some_and(|settings| settings.enabled)
        {
            self.staff.recorded.borrow_mut().clear();
            return;
        }
        let Some(client) = &self.brain.client else {
            return;
        };
        let mut pending = self.staff.recorded.borrow_mut();
        while let Some((event, result)) = pending.pop_front() {
            if self.brain_scope().as_ref() != Some(&event.scope) {
                continue;
            }
            if let Some(undelivered) = client.push_recorded(event, result) {
                pending.push_front(undelivered);
                break;
            }
        }
    }

    pub(in crate::app::brain_host) fn save_staff_events(
        &mut self,
        store: Arc<TaskStore>,
        cx: &mut Context<Self>,
    ) {
        if self.staff.saving_event || self.staff.events.borrow().is_empty() {
            return;
        }
        let event = self
            .staff
            .events
            .borrow_mut()
            .pop_front()
            .expect("queued event");
        self.staff.saving_event = true;
        cx.spawn(async move |app, cx| {
            loop {
                let writer = store.clone();
                let next = event.clone();
                let saved = cx
                    .background_executor()
                    .spawn(async move {
                        let result = writer.record_event(&next.scope.workspace_id, &next.kind)?;
                        let task = match &result {
                            TaskEventResult::Recorded { task_id, .. } => {
                                Some(writer.get(&next.scope.workspace_id, task_id)?)
                            }
                            _ => None,
                        };
                        Ok::<_, std::io::Error>((result, task))
                    })
                    .await;
                match saved {
                    Ok((recorded, task)) => {
                        let _ = app.update(cx, |app, cx| {
                            app.staff.saving_event = false;
                            if app.brain_scope().as_ref() == Some(&event.scope) {
                                if let Some(task) = task {
                                    app.receive_staff_tasks(vec![task], cx);
                                }
                                if app
                                    .brain
                                    .settings
                                    .as_ref()
                                    .is_some_and(|settings| settings.enabled)
                                {
                                    app.staff
                                        .recorded
                                        .borrow_mut()
                                        .push_back((event.clone(), recorded));
                                    app.flush_staff_notifications();
                                }
                            }
                            app.save_staff_events(store.clone(), cx);
                        });
                        break;
                    }
                    Err(error) => {
                        let _ = app.update(cx, |app, cx| {
                            app.staff.error =
                                Some(crate::app::staff::error_message(error.to_string()));
                            app.notify_staff(cx);
                        });
                        // Match the service's 1-second persistence retry; do not rerun the Worker.
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                    }
                }
            }
        })
        .detach();
    }
}
