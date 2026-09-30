use super::{CommandLease, Shared};
use std::sync::{atomic::Ordering, Arc, Weak};

impl Shared {
    pub(in crate::mobile) fn command_lease(
        &self,
        device: &str,
        operation: &str,
    ) -> Result<Arc<CommandLease>, &'static str> {
        let mut leases = self.command_leases.lock().map_err(|_| "unavailable")?;
        leases.retain(|_, values| {
            values.retain(|lease| lease.strong_count() > 0);
            !values.is_empty()
        });
        if self.stopped.load(Ordering::Acquire) {
            return Err("unavailable");
        }
        if leases.values().map(Vec::len).sum::<usize>() >= 8 {
            return Err("busy");
        }
        let lease = Arc::new(CommandLease::new(operation));
        leases
            .entry(device.into())
            .or_default()
            .push(Arc::downgrade(&lease));
        Ok(lease)
    }

    pub(in crate::mobile) fn command_pending(&self, device: &str, operation: &str) -> bool {
        self.command_leases
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(device)
            .is_some_and(|leases| {
                leases.iter().filter_map(Weak::upgrade).any(|lease| {
                    lease.operation_id == operation && !lease.finished.load(Ordering::Acquire)
                })
            })
    }
}
