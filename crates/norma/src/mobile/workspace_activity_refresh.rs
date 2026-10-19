/// Coalesces activity refreshes. `mark` only schedules; `begin` is the snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActivityRefreshGate {
    dirty: bool,
    scheduled: bool,
    inflight: bool,
}

impl ActivityRefreshGate {
    /// Returns whether the caller should start one recompute.
    pub fn mark(&mut self) -> bool {
        if self.inflight {
            self.dirty = true;
            return false;
        }
        if self.scheduled {
            return false;
        }
        self.scheduled = true;
        true
    }

    /// Called when the background pass reads facts. Earlier marks are in that snapshot.
    pub fn begin(&mut self) {
        self.scheduled = false;
        self.inflight = true;
        self.dirty = false;
    }

    /// Returns whether changes after the snapshot need one more pass.
    pub fn finish(&mut self) -> bool {
        self.inflight = false;
        if !self.dirty {
            return false;
        }
        self.dirty = false;
        self.scheduled = true;
        true
    }
}
