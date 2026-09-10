use super::{check_deadline, ReviewError};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub(super) fn check_repository(root: &super::Root, deadline: Instant) -> Result<(), ReviewError> {
    // status can invoke clean/process filters or descend into submodules with their own config.
    // Until those are read without executing repository programs, reject them explicitly.
    let (code, _) = git(
        root,
        &[
            "config",
            "--name-only",
            "--get-regexp",
            r"^filter\..*\.(clean|process)$",
        ],
        deadline,
        64 * 1024,
    )?;
    if code == 0 {
        return Err(ReviewError::Unsupported);
    }
    if code != 1 {
        return Err(ReviewError::Unavailable);
    }
    // Metadata only, <=8MiB; this does not read tracked files or run filters.
    let (code, index) = git(
        root,
        &["ls-files", "--stage", "-v", "-z", "--", "."],
        deadline,
        8 * 1024 * 1024,
    )?;
    if code != 0 {
        return Err(ReviewError::Unavailable);
    }
    if index.split(|b| *b == 0).any(|entry| {
        // -v prefixes each record with its index tag. S=skip-worktree; lowercase=assume-unchanged.
        // Both can hide real working-copy edits from status, so HEAD would be a false baseline.
        entry
            .first()
            .is_some_and(|tag| *tag == b'S' || tag.is_ascii_lowercase())
            || entry
                .get(2..)
                .is_some_and(|entry| entry.starts_with(b"160000 "))
    }) {
        return Err(ReviewError::Unsupported);
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn git(
    root: &super::Root,
    args: &[&str],
    deadline: Instant,
    limit: usize,
) -> Result<(i32, Vec<u8>), ReviewError> {
    let mut command = Command::new("/usr/bin/git");
    // Inherited Git routing/config cannot redirect a read to an unrelated working tree.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            command.env_remove(key);
        }
    }
    command
        .args([
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "diff.external=",
            "-c",
            "core.quotePath=false",
        ])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    root.cwd(&mut command)?;
    run(command, deadline, limit)
}
pub(super) fn run(
    mut command: Command,
    deadline: Instant,
    limit: usize,
) -> Result<(i32, Vec<u8>), ReviewError> {
    check_deadline(deadline)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|_| ReviewError::Unavailable)?;
    let stdout = child
        .stdout
        .take()
        .expect("stdout was configured as a pipe");
    let (sender, receiver) = mpsc::sync_channel(1);
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = stdout
            .take(limit as u64 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    let mut output = None;
    let outcome = loop {
        if output.is_none() {
            match receiver.try_recv() {
                Ok(Ok(bytes)) if bytes.len() > limit => break Err(ReviewError::TooLarge),
                Ok(Ok(bytes)) => output = Some(bytes),
                Ok(Err(_)) => break Err(ReviewError::Unavailable),
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => break Err(ReviewError::Unavailable),
            }
        }
        if let Err(error) = check_deadline(deadline) {
            break Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) if output.is_some() => {
                break Ok((
                    status.code().unwrap_or(-1),
                    output.take().expect("output ready"),
                ))
            }
            Ok(_) => {}
            Err(_) => break Err(ReviewError::Unavailable),
        }
        //5ms checks are only while this short local process runs; all work is off the UI thread.
        thread::sleep(Duration::from_millis(5));
    };
    if outcome.is_err() {
        #[cfg(unix)]
        {
            // SAFETY: spawn created a fresh group with the child's pid. Reap below even on timeout.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
        }
        let _ = child.kill();
    }
    let _ = child.wait();
    let _ = reader.join();
    outcome
}
