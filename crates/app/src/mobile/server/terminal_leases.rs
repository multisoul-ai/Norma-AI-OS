use super::Shared;
use runtime::TerminalInputLease;
use std::sync::{atomic::Ordering, Arc, Weak};

impl Shared {
    /// Caller holds access so registration and permission changes cannot pass each other.
    pub(in crate::mobile) fn terminal_lease(
        &self,
        device: &str,
        key: storage::MobileTurnKey,
    ) -> Result<Arc<TerminalInputLease>, &'static str> {
        let mut leases = self.terminal_leases.lock().map_err(|_| "unavailable")?;
        leases.retain(|_, values| {
            values.retain(|lease| lease.upgrade().is_some_and(|lease| !lease.is_revoked()));
            !values.is_empty()
        });
        if self.stopped.load(Ordering::Acquire) {
            return Err("unavailable");
        }
        // Same bound as the relay's eight live peers; revoked Runtime slots do not consume capacity.
        if leases.values().map(Vec::len).sum::<usize>() >= 8 {
            return Err("busy");
        }
        let lease = TerminalInputLease::new(key)?;
        leases
            .entry(device.into())
            .or_default()
            .push(Arc::downgrade(&lease));
        Ok(lease)
    }

    pub(super) fn revoke_terminal_leases(&self, device: Option<&str>) {
        let mut leases = self
            .terminal_leases
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        leases.retain(|id, values| {
            if device.is_some_and(|device| device != id) {
                return true;
            }
            for lease in values.iter().filter_map(Weak::upgrade) {
                lease.revoke();
            }
            false
        });
    }
}
