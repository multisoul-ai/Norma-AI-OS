use super::*;
use norma::codex::CodexConnection;
use norma::mobile::ConversationPage;
use serde_json::{json, Value};
use std::sync::{Mutex, Weak};
use std::time::Duration;

#[derive(Default)]
pub(crate) struct ArchiveReads {
    pending: Vec<Weak<launch::LaunchControl>>,
    codex: Arc<CodexReader>,
}
#[derive(Default)]
struct CodexReader {
    connection: Mutex<Option<Arc<CodexConnection>>>,
    control: Mutex<Arc<launch::LaunchControl>>,
    #[cfg(test)]
    connector: Option<Arc<dyn Fn() -> Result<CodexConnection, String> + Send + Sync>>,
    #[cfg(feature = "e2e-test-support")]
    isolated_config: Option<PathBuf>,
}
impl Drop for CodexReader {
    fn drop(&mut self) {
        self.control
            .lock()
            .expect("Codex archive control poisoned")
            .cancel();
    }
}
impl Drop for ArchiveReads {
    fn drop(&mut self) {
        self.cancel();
    }
}
impl ArchiveReads {
    pub(crate) fn cancel(&mut self) {
        for control in self.pending.drain(..).filter_map(|weak| weak.upgrade()) {
            control.cancel();
        }
        self.codex
            .control
            .lock()
            .expect("Codex archive control poisoned")
            .cancel();
        self.codex = Arc::new(CodexReader::default());
    }
}

/// Runtime owns cancellation, while the background caller owns this short-lived read.
pub struct MobileArchiveRead {
    workspace: PathBuf,
    codex: Arc<CodexReader>,
    control: Arc<launch::LaunchControl>,
}
impl Drop for MobileArchiveRead {
    fn drop(&mut self) {
        self.control.cancel();
    }
}
impl Runtime {
    /// Keep the real TUI and read-only archive host in the same isolated test configuration.
    #[cfg(feature = "e2e-test-support")]
    pub fn set_mobile_codex_archive_config_for_e2e(&mut self, config: PathBuf) {
        Arc::get_mut(&mut self.archive_reads.codex)
            .expect("configure test archive before readers exist")
            .isolated_config = Some(config);
    }

    /// Capture only in-memory scope. No process, SQLite or file IO on the UI thread.
    pub fn mobile_archive_read(&mut self) -> Result<MobileArchiveRead, String> {
        self.archive_reads
            .pending
            .retain(|weak| weak.strong_count() > 0);
        // Same ceiling as the mobile server's eight concurrent peer connections.
        if self.archive_reads.pending.len() >= 8 {
            return Err("archive reader busy".into());
        }
        let workspace = self.workspace_root.clone().ok_or("project unavailable")?;
        let control = Arc::new(launch::LaunchControl::default());
        self.archive_reads.pending.push(Arc::downgrade(&control));
        Ok(MobileArchiveRead {
            workspace,
            control,
            codex: self.archive_reads.codex.clone(),
        })
    }
}
impl MobileArchiveRead {
    /// Caller loads an exact persisted identity after project authorization; no remote file paths.
    pub fn read(
        &self,
        saved: storage::RecordedMobileSession,
        cursor: Option<String>,
        deadline: Instant,
    ) -> Result<ConversationPage, String> {
        saved.session.key.validate().map_err(|e| e.to_string())?;
        if saved.session.workspace_root != self.workspace {
            return Err("archive workspace changed".into());
        }
        self.check(deadline)?;
        let page = match saved.session.key.profile.as_str() {
            "Claude" => {
                let cursor = cursor
                    .as_deref()
                    .map(serde_json::from_str::<norma::claude::TranscriptCursor>)
                    .transpose()
                    .map_err(|e| e.to_string())?;
                let raw = norma::claude::read_transcript(
                    saved
                        .transcript_path
                        .as_deref()
                        .ok_or("Claude has no saved transcript")?,
                    &history::claude_projects_root()?,
                    &saved.session.key.session_id,
                    &self.workspace,
                    cursor,
                    20,
                )?;
                ConversationPage::from_claude(raw)?
            }
            "Cursor" => {
                let cursor = cursor
                    .as_deref()
                    .map(serde_json::from_str::<norma::cursor::TranscriptCursor>)
                    .transpose()
                    .map_err(|error| error.to_string())?;
                let raw = norma::cursor::read_transcript(
                    saved
                        .transcript_path
                        .as_deref()
                        .ok_or("Cursor has no saved transcript")?,
                    &history::cursor_projects_root()?,
                    &saved.session.key.session_id,
                    &self.workspace,
                    cursor,
                    20,
                )?;
                ConversationPage::from_cursor(raw)?
            }
            "Codex" => self.codex(&saved.session.key.session_id, cursor, deadline)?,
            _ => return Err("archive provider unavailable".into()),
        };
        self.check(deadline)?;
        Ok(page)
    }
    pub(super) fn is_closed(&self) -> bool {
        self.control.is_cancelled()
    }

