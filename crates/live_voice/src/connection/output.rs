use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct OutputGate(AtomicU64);
impl OutputGate {
    pub fn epoch(&self) -> u64 {
        self.0.load(Ordering::Acquire)
    }
    pub fn is_current(&self, epoch: u64) -> bool {
        epoch & 1 == 0 && self.epoch() == epoch
    }
}
impl LiveControl {
    /// Invalidate queued local audio on both edges. This does not cancel remote speech or tasks.
    pub fn set_output_blocked(&self, blocked: bool) {
        let gate = self.input_gate.lock().expect("Live gate lock");
        let previous = gate.output.epoch();
        gate.output.0.store(
            (previous.wrapping_add(2) & !1) | u64::from(blocked),
            Ordering::Release,
        );
    }
}
