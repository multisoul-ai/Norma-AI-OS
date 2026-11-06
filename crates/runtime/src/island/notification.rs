//! Decides which pending requests need a system notification while the island is hidden.
use super::{IslandAlert, IslandAlertKind, IslandPreferences};
use std::collections::BTreeSet;

#[derive(Debug, Default)]
pub struct NotificationGate {
    notified: BTreeSet<String>,
}

impl NotificationGate {
    /// Returns alerts that have not been notified yet. Visible islands never notify and reset
    /// the record, so hiding again re-offers every request that is still pending.
    pub fn fresh(
        &mut self,
        preferences: IslandPreferences,
        alerts: &[IslandAlert],
    ) -> Vec<IslandAlert> {
        if !preferences.hidden {
            self.notified.clear();
            return Vec::new();
        }
        self.notified
            .retain(|id| alerts.iter().any(|alert| &alert.id == id));
        alerts
            .iter()
            .filter(|alert| match alert.kind {
                IslandAlertKind::Approval => preferences.approvals,
                IslandAlertKind::Question => preferences.questions,
            })
            .filter(|alert| self.notified.insert(alert.id.clone()))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/island_notification.rs"]
mod tests;
