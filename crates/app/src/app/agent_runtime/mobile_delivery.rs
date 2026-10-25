use crate::app::MultisoulApp;
use canvas::NodeId;
use gpui::Context;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use storage::{
    MobileTaskBaselineStore, MobileTaskDeliveryStore, MobileTurnKey, MobileTurnStore,
    NewMobileCommand, TaskBaseline, TaskDeliveryContent,
};

impl MultisoulApp {
    pub(crate) fn mobile_delivery_pending(&self, node: NodeId) -> bool {
        self.pending_mobile_deliveries
            .get(&node)
            .is_some_and(|source| self.delivery_scope(source))
    }

    fn delivery_scope(&self, source: &NewMobileCommand) -> bool {
        self.workspace_id.as_deref() == Some(&source.workspace_id)
            && self.workspace_route_id.as_deref() == Some(&source.route_id)
    }

    pub(crate) fn clear_mobile_delivery(
        &mut self,
        node: NodeId,
        source: &NewMobileCommand,
        cx: &mut Context<Self>,
    ) {
        if self
            .pending_mobile_deliveries
            .get(&node)
            .is_some_and(|pending| {
                pending.workspace_id == source.workspace_id
                    && pending.route_id == source.route_id
                    && pending.device_id == source.device_id
                    && pending.operation_id == source.operation_id
            })
        {
            self.pending_mobile_deliveries.remove(&node);
            crate::mobile::project_events::changed();
            cx.notify();
        }
    }

    /// This presentation state delays the next phone task until completion evidence is frozen.
    pub(crate) fn mobile_card_snapshots(&self) -> Vec<norma::mobile::CardSnapshot> {
        let mut cards = self.runtime.mobile_card_snapshots();
        for card in &mut cards {
            if self.mobile_delivery_pending(card.node_id)
                && matches!(card.state.as_str(), "done" | "failed" | "ended")
            {
                card.state = "collecting".into();
            }
            // 失败态的 task_summary 是失败原因，实时动作只在真正运行时填。
            if card.task_summary.is_none()
                && matches!(card.state.as_str(), "thinking" | "running_tool")
            {
                card.task_summary = self
                    .runtime
                    .agent_task_state(card.node_id)
                    .and_then(|state| state.live_label());
            }
        }
        cards
    }

    /// Weak window ownership; only memory is polled while the actual Provider works.
    pub(crate) fn watch_mobile_delivery(
        &mut self,
        key: MobileTurnKey,
        source: NewMobileCommand,
        cx: &mut Context<Self>,
    ) {
        let database = self.database_paths.app_db_path().to_path_buf();
        cx.spawn(async move |this, cx| {
            let scope = loop {
                let state = this.update(cx, |app, _| {
                    if !app.delivery_scope(&source) { return Some(Err("window_closed")); }
                    if app.runtime.mobile_latest_turn_key(key.node_id).as_ref() != Some(&key) {
                        return Some(Err("task_changed"));
                    }
                    if app.runtime.mobile_delivery_can_capture(&key) {
                        return Some(app.runtime.workspace_root().map(|p| p.to_path_buf()).ok_or("window_closed"));
                    }
                    // A native submit can arrive before the next Provider start. Do not capture later bytes.
                    let terminal = app.runtime.mobile_card_snapshots().iter().any(|c| c.node_id == key.node_id && matches!(c.state.as_str(), "done"|"failed"|"ended"));
                    terminal.then_some(Err("task_changed"))
                }).unwrap_or(Some(Err("window_closed")));
                if let Some(scope) = state { break scope; }
                //250ms = four cheap memory reads/s; no SQLite or filesystem work until the turn ends.
                cx.background_executor().timer(Duration::from_millis(250)).await;
            };
            //The audit writer may finish after the in-memory terminal state. Allow2s for that handoff.
            let audit_deadline = Instant::now() + Duration::from_secs(2);
            let durable = loop {
                let store = MobileTurnStore::for_workspace(database.clone(), source.workspace_id.clone());
                let turn = key.clone();
                let outcome = cx.background_executor().spawn(async move { store.get(&turn) }).await;
                match outcome {
                    Ok(Some(_)) => break true,
                    Err(error) => { tracing::warn!(%error, "mobile delivery outcome read failed"); break false; }
                    Ok(None) if Instant::now() >= audit_deadline => break false,
                    Ok(None) => cx.background_executor().timer(Duration::from_millis(50)).await,
                }
            };
            if durable {
                // Publish the committed turn before potentially slow diff/image capture.
                crate::mobile::project_events::changed();
                let path = database.clone(); let origin = source.clone(); let turn = key.clone();
                let mut content = cx.background_executor().spawn(async move {
                    capture(path, &origin, &turn, scope)
                }).await;
                if matches!(content, TaskDeliveryContent::Available {..}) {
                    let still_current = this.update(cx, |app, _| app.delivery_scope(&source) && app.runtime.mobile_delivery_can_capture(&key)).unwrap_or(false);
                    if !still_current { content = unavailable("task_changed"); }
                }
                let turn = key.clone(); let workspace = source.workspace_id.clone();
                let result = cx.background_executor().spawn(async move {
                    MobileTaskDeliveryStore::new(database).record(&workspace, &turn, &content)
                }).await;
                if let Err(error) = result { tracing::warn!(%error, "mobile task delivery could not be saved"); }
            } else {
                tracing::warn!(turn_id=%key.turn_id, "mobile delivery has no durable Provider outcome");
            }
            let _ = this.update(cx, |app, cx| app.clear_mobile_delivery(key.node_id, &source, cx));
        }).detach();
    }
}

