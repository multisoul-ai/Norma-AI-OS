//! Late traces remain historical; retired service status cannot affect live approvals.
use super::{activity_sink::Pending, BrainHostState};
use brain::{BrainStatus, BrainStatusKind};

impl BrainHostState {
    pub(super) fn take_statuses(&mut self) -> Vec<BrainStatus> {
        let mut active = Vec::new();
        while let Some(entry) = self.client.as_ref().and_then(|client| client.try_recv()) {
            if entry.retired {
                if let BrainStatusKind::Trace(trace) = entry.status.kind {
                    self.activity_outbox.push(Pending::Trace(trace));
                }
            } else {
                active.push(entry.status);
            }
        }
        active
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/brain_host_lifecycle_tests.rs"]
mod tests;