    fn check(&self, deadline: Instant) -> Result<(), String> {
        if self.control.is_cancelled() || Instant::now() >= deadline {
            return Err("archive read cancelled or expired".into());
        }
        Ok(())
    }

    fn codex(
        &self,
        session: &str,
        cursor: Option<String>,
        deadline: Instant,
    ) -> Result<ConversationPage, String> {
        let cursor = cursor
            .as_deref()
            .map(serde_json::from_str::<String>)
            .transpose()
            .map_err(|e| e.to_string())?;
        let connection = self.codex_connection(deadline)?;
        self.codex_page(&connection, session, cursor, deadline)
    }

    fn codex_connection(&self, deadline: Instant) -> Result<Arc<CodexConnection>, String> {
        Ok({
            // Only background reads acquire this lock; the UI cancels via the separate atomic control.
            let mut cached = self
                .codex
                .connection
                .lock()
                .map_err(|_| "Codex reader unavailable")?;
            if cached
                .as_ref()
                .is_none_or(|connection| !connection.is_connected())
            {
                let control = {
                    let mut control = self
                        .codex
                        .control
                        .lock()
                        .map_err(|_| "Codex reader unavailable")?;
                    if cached.is_some() || control.is_cancelled() {
                        control.cancel();
                        *control = Arc::new(launch::LaunchControl::default());
                    }
                    control.clone()
                };
                *cached = Some(Arc::new(self.connect_codex(deadline, &control)?));
            }
            cached.as_ref().expect("connected reader").clone()
        })
    }

