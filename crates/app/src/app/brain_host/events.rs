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

    pub(super) fn push_scoped_brain(&self, event: BrainEvent) {
        if self.brain_scope().as_ref() != Some(&event.scope) {
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
