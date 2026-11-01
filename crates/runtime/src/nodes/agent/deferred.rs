//! No input queue: input is accepted only once the native screen is visible, never replayed later.
use super::*;
use crate::runtime::ControlledPty;
use crate::runtime::LaunchControl;
use std::sync::Arc;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use terminal::PtyBackend;
#[cfg(test)]
use terminal::PtyEvent;

/// Startup budget: idle time allowed without user input, and an absolute ceiling.
#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub(super) idle: Duration,
    pub(super) cap: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            idle: Duration::from_secs(60),
            cap: Duration::from_secs(600),
        }
    }
}

/// Distinguishes a user answering a native prompt (e.g. update menu) from a stuck launch:
/// only keyboard input extends the deadline; output alone never does.
struct Handshake {
    limits: Limits,
    started: Instant,
    state: Mutex<HandshakeState>,
}
#[derive(Default)]
struct HandshakeState {
    visible: bool,
    marker_seen: bool,
    /// Tail of the previous chunk, so a marker split across reads is still found.
    tail: Vec<u8>,
    last_input: Option<Instant>,
}
/// Printed by the launch wrapper right before it execs the native TUI. Output before it is
/// login-shell noise (rc files, nvm) the user cannot answer; keys typed then would queue in the
/// PTY line buffer and hit the first menu's default item.
pub(super) const TUI_START_MARKER: &[u8] = b"\x1b]9999;norma-tui-start\x07";
impl Handshake {
    fn expired(&self) -> bool {
        let now = Instant::now();
        let last = self.state.lock().ok().and_then(|state| state.last_input);
        let deadline = last.unwrap_or(self.started).max(self.started) + self.limits.idle;
        now >= deadline.min(self.started + self.limits.cap)
    }
}

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
    handshake: Arc<Handshake>,
}
impl DeferredPty {
    /// Other agents accept input as soon as their backend is installed.
    pub(super) fn new(control: Arc<LaunchControl>, cols: u16, rows: u16) -> Self {
        let backend = Self::with_limits(control, cols, rows, Limits::default());
        backend.slot.mark_screen_visible();
        backend
    }
    /// Native Codex: the backend is installed early, so input waits for the first visible output.
    pub(super) fn with_limits(
        control: Arc<LaunchControl>,
        cols: u16,
        rows: u16,
        limits: Limits,
    ) -> Self {
        let (resize_wake, resize_requests) = crossbeam_channel::bounded(1);
        Self {
            slot: Preparation {
                control,
                pty: Arc::new(Mutex::new(None)),
                size: Arc::new(AtomicU32::new(pack(cols, rows))),
                resize_wake,
                resize_requests,
                handshake: Arc::new(Handshake {
                    limits,
                    started: Instant::now(),
                    state: Mutex::new(HandshakeState::default()),
                }),
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
    /// Input is accepted once the native TUI itself has drawn something: the start marker
    /// followed by at least one more byte.
    pub(super) fn observe_output(&self, bytes: &[u8]) {
        let Ok(mut state) = self.handshake.state.lock() else {
            return;
        };
        if state.visible {
            return;
        }
        if state.marker_seen {
            state.visible = !bytes.is_empty();
            return;
        }
        let mut window = std::mem::take(&mut state.tail);
        window.extend_from_slice(bytes);
        if let Some(at) = window
            .windows(TUI_START_MARKER.len())
            .position(|w| w == TUI_START_MARKER)
        {
            state.marker_seen = true;
            state.visible = window.len() > at + TUI_START_MARKER.len();
        } else {
            let keep = TUI_START_MARKER.len() - 1;
            state.tail = window[window.len().saturating_sub(keep)..].to_vec();
        }
    }
    /// For backends that are not gated (everything except native Codex).
    pub(super) fn mark_screen_visible(&self) {
        if let Ok(mut state) = self.handshake.state.lock() {
            state.visible = true;
        }
    }
    pub(super) fn launch_expired(&self) -> bool {
        self.handshake.expired()
    }
    pub(super) fn accepts_input(&self) -> bool {
        self.handshake.state.lock().is_ok_and(|state| state.visible)
    }
    fn note_input(&self) {
        if let Ok(mut state) = self.handshake.state.lock() {
            state.last_input = Some(Instant::now());
        }
    }
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
        if !self.slot.accepts_input() {
            return Err(starting());
        }
        let mut slot = self.slot.pty.try_lock().map_err(|_| starting())?;
        slot.as_mut().ok_or_else(starting)?.write(bytes)?;
        self.slot.note_input();
        Ok(())
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