    fn connect_codex(
        &self,
        deadline: Instant,
        control: &Arc<launch::LaunchControl>,
    ) -> Result<CodexConnection, String> {
        #[cfg(test)]
        if let Some(connector) = &self.codex.connector {
            return connector();
        }
        let directory = tempfile::Builder::new()
            .prefix("norma-codex-archive-")
            .tempdir_in("/tmp")
            .map_err(|e| e.to_string())?;
        let path = directory.path().join("read.sock");
        let endpoint = format!("unix://{}", path.display());
        let command = PtyCommand::new("/bin/zsh")
            .args([
                "-lic",
                &format!(
                    "exec codex app-server --listen {}",
                    crate::session_snapshot::shell_quote(&endpoint)
                ),
            ])
            .cwd(&self.workspace);
        #[cfg(feature = "e2e-test-support")]
        let command = {
            let mut command = command;
            if let Some(config) = &self.codex.isolated_config {
                command
                    .env
                    .push(("CODEX_HOME".into(), config.to_string_lossy().into_owned()));
            }
            command
        };
        control
            .spawn_codex_host(command, directory)
            .map_err(|e| e.to_string())?;
        let result = (|| {
            loop {
                self.check(deadline)?;
                if control.is_cancelled() {
                    return Err("Codex reader closed".into());
                }
                if path.exists() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            CodexConnection::connect(&path)
        })();
        if result.is_err() {
            control.cancel();
        }
        result
    }
    fn codex_page(
        &self,
        connection: &CodexConnection,
        session: &str,
        cursor: Option<String>,
        deadline: Instant,
    ) -> Result<ConversationPage, String> {
        self.validate_codex_thread(connection, session, deadline)?;
        ConversationPage::from_codex(self.rpc(
            connection,
            "thread/turns/list",
            json!({"threadId":session,"limit":20,"cursor":cursor,"itemsView":"full"}),
            deadline,
        )?)
    }

    fn validate_codex_thread(
        &self,
        connection: &CodexConnection,
        session: &str,
        deadline: Instant,
    ) -> Result<(), String> {
        // read/list never resume a thread, start a turn, attach a TUI or approve a tool.
        let metadata = self.rpc(
            connection,
            "thread/read",
            json!({"threadId":session,"includeTurns":false}),
            deadline,
        )?;
        let cwd = metadata["thread"]["cwd"]
            .as_str()
            .ok_or("Codex archive missing workspace")?;
        if metadata["thread"]["id"] != session
            || Path::new(cwd).canonicalize().map_err(|e| e.to_string())?
                != self.workspace.canonicalize().map_err(|e| e.to_string())?
        {
            return Err("Codex archive identity mismatch".into());
        }
        Ok(())
    }

    /// Read only canonical boundaries. thread/turns/list synthesizes interrupted for unfinished
    /// disk history and therefore cannot prove that the original TUI's turn has stopped.
    pub(super) fn codex_interrupt_boundary(
        &self,
        session: &str,
        turn: &str,
        after: Option<u64>,
        deadline: Instant,
    ) -> Result<(u64, Option<&'static str>), String> {
        let connection = self.codex_connection(deadline)?;
        self.validate_codex_thread(&connection, session, deadline)?;
        let page = self.rpc(
            &connection,
            "thread/timeline/list",
            json!({"threadId":session,"limit":100}),
            deadline,
        )?;
        timeline_boundary(&page, turn, after)
    }
    fn rpc(
        &self,
        connection: &CodexConnection,
        method: &str,
        params: Value,
        deadline: Instant,
    ) -> Result<Value, String> {
        self.check(deadline)?;
        let response = connection.request(method, params)?;
        loop {
            self.check(deadline)?;
            match response.recv_timeout(Duration::from_millis(20)) {
                Ok(value) => return value,
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err("Codex archive disconnected".into()),
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile_codex_archive_reconnect_tests.rs"]
mod reconnect_tests;

pub(super) fn timeline_boundary(
    page: &Value,
    turn: &str,
    after: Option<u64>,
) -> Result<(u64, Option<&'static str>), String> {
    let entries = page["data"]
        .as_array()
        .ok_or("Codex timeline missing entries")?;
    // Matches the explicit RPC limit; bounded by the connection's existing 8MiB frame cap too.
    if entries.len() > 100 {
        return Err("Codex timeline exceeds requested page".into());
    }
    let mut latest = 0;
    let mut completion = None;
    for entry in entries {
        let position = entry["position"]
            .as_u64()
            .ok_or("Codex timeline missing position")?;
        latest = latest.max(position);
        if entry["type"] != "turnCompleted"
            || entry["turnId"] != turn
            || after.is_some_and(|after| position <= after)
        {
            continue;
        }
        let status = match entry["status"].as_str() {
            Some("interrupted") => "interrupted",
            Some("completed") => "completed",
            Some("failed") => "failed",
            _ => return Err("Codex timeline has unknown completion".into()),
        };
        if completion.replace(status).is_some() {
            return Err("Codex timeline repeats turn completion".into());
        }
    }
    Ok((latest, completion))
}
