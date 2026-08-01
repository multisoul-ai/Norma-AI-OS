use super::{
    parse_git_progress_line, secure_cloned_workspace, CloneDestination, CloneProgress,
    CloneTransport, ClonedWorkspaceSecurityError, GitHubRemote,
};
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

const CHILD_POLL_INTERVAL: Duration = Duration::from_millis(25);
const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub struct GitCloneRequest {
    remote_url: String,
    destination: CloneDestination,
}

impl GitCloneRequest {
    pub fn new(
        remote: &GitHubRemote,
        transport: CloneTransport,
        destination: CloneDestination,
    ) -> Self {
        Self {
            remote_url: remote.url_for(transport),
            destination,
        }
    }

    pub fn target(&self) -> &Path {
        self.destination.target()
    }

    #[cfg(test)]
    pub(crate) fn for_url_for_test(remote_url: String, destination: CloneDestination) -> Self {
        Self {
            remote_url,
            destination,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CloneCancellation {
    cancelled: Arc<AtomicBool>,
}

impl CloneCancellation {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CloneEvent {
    Started { target: PathBuf },
    Progress(CloneProgress),
    Completed { target: PathBuf },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloneFailureKind {
    Cancelled,
    GitUnavailable,
    DestinationUnavailable,
    AuthenticationFailed,
    HostVerificationFailed,
    RepositoryNotFound,
    NetworkUnavailable,
    GitFailed,
    UnsafeWorkspaceState,
    SecurityFinalizeFailed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CloneFailure {
    kind: CloneFailureKind,
    message: String,
    target: PathBuf,
    target_preserved: bool,
}

impl CloneFailure {
    pub fn kind(&self) -> CloneFailureKind {
        self.kind
    }

    pub fn target(&self) -> &Path {
        &self.target
    }

    pub fn target_preserved(&self) -> bool {
        self.target_preserved
    }

    fn new(
        kind: CloneFailureKind,
        message: impl Into<String>,
        target: PathBuf,
        target_preserved: bool,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            target,
            target_preserved,
        }
    }
}

impl fmt::Display for CloneFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CloneFailure {}

#[derive(Clone, Debug)]
pub struct GitCloneRunner {
    git_binary: PathBuf,
}

impl GitCloneRunner {
    pub fn system() -> Self {
        Self {
            git_binary: PathBuf::from("git"),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_binary_for_test(git_binary: PathBuf) -> Self {
        Self { git_binary }
    }

    pub fn run(
        &self,
        request: GitCloneRequest,
        cancellation: &CloneCancellation,
        mut on_event: impl FnMut(CloneEvent),
    ) -> Result<PathBuf, CloneFailure> {
        let target = request.destination.target().to_path_buf();
        if cancellation.is_cancelled() {
            return Err(cancelled_failure(target));
        }

        fs::create_dir(&target).map_err(|error| {
            CloneFailure::new(
                CloneFailureKind::DestinationUnavailable,
                format!("Could not create the project folder: {error}"),
                target.clone(),
                false,
            )
        })?;
        let owned_target = OwnedTarget::capture(target.clone()).map_err(|error| {
            let _ = fs::remove_dir(&target);
            CloneFailure::new(
                CloneFailureKind::DestinationUnavailable,
                format!("Could not verify the project folder: {error}"),
                target.clone(),
                false,
            )
        })?;

        if cancellation.is_cancelled() {
            owned_target.cleanup();
            return Err(cancelled_failure(target));
        }
        on_event(CloneEvent::Started {
            target: target.clone(),
        });

        let mut command = Command::new(&self.git_binary);
        command
            .args(["clone", "--progress", "--"])
            .arg(&request.remote_url)
            .arg(&target)
            .env("GIT_TERMINAL_PROMPT", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let mut child = command.spawn().map_err(|error| {
            owned_target.cleanup();
            CloneFailure::new(
                CloneFailureKind::GitUnavailable,
                format!("Could not start Git: {error}"),
                target.clone(),
                false,
            )
        })?;

        let stderr = child.stderr.take();
        let (message_tx, message_rx) = mpsc::channel();
        let reader =
            stderr.map(|stderr| thread::spawn(move || read_git_stderr(stderr, message_tx)));
        let mut diagnostics = String::new();

        let status = loop {
            drain_reader_messages(&message_rx, &mut diagnostics, &mut on_event);
            if cancellation.is_cancelled() {
                terminate_child_tree(&mut child);
                if let Some(reader) = reader {
                    let _ = reader.join();
                }
                owned_target.cleanup();
                return Err(cancelled_failure(target));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(CHILD_POLL_INTERVAL),
                Err(error) => {
                    terminate_child_tree(&mut child);
                    if let Some(reader) = reader {
                        let _ = reader.join();
                    }
                    owned_target.cleanup();
                    return Err(CloneFailure::new(
                        CloneFailureKind::GitFailed,
                        format!("Could not monitor Git: {error}"),
                        target,
                        false,
                    ));
                }
            }
        };

        if let Some(reader) = reader {
            let _ = reader.join();
        }
        drain_reader_messages(&message_rx, &mut diagnostics, &mut on_event);
        if !status.success() {
            owned_target.cleanup();
            let kind = classify_git_failure(&diagnostics);
            return Err(CloneFailure::new(
                kind,
                failure_message(kind),
                target,
                false,
            ));
        }

        if let Err(error) = secure_cloned_workspace(&target) {
            let kind = match error {
                ClonedWorkspaceSecurityError::TrackedNormaState => {
                    CloneFailureKind::UnsafeWorkspaceState
                }
                _ => CloneFailureKind::SecurityFinalizeFailed,
            };
            return Err(CloneFailure::new(kind, error.to_string(), target, true));
        }

        on_event(CloneEvent::Completed {
            target: target.clone(),
        });
        Ok(target)
    }
}

fn terminate_child_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let process_group = -(child.id() as i32);
        // SAFETY: the child was spawned into a fresh process group whose id equals
        // its pid. A negative pid targets only that group; wait below reaps Git.
        let killed = unsafe { libc::kill(process_group, libc::SIGKILL) } == 0;
        if !killed {
            let _ = child.kill();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn read_git_stderr(stderr: impl std::io::Read, sender: mpsc::Sender<ReaderMessage>) {
    let mut reader = BufReader::new(stderr);
    let mut chunk = Vec::new();
    loop {
        chunk.clear();
        match reader.read_until(b'\r', &mut chunk) {
            Ok(0) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&chunk);
                for line in text.split(['\r', '\n']).filter(|line| !line.is_empty()) {
                    if let Some(progress) = parse_git_progress_line(line) {
                        let _ = sender.send(ReaderMessage::Progress(progress));
                    }
                    let _ = sender.send(ReaderMessage::Diagnostic(line.to_string()));
                }
            }
            Err(_) => break,
        }
    }
}

enum ReaderMessage {
    Progress(CloneProgress),
    Diagnostic(String),
}

fn drain_reader_messages(
    receiver: &mpsc::Receiver<ReaderMessage>,
    diagnostics: &mut String,
    on_event: &mut impl FnMut(CloneEvent),
) {
    let mut latest_progress = None;
    while let Ok(message) = receiver.try_recv() {
        match message {
            ReaderMessage::Progress(progress) => latest_progress = Some(progress),
            ReaderMessage::Diagnostic(line) => {
                if diagnostics.len() < MAX_DIAGNOSTIC_BYTES {
                    diagnostics.push_str(&line);
                    diagnostics.push('\n');
                }
            }
        }
    }
    if let Some(progress) = latest_progress {
        on_event(CloneEvent::Progress(progress));
    }
}

pub(super) fn classify_git_failure(diagnostics: &str) -> CloneFailureKind {
    let diagnostics = diagnostics.to_ascii_lowercase();
    if diagnostics.contains("authentication failed")
        || diagnostics.contains("could not read username")
        || diagnostics.contains("permission denied (publickey)")
    {
        CloneFailureKind::AuthenticationFailed
    } else if diagnostics.contains("host key verification failed") {
        CloneFailureKind::HostVerificationFailed
    } else if diagnostics.contains("repository not found")
        || diagnostics.contains("does not appear to be a git repository")
    {
        CloneFailureKind::RepositoryNotFound
    } else if diagnostics.contains("could not resolve host")
        || diagnostics.contains("failed to connect")
        || diagnostics.contains("network is unreachable")
    {
        CloneFailureKind::NetworkUnavailable
    } else {
        CloneFailureKind::GitFailed
    }
}

fn failure_message(kind: CloneFailureKind) -> &'static str {
    match kind {
        CloneFailureKind::AuthenticationFailed => {
            "GitHub authentication failed. Check your HTTPS credentials or SSH key."
        }
        CloneFailureKind::HostVerificationFailed => {
            "SSH host verification failed. Verify GitHub's host key and try again."
        }
        CloneFailureKind::RepositoryNotFound => {
            "The repository was not found or your account cannot access it."
        }
        CloneFailureKind::NetworkUnavailable => {
            "Could not reach GitHub. Check your network connection and proxy settings."
        }
        _ => "Git could not clone this repository.",
    }
}

fn cancelled_failure(target: PathBuf) -> CloneFailure {
    CloneFailure::new(
        CloneFailureKind::Cancelled,
        "Clone cancelled.",
        target,
        false,
    )
}

struct OwnedTarget {
    path: PathBuf,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl OwnedTarget {
    fn capture(path: PathBuf) -> std::io::Result<Self> {
        let metadata = fs::symlink_metadata(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(Self {
                path,
                device: metadata.dev(),
                inode: metadata.ino(),
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Self { path })
        }
    }

    fn cleanup(&self) {
        let Ok(metadata) = fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.dev() != self.device || metadata.ino() != self.inode {
                return;
            }
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}
