//! Exclusive input on the existing native PTY. This never pauses Provider processes or tools.
use super::*;
use norma::mobile::{CardSnapshot, TerminalInput, TerminalPage, TerminalRequest};
use storage::MobileTurnKey;

#[cfg(test)]
#[path = "../../../tests/unit/mobile_terminal_lease_tests.rs"]
mod lease_tests;
#[cfg(test)]
#[path = "../../../tests/unit/mobile_terminal_control_tests.rs"]
mod tests;

/// Mac-only capability bound to one task. Network owners revoke it on disconnect or loss of access.
/// It cannot be deserialized or reconstructed from a phone-supplied operation identifier.
pub struct TerminalInputLease {
    key: MobileTurnKey,
    revoked: AtomicBool,
}

impl TerminalInputLease {
    pub fn new(key: MobileTurnKey) -> Result<Arc<Self>, &'static str> {
        key.validate().map_err(|_| "invalid_request")?;
        Ok(Arc::new(Self {
            key,
            revoked: AtomicBool::new(false),
        }))
    }

    /// May run off the foreground thread. Revocation is permanent and never alters PTY bytes.
    pub fn revoke(&self) {
        self.revoked.store(true, Ordering::Release);
    }

    pub fn is_revoked(&self) -> bool {
        self.revoked.load(Ordering::Acquire)
    }
}

pub(super) struct TerminalInputControl {
    lease: Arc<TerminalInputLease>,
    pub(super) deadline: Instant,
}

impl Drop for TerminalInputControl {
    fn drop(&mut self) {
        self.lease.revoke();
    }
}

impl MobileRun {
    fn active_terminal_control(&self) -> Option<&TerminalInputControl> {
        let owner = self.native_input.control.as_ref()?;
        if !self.native
            || self.control.is_cancelled()
            || self.run_id.as_deref() != Some(&owner.lease.key.run_id)
            || self.session_id.as_deref() != Some(&owner.lease.key.session_id)
            || Instant::now() >= owner.deadline
        {
            owner.lease.revoke();
        }
        (!owner.lease.revoked.load(Ordering::Acquire)).then_some(owner)
    }
}

impl Runtime {
    pub fn mobile_terminal_is_controlled(&self, node: NodeId) -> bool {
        self.mobile_runs
            .get(&node)
            .and_then(MobileRun::active_terminal_control)
            .is_some()
    }

