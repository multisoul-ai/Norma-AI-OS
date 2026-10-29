//! No input queue: preparation cannot replay a click into an unknown future terminal screen.
use super::*;
use crate::runtime::ControlledPty;
use crate::runtime::LaunchControl;
use std::sync::Arc;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex,
};
use std::time::Duration;
#[cfg(test)]
use std::time::Instant;
use terminal::PtyBackend;
#[cfg(test)]
use terminal::PtyEvent;

pub(super) struct DeferredPty {
    slot: Preparation,
}
#[derive(Clone)]
pub(super) struct Preparation {
    control: Arc<LaunchControl>,
    pty: Arc<Mutex<Option<ControlledPty>>>,
    size: Arc<AtomicU32>,
    resize_wake: crossbeam_channel::Sender<()>,
    resize_requests: crossbeam_channel::Receiver<()>,
}
impl DeferredPty {
    pub(super) fn new(control: Arc<LaunchControl>, cols: u16, rows: u16) -> Self {
        let (resize_wake, resize_requests) = crossbeam_channel::bounded(1);
        Self {
            slot: Preparation {
                control,
                pty: Arc::new(Mutex::new(None)),
                size: Arc::new(AtomicU32::new(pack(cols, rows))),
                resize_wake,
                resize_requests,
            },
        }
    }
    pub(super) fn preparation(&self) -> Preparation {
        self.slot.clone()
    }
    #[cfg(test)]
    pub(super) fn dimensions(&self) -> (u16, u16) {
        self.slot.dimensions()
    }
}
impl Preparation {
    pub(super) fn dimensions(&self) -> (u16, u16) {
        let value = self.size.load(Ordering::Acquire);
        ((value >> 16) as u16, value as u16)
    }
    pub(super) fn install(&self, mut pty: ControlledPty) -> Result<(), PtyError> {
        let mut slot = self
            .pty
            .lock()
            .map_err(|_| PtyError::Spawn("native backend poisoned".into()))?;
        check_launch(&self.control)?;
        let (cols, rows) = self.dimensions();
        pty.resize(cols, rows)?;
        *slot = Some(pty);
        Ok(())
    }

    /// The launch worker owns resize IO. A bounded wake coalesces requests; reading the
    /// latest size after acquiring the slot also covers resize during backend installation.
    pub(super) fn maintain_sizes(&self) {
        while !self.control.is_cancelled() {
            if self
                .resize_requests
                .recv_timeout(Duration::from_millis(50))
                .is_err()
            {
                continue;
            }
            let Ok(mut slot) = self.pty.lock() else {
                return;
            };
            if self.control.is_cancelled() {
                return;
            }
            if let Some(pty) = slot.as_mut() {
                let (cols, rows) = self.dimensions();
                if let Err(error) = pty.resize(cols, rows) {
                    tracing::warn!(%error, "native terminal resize failed");
                }
            }
        }
    }
}
impl PtyBackend for DeferredPty {
    fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        check_launch(&self.slot.control)?;
        let mut slot = self.slot.pty.try_lock().map_err(|_| starting())?;
        slot.as_mut().ok_or_else(starting)?.write(bytes)
    }
    fn resize(&mut self, cols: u16, rows: u16) -> Result<(), PtyError> {
        check_launch(&self.slot.control)?;
        self.slot.size.store(pack(cols, rows), Ordering::Release);
        let _ = self.slot.resize_wake.try_send(());
        Ok(())
    }
    fn kill(&mut self) {
        self.slot.control.cancel();
    }
    fn child_pid(&self) -> Option<u32> {
        self.slot.pty.try_lock().ok()?.as_ref()?.child_pid()
    }
    fn child_process_observer(&self) -> Option<terminal::ChildProcessObserver> {
        self.slot
            .pty
            .try_lock()
            .ok()?
            .as_ref()?
            .child_process_observer()
    }
}
impl Drop for DeferredPty {
    fn drop(&mut self) {
        self.kill();
    }
}
fn pack(cols: u16, rows: u16) -> u32 {
    (u32::from(cols) << 16) | u32::from(rows)
}
fn starting() -> PtyError {
    PtyError::Io(std::io::Error::new(
        std::io::ErrorKind::WouldBlock,
        "Agent 正在准备，请稍候",
    ))
}

#[cfg(test)]
#[path = "../../../tests/unit/codex_deferred_resize_tests.rs"]
mod tests;

pub(super) fn check_launch(control: &LaunchControl) -> Result<(), PtyError> {
    if control.is_cancelled() {
        return Err(PtyError::Spawn("native launch cancelled".into()));
    }
    Ok(())
}
