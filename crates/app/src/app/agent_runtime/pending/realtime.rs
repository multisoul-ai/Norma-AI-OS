use super::*;
use gpui::Context;
use sha2::{Digest, Sha256};
use storage::{MobileTaskBaselineStore, NewMobileCommand, TaskBaseline};
#[cfg(test)]
#[path = "../../../../tests/unit/desktop_delivery_source_tests.rs"]
mod tests;

#[derive(Debug)]
pub(super) struct Delivery {
    pub(super) node: NodeId,
    task: String,
    source: NewMobileCommand,
    baseline: Option<TaskBaseline>,
    capturing: bool,
    submitted: bool,
    run: Option<String>,
}

impl PendingAgentSubmissions {
    pub(crate) fn track_realtime_delivery(
        &mut self,
        task: &agent::AgentCoordinatedTask,
        source: NewMobileCommand,
    ) {
        self.realtime
            .entry(task.task_id)
            .or_insert_with(|| Delivery {
                node: task.target_node_id,
                task: task.task.clone(),
                source,
                baseline: None,
                capturing: false,
                submitted: false,
                run: None,
            });
    }
}

impl MultisoulApp {
    /// Capture only once this task reaches its original queue's input stage, before Enter.
    pub(super) fn prepare_realtime_submission(
        &mut self,
        task: u64,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.pending_agent_submissions.realtime.contains_key(&task)
            && self
                .agent_task_coordinator
                .as_ref()
                .and_then(|c| c.tasks().into_iter().find(|t| t.task_id == task))
                .is_some_and(|t| {
                    self.runtime
                        .mobile_card_snapshots()
                        .iter()
                        .any(|c| c.node_id == t.target_node_id && c.mobile_control)
                })
        {
            let coordinated = self
                .agent_task_coordinator
                .as_ref()
                .and_then(|c| c.tasks().into_iter().find(|t| t.task_id == task));
            if let (Some(workspace), Some(route), Some(coordinated)) =
                (&self.workspace_id, &self.workspace_route_id, coordinated)
            {
                if let Some(source) =
                    desktop_delivery_source(workspace, route, &coordinated.task_ref)
                {
                    self.pending_agent_submissions
                        .track_realtime_delivery(&coordinated, source);
                }
            }
        }
        let Some(item) = self.pending_agent_submissions.realtime.get_mut(&task) else {
            return true;
        };
        if item.baseline.is_some() {
            return true;
        }
        if item.capturing {
            return false;
        }
        item.capturing = true;
        item.run = self
            .runtime
            .mobile_card_snapshots()
            .into_iter()
            .find(|card| card.node_id == item.node)
            .and_then(|card| card.run_id);
        let source = item.source.clone();
        let root = self
            .runtime
            .workspace_root()
            .map(std::path::Path::to_path_buf);
        let database = self.database_paths.app_db_path().to_owned();
        let capture_source = source.clone();
        let capture = cx.background_executor().spawn(async move {
            let source = capture_source;
            // Desktop-coordinated tasks use this same ledger and binding pipeline; reserve off GPUI.
            let reserved = storage::MobileCommandStore::new(database)
                .reserve(&source)
                .is_ok_and(|r| !matches!(r, storage::MobileCommandReservation::Conflict));
            if !reserved {
                return TaskBaseline::Unavailable {
                    reason: workspace::review::ReviewError::Unavailable,
                };
            }
            // Same 10s bounded Git capture as the existing mobile message baseline.
            match root.and_then(|root| {
                workspace::review::capture(&root, Instant::now() + Duration::from_secs(10)).ok()
            }) {
                Some(review) => TaskBaseline::Available { review },
                None => TaskBaseline::Unavailable {
                    reason: workspace::review::ReviewError::Unavailable,
                },
            }
        });
        cx.spawn(async move |this, cx| {
            let baseline = capture.await;
            let _ = this.update(cx, |app, cx| {
                if let Some(item) = app
                    .pending_agent_submissions
                    .realtime
                    .get_mut(&task)
                    .filter(|item| {
                        item.source.operation_id == source.operation_id
                            && item.source.device_id == source.device_id
                            && item.source.workspace_id == source.workspace_id
                            && item.source.route_id == source.route_id
                    })
                {
                    item.baseline = Some(baseline);
                    cx.notify();
                }
            });
        })
        .detach();
        false
    }