    /// Caller authenticates the original device/task before reaching this foreground seam.
    /// Repeating an active claim renews it; revoked/expired capabilities can never acquire again.
    pub fn claim_mobile_terminal_input(
        &mut self,
        key: &MobileTurnKey,
        lease: &Arc<TerminalInputLease>,
    ) -> Result<(), &'static str> {
        if self.workspace_card_input_held(key.node_id) {
            return Err("workspace_input_pending");
        }
        if lease.key != *key {
            return Err("session_changed");
        }
        if lease.revoked.load(Ordering::Acquire) {
            return Err("terminal_control_expired");
        }
        let run = self
            .mobile_runs
            .get(&key.node_id)
            .ok_or("session_changed")?;
        if !run.native
            || run.control.is_cancelled()
            || run.run_id.as_deref() != Some(&key.run_id)
            || run.session_id.as_deref() != Some(&key.session_id)
            || !self
                .live
                .get(&key.node_id)
                .is_some_and(|live| live.terminal_status() == Some(TerminalStatus::Running))
        {
            return Err("session_changed");
        }
        if let Some(owner) = run.active_terminal_control() {
            if !Arc::ptr_eq(&owner.lease, lease) {
                return Err("terminal_controlled");
            }
        } else {
            // Checking the current slot above permanently revokes expired capabilities too.
            if lease.revoked.load(Ordering::Acquire) {
                return Err("terminal_control_expired");
            }
            if self.mobile_latest_turn_key(key.node_id).as_ref() != Some(key) {
                return Err("session_changed");
            }
            if run.native_input.has_pending_input() {
                return Err("desktop_input_pending");
            }
            if !super::terminal_view::terminal_input_ready(run) {
                return Err("state_not_ready");
            }
        }
        // 3 × 5s renewal intervals = 15s, covering two missed renewals without an indefinite lock.
        let control = &mut self
            .mobile_runs
            .get_mut(&key.node_id)
            .expect("validated run")
            .native_input
            .control;
        let deadline = Instant::now() + std::time::Duration::from_secs(15);
        if let Some(owner) = control
            .as_mut()
            .filter(|owner| Arc::ptr_eq(&owner.lease, lease))
        {
            owner.deadline = deadline;
        } else {
            *control = Some(TerminalInputControl {
                lease: lease.clone(),
                deadline,
            });
        }
        if lease.revoked.load(Ordering::Acquire) {
            return Err("terminal_control_expired");
        }
        Ok(())
    }

    /// Renewal never acquires. A successful durable claim receipt from an old connection is not ownership.
    pub fn renew_mobile_terminal_input(
        &mut self,
        key: &MobileTurnKey,
        lease: &Arc<TerminalInputLease>,
    ) -> Result<(), &'static str> {
        self.validate_terminal_input_owner(key, lease)?;
        self.claim_mobile_terminal_input(key, lease)
    }

    /// Idempotent for a released lease, but an old release never clears a newer owner.
    /// Draft tracking survives return/expiry; only a real Provider acknowledgement can clear it.
    pub fn release_mobile_terminal_input(
        &mut self,
        key: &MobileTurnKey,
        lease: &Arc<TerminalInputLease>,
    ) -> Result<(), &'static str> {
        let run = self
            .mobile_runs
            .get_mut(&key.node_id)
            .ok_or("session_changed")?;
        if lease.key != *key
            || run.run_id.as_deref() != Some(&key.run_id)
            || run.session_id.as_deref() != Some(&key.session_id)
        {
            return Err("session_changed");
        }
        if let Some(owner) = &mut run.native_input.control {
            if !Arc::ptr_eq(&owner.lease, lease) {
                return Err("terminal_controlled");
            }
        }
        lease.revoke();
        Ok(())
    }

    pub(super) fn validate_terminal_input_owner(
        &self,
        key: &MobileTurnKey,
        lease: &Arc<TerminalInputLease>,
    ) -> Result<(), &'static str> {
        if self.workspace_card_input_held(key.node_id) {
            return Err("workspace_input_pending");
        }
        if lease.revoked.load(Ordering::Acquire) {
            return Err("terminal_control_expired");
        }
        let owner = self
            .mobile_runs
            .get(&key.node_id)
            .and_then(MobileRun::active_terminal_control)
            .ok_or("terminal_control_expired")?;
        if lease.key != *key || !Arc::ptr_eq(&owner.lease, lease) {
            return Err("terminal_controlled");
        }
        Ok(())
    }

    /// A controlled session may advance to a human turn; its page reports the current Provider turn.
    /// The capability's original task key stays immutable. Reading alone does not renew it.
    pub fn controlled_mobile_terminal_page(
        &self,
        key: &MobileTurnKey,
        lease: &Arc<TerminalInputLease>,
        request: &TerminalRequest,
    ) -> Result<TerminalPage, &'static str> {
        self.validate_terminal_input_owner(key, lease)?;
        if request.node_id != key.node_id
            || request.run_id != key.run_id
            || request.session_id.as_deref() != Some(&key.session_id)
        {
            return Err("session_changed");
        }
        let mut page = self.mobile_terminal_page(request)?;
        page.input_ready = page.running
            && request.before.is_none()
            && super::terminal_view::terminal_input_ready(
                self.mobile_runs
                    .get(&key.node_id)
                    .ok_or("session_changed")?,
            );
        self.validate_terminal_input_owner(key, lease)?;
        Ok(page)
    }

    /// Same screen/input counter and original input tracker as ordinary terminal typing.
    /// An input request cannot renew or acquire ownership implicitly.
    pub fn send_controlled_terminal_input(
        &mut self,
        key: &MobileTurnKey,
        lease: &Arc<TerminalInputLease>,
        expected: &CardSnapshot,
        screen_signature: u64,
        input_sequence: u64,
        input: &TerminalInput,
    ) -> Result<(), &'static str> {
        self.validate_terminal_input_owner(key, lease)?;
        if expected.node_id != key.node_id {
            return Err("session_changed");
        }
        self.send_terminal_input(
            expected,
            screen_signature,
            input_sequence,
            input,
            Some((key, lease)),
        )
    }
}
