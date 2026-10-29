use super::ContentError;
use super::Result;
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Use normal Git configuration and hooks, without inherited repository routing or shell parsing.
pub(super) fn git(root: &Path, args: &[&str]) -> Result<String> {
    let mut stdout = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut stderr = tempfile::tempfile().map_err(|e| e.to_string())?;
    let mut command = Command::new("/usr/bin/git");
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .current_dir(root)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .stdin(Stdio::null())
        .stdout(stdout.try_clone().map_err(|e| e.to_string())?)
        .stderr(stderr.try_clone().map_err(|e| e.to_string())?);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    let start = Instant::now();
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Err(e) => break Err(ContentError::Diagnostic(e.to_string())),
            Ok(None) => {}
        }
        if stdout.metadata().map(|m| m.len()).unwrap_or(0) > 2 * 1024 * 1024
            || stderr.metadata().map(|m| m.len()).unwrap_or(0) > 2 * 1024 * 1024
        {
            break Err(ContentError::GitOutputLimit);
        }
        if start.elapsed() > Duration::from_secs(60) {
            break Err(ContentError::GitTimeout);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if outcome.is_err() {
        #[cfg(unix)]
        // SAFETY: the child owns its newly created process group; terminate its hooks too.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
        let _ = child.kill();
    }
    let _ = child.wait();
    let status = outcome?;
    let stream = if status.success() {
        &mut stdout
    } else {
        &mut stderr
    };
    stream.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    stream
        .take(128 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let message = String::from_utf8_lossy(&bytes).into_owned();
    if status.success() {
        Ok(message)
    } else {
        Err(ContentError::GitFailed(message))
    }
}