    pub(super) fn realtime_submission_sent(&mut self, task: u64, cx: &mut Context<Self>) {
        let Some(item) = self.pending_agent_submissions.realtime.get_mut(&task) else {
            return;
        };
        item.submitted = true;
        let node = item.node;
        let source = item.source.clone();
        self.pending_mobile_deliveries.insert(node, source.clone());
        cx.spawn(async move |this, cx| {
            // Match mobile sending: 25s for a real Provider acknowledgement, then retain unknown status.
            cx.background_executor()
                .timer(Duration::from_secs(25))
                .await;
            let _ = this.update(cx, |app, cx| {
                if app
                    .pending_agent_submissions
                    .realtime
                    .get(&task)
                    .is_some_and(|item| {
                        item.source.operation_id == source.operation_id
                            && item.source.device_id == source.device_id
                            && item.source.workspace_id == source.workspace_id
                            && item.source.route_id == source.route_id
                            && item.submitted
                    })
                {
                    app.pending_agent_submissions.realtime.remove(&task);
                    app.clear_mobile_delivery(node, &source, cx);
                    tracing::warn!(
                        task_id = task,
                        "realtime task has no confirmed Provider turn; no delivery fabricated"
                    );
                }
            });
        })
        .detach();
    }

    /// Bind pre-send bytes to the actual Provider turn, never to a guessed task or late manual input.
    pub(crate) fn bind_realtime_delivery(
        &mut self,
        event: &norma::agent_hook::AgentHookEvent,
        cx: &mut Context<Self>,
    ) {
        if event.provider_event != "UserPromptSubmit" {
            return;
        }
        let prompt = ["prompt", "message", "user_message"]
            .iter()
            .find_map(|field| event.raw_json[*field].as_str());
        let Some(key) = self.runtime.mobile_latest_turn_key(event.node_id) else {
            return;
        };
        let task = self
            .pending_agent_submissions
            .realtime
            .iter()
            .find_map(|(task, item)| {
                (item.node == event.node_id
                    && item.submitted
                    && item.run.as_ref() == Some(&key.run_id)
                    && event.turn_id.as_deref() == Some(&key.turn_id)
                    && prompt == Some(&item.task))
                .then_some(*task)
            });
        let Some(task) = task else { return };
        let Some(item) = self.pending_agent_submissions.realtime.remove(&task) else {
            return;
        };
        let Some(baseline) = item.baseline else {
            return;
        };
        let database = self.database_paths.app_db_path().to_path_buf();
        cx.spawn(async move |this, cx| {
            let source = item.source;
            let origin = source.clone();
            let turn = key.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    MobileTaskBaselineStore::new(database).record(&origin, &turn, &baseline)
                })
                .await;
            if let Err(error) = result {
                tracing::warn!(%error,task_id=task,"realtime task baseline could not be saved");
            }
            let _ = this.update(cx, |app, cx| app.watch_mobile_delivery(key, source, cx));
        })
        .detach();
    }
}

fn desktop_delivery_source(
    workspace: &str,
    route: &str,
    task_ref: &str,
) -> Option<NewMobileCommand> {
    if !workspace.starts_with("desktop:") {
        return None;
    }
    let hash = hex::encode(Sha256::digest(format!("{workspace}:{task_ref}").as_bytes()));
    Some(NewMobileCommand {
        device_id: "desktop-delivery".into(),
        operation_id: format!("delivery-{hash}"),
        workspace_id: workspace.into(),
        route_id: route.into(),
        owner_id: "desktop-delivery".into(),
        request_hash: hash,
    })
}