fn capture(
    database: PathBuf,
    source: &NewMobileCommand,
    key: &MobileTurnKey,
    scope: Result<PathBuf, &'static str>,
) -> TaskDeliveryContent {
    let root = match scope {
        Ok(root) => root,
        Err(reason) => return unavailable(reason),
    };
    let baseline = match MobileTaskBaselineStore::new(database).get(&source.workspace_id, key) {
        Ok(Some(saved)) => saved.baseline,
        Ok(None) => return unavailable("baseline_missing"),
        Err(error) => {
            tracing::warn!(%error, "mobile delivery baseline read failed");
            return unavailable("baseline_unavailable");
        }
    };
    let TaskBaseline::Available { review } = baseline else {
        return unavailable("baseline_unavailable");
    };
    //10s matches pre-send and interactive review capture; never perform filesystem IO on GPUI.
    match workspace::review::capture_since(&root, &review, Instant::now() + Duration::from_secs(10))
    {
        Ok(review) => TaskDeliveryContent::Available { review },
        Err(reason) => unavailable(match reason {
            workspace::review::ReviewError::Expired => "capture_expired",
            workspace::review::ReviewError::TooLarge => "capture_too_large",
            workspace::review::ReviewError::UnsafePath => "capture_unsafe_path",
            workspace::review::ReviewError::Changed => "capture_changed",
            workspace::review::ReviewError::Unavailable => "capture_unavailable",
            workspace::review::ReviewError::Unsupported => "capture_unsupported",
        }),
    }
}
fn unavailable(reason: &str) -> TaskDeliveryContent {
    TaskDeliveryContent::Unavailable {
        reason: reason.into(),
    }
}

impl MultisoulApp {
    pub(crate) fn record_native_mobile_event(
        &self,
        event: &norma::agent_hook::AgentHookEvent,
        cx: &Context<Self>,
    ) {
        let Some((session, transcript, outcome)) = self.runtime.native_mobile_event(event) else {
            return;
        };
        let Some(workspace) = self.workspace_id.clone() else {
            return;
        };
        let store = MobileTurnStore::for_workspace(
            self.database_paths.app_db_path().to_path_buf(),
            workspace,
        );
        cx.background_executor()
            .spawn(async move {
                match store.record_native_event(&session, transcript.as_deref(), outcome.as_ref()) {
                    Ok(()) => crate::mobile::project_events::changed(),
                    Err(error) => {
                        tracing::warn!(%error, "native Agent evidence could not be recorded")
                    }
                }
            })
            .detach();
    }
}
