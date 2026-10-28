//! Exact-session background reads, requested by the original terminal owner. No input or writes.
use super::store::{CursorQuestionStore, CursorStoreSnapshot};
use crossbeam_channel::{Receiver, Sender};
use std::time::Instant;

pub struct CursorQuestionHost {
    node: u64,
    process: String,
    session: String,
    connection: String,
    requests: Sender<()>,
    snapshots: Receiver<(Instant, Result<CursorStoreSnapshot, &'static str>)>,
    outstanding: bool,
    last_requested: Option<Instant>,
}
impl CursorQuestionHost {
    pub fn start(
        config_root: &std::path::Path,
        workspace: &str,
        session: &str,
        node: u64,
        process: String,
    ) -> Result<Self, String> {
        let store = CursorQuestionStore::for_session(config_root, workspace, session)?;
        if process.is_empty() || process.len() > 128 {
            return Err("cursor_invalid_process".into());
        }
        let connection = format!(
            "cursor-native:{}",
            crate::agent_hook::fresh_control_id().map_err(|_| "cursor_connection_unavailable")?
        );
        let (requests, read) = crossbeam_channel::bounded(1);
        let (send, snapshots) = crossbeam_channel::bounded(1);
        std::thread::Builder::new()
            .name("cursor-question-store".into())
            .spawn(move || {
                while read.recv().is_ok() {
                    let started = Instant::now();
                    let result = store.read();
                    if send.send((started, result)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|_| "cursor_worker_unavailable")?;
        Ok(Self {
            node,
            process,
            session: session.into(),
            connection,
            requests,
            snapshots,
            outstanding: false,
            last_requested: None,
        })
    }
    pub fn node_id(&self) -> u64 {
        self.node
    }
    pub fn process_id(&self) -> &str {
        &self.process
    }
    pub fn session_id(&self) -> &str {
        &self.session
    }
    pub fn connection_id(&self) -> &str {
        &self.connection
    }
    /// Foreground channel-only poll. At most one outstanding read, never reuse an old snapshot.
    pub fn poll(&mut self) -> Option<(Instant, Result<CursorStoreSnapshot, &'static str>)> {
        let snapshot = self.snapshots.try_recv().ok();
        if snapshot.is_some() {
            self.outstanding = false;
        }
        if !self.outstanding
            && self
                .last_requested
                .is_none_or(|at| at.elapsed() >= std::time::Duration::from_millis(100))
            && self.requests.try_send(()).is_ok()
        {
            self.outstanding = true;
            self.last_requested = Some(Instant::now());
        }
        snapshot
    }
}
