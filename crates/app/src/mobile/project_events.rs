use std::{sync::Mutex, time::Duration};
use tokio::sync::Notify;

// ponytail: one process-wide signal may wake watches for another App database.
// It carries no data; every read still resolves its own database and permission scope.
static CHANGES: Changes = Changes::new();
pub(crate) fn changed() {
    CHANGES.changed();
}
pub(crate) fn revision() -> u64 {
    CHANGES.revision()
}
pub(super) async fn wait(revision: u64, timeout: Duration) {
    CHANGES.wait(revision, timeout).await;
}

struct Changes {
    revision: Mutex<u64>,
    changed: Notify,
}
impl Changes {
    const fn new() -> Self {
        Self {
            revision: Mutex::new(1),
            changed: Notify::const_new(),
        }
    }
    fn revision(&self) -> u64 {
        *self
            .revision
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
    fn changed(&self) {
        let mut revision = self
            .revision
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        *revision = revision.wrapping_add(1);
        drop(revision);
        self.changed.notify_waiters();
    }
    async fn wait(&self, revision: u64, timeout: Duration) {
        // Register before checking the revision: an intervening change cannot be lost.
        // Notify performs no network/file IO on the foreground publisher.
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.revision() != revision
                || tokio::time::timeout_at(deadline, notified).await.is_err()
            {
                return;
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/mobile_project_events_tests.rs"]
mod tests;
