//! Reap the client before closing ConPTY; keep reading until its final output reaches EOF.
use super::{child_exit_code, PtyError};
use crate::PtyEvent;
use crossbeam_channel::{bounded, Receiver, SendTimeoutError, Sender};
use portable_pty::{Child, ChildKiller, MasterPty, PtySize};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

type SharedConsole = Arc<Mutex<Option<Box<dyn MasterPty + Send>>>>;

#[derive(Clone)]
pub(super) struct Master {
    console: SharedConsole,
    cancelled: Arc<AtomicBool>,
}

impl Master {
    pub(super) fn resize(&self, size: PtySize) -> Result<(), String> {
        self.console
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .ok_or_else(|| "ConPTY has exited".to_string())?
            .resize(size)
            .map_err(|error| error.to_string())
    }

    fn close(&self) {
        let master = self
            .console
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take();
        // ClosePseudoConsole can wait for output: the reader remains active, and resize never
        // waits behind this blocking destruction on the foreground thread.
        drop(master);
    }

    // Failed construction has no subscriber. An installed PTY must still publish Exited on kill.
    fn cancel_preparation_output(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

struct CloseOnDrop(Master);
impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        self.0.close();
    }
}

pub(super) fn start(
    master: Box<dyn MasterPty + Send>,
    reader: Box<dyn Read + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Sender<PtyEvent>,
) -> Result<Master, PtyError> {
    let master = Master {
        console: Arc::new(Mutex::new(Some(master))),
        cancelled: Arc::new(AtomicBool::new(false)),
    };
    let cancelled = master.cancelled.clone();
    let killer = child.clone_killer();
    let child = Arc::new(Mutex::new(child));
    let (finished, completion) = bounded(1);
    let reader_thread = match thread::Builder::new()
        .name("conpty-reader".into())
        .spawn(move || drain(reader, output, killer, completion, cancelled))
    {
        Ok(thread) => thread,
        Err(error) => {
            let mut child = child.lock().unwrap_or_else(|error| error.into_inner());
            let _ = child.kill();
            let _ = child.wait();
            return Err(PtyError::Io(error));
        }
    };
    let waiting = child.clone();
    let closing = master.clone();
    if let Err(error) = thread::Builder::new()
        .name("conpty-wait".into())
        .spawn(move || {
            let closing = CloseOnDrop(closing);
            let code = child_exit_code(
                waiting
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .wait(),
            );
            drop(closing);
            let _ = finished.send(code);
        })
    {
        master.cancel_preparation_output();
        let mut child = child.lock().unwrap_or_else(|error| error.into_inner());
        let _ = child.kill();
        let _ = child.wait();
        drop(child);
        master.close();
        let _ = reader_thread.join();
        return Err(PtyError::Io(error));
    }
    Ok(master)
}

fn drain(
    mut reader: Box<dyn Read + Send>,
    output: Sender<PtyEvent>,
    mut killer: Box<dyn ChildKiller + Send + Sync>,
    completion: Receiver<Option<i32>>,
    cancelled: Arc<AtomicBool>,
) {
    let mut buffer = [0u8; 8192];
    let mut forwarding = true;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                drop(reader);
                let _ = killer.kill();
                let code = completion.recv().unwrap_or(None);
                let _ = forward(&output, PtyEvent::Exited { code }, &cancelled);
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                tracing::warn!(%error, "ConPTY output read failed");
                drop(reader);
                let _ = killer.kill();
                let _ = completion.recv();
                let _ = forward(&output, PtyEvent::Exited { code: None }, &cancelled);
                return;
            }
            Ok(count) => {
                if forwarding
                    && !forward(
                        &output,
                        PtyEvent::Output(buffer[..count].to_vec()),
                        &cancelled,
                    )
                {
                    forwarding = false;
                    let _ = killer.kill();
                }
                // A detached consumer has cancelled the session. Continue draining without
                // buffering so ClosePseudoConsole cannot deadlock on a full output pipe.
            }
        }
    }
}

fn forward(output: &Sender<PtyEvent>, mut event: PtyEvent, cancelled: &AtomicBool) -> bool {
    while !cancelled.load(Ordering::Acquire) {
        match output.send_timeout(event, Duration::from_millis(5)) {
            Ok(()) => return true,
            Err(SendTimeoutError::Timeout(pending)) => event = pending,
            Err(SendTimeoutError::Disconnected(_)) => return false,
        }
    }
    false
}
