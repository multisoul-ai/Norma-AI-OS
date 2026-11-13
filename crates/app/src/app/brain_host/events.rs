//! Capture event identity before queueing; never rebind a delayed event to a new route.
use super::MultisoulApp;
use brain::{BrainEvent, BrainEventKind, BrainStatusKind};
use gpui::Context;
use norma::protocol::BrainScope;

impl MultisoulApp {
    pub(super) fn brain_scope(&self) -> Option<BrainScope> {
        Some(BrainScope {
            workspace_id: self.workspace_id.clone()?,
            route_id: self.workspace_route_id.clone()?,
        })
    }

    pub(in crate::app) fn push_brain(&self, kind: BrainEventKind) {
        let Some(scope) = self.brain_scope() else {
            return;
        };
        self.push_scoped_brain(BrainEvent { scope, kind });
    }

    /// Island chat entry point: the exact identity of this live host carries the request to the
    /// single shared brain. Unavailability is reported as a localized key, never dropped silently.
    pub(crate) fn push_staff_event(&self, kind: BrainEventKind) -> Result<(), &'static str> {
        self.staff_ready()?;
        self.push_brain(kind);
        Ok(())
    }

    /// Why this host cannot carry island chat to the brain right now, if it cannot.
    pub(crate) fn staff_ready(&self) -> Result<(), &'static str> {
        if self.brain_scope().is_none() {
            return Err(if self.runtime.workspace_root().is_some() {
                "staff.connection_unavailable"
            } else {
                "island.staff_no_workspace"
            });
        }
        match self.brain.settings.as_ref() {
            None => Err("island.staff_loading"),
            Some(settings) if !settings.enabled => Err("island.staff_disabled"),
            Some(_) => Ok(()),
        }
    }

    pub(super) fn push_scoped_brain(&self, event: BrainEvent) {
        if self.brain_scope().as_ref() != Some(&event.scope) {
            return;
        }
        // Worker settlement outlives the model service. Persist before handing it to Router.
        if matches!(
            &event.kind,
            BrainEventKind::AgentFinished {
                dispatch_id: Some(_),
                ..
            } | BrainEventKind::WorkerAttention { .. }
                | BrainEventKind::WorkerDelivery { .. }
        ) {
            self.staff.events.borrow_mut().push_back(event);
            return;
        }
        let event = match self.brain.client.as_ref() {
            Some(client) => match client.push(event) {
                None => return,
                Some(event) => event,
            },
            None => event,
        };
        if self
            .brain
            .settings
            .as_ref()
            .is_none_or(|settings| settings.enabled)
        {
            let mut pending = self.brain.pending_events.borrow_mut();
            pending.push_back(event);
            if pending.len() > 200 {
                pending.pop_front();
            }
        }
    }

    pub(super) fn drain_brain_status(&mut self, cx: &mut Context<Self>) {
        for status in self.brain.take_statuses() {
            if self.brain_scope().as_ref() != Some(&status.scope) {
                continue;
            }
            match status.kind {
                BrainStatusKind::TaskActivity {
                    task_id,
                    turn,
                    update,
                } => {
                    self.staff
                        .activity
                        .entry(task_id.clone())
                        .or_default()
                        .apply(turn, update);
                    if self.staff.selected.as_ref() == Some(&task_id) {
                        let remaining = f32::from(self.staff.scroll.max_offset().height)
                            + f32::from(self.staff.scroll.offset().y);
                        // Same 48px follow threshold as durable conversation messages.
                        if remaining <= 48.0 {
                            self.staff.scroll.scroll_to_bottom();
                        }
                    }
                    self.notify_staff(cx);
                }
                BrainStatusKind::TaskChanged(task) => self.receive_staff_tasks(vec![task], cx),
                BrainStatusKind::TaskError { reason } => {
                    self.staff.error = Some(crate::app::staff::error_message(reason));
                    self.notify_staff(cx);
                }
                BrainStatusKind::Ready => tracing::info!("大脑会话就绪"),
                BrainStatusKind::BackendDown { reason } => {
                    tracing::warn!(%reason, "大脑后端不可用");
                    let pending: Vec<String> = self.brain.tracked.keys().cloned().collect();
                    for id in pending {
                        self.escalate(&id, "幕僚暂时不可用", cx);
                    }
                }
                BrainStatusKind::Trace(entry) => self.brain.record_trace(entry),
                BrainStatusKind::ApprovalTurnEnded { id } => {
                    if self.brain.tracked.get(&id).is_some_and(|t| !t.escalated) {
                        self.escalate(&id, "我拿不准", cx);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_event_scope_tests.rs"]
mod tests;
