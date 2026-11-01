use super::{check_cancelled, disk, A1InstallFailure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

pub(super) fn file_hash(path: &Path) -> Result<String, A1InstallFailure> {
    let mut file = fs::File::open(path).map_err(|e| disk(path, e))?;
    let mut hash = Sha256::new();
    std::io::copy(&mut file, &mut hash).map_err(|e| disk(path, e))?;
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_a1_runtime_process_tests.rs"]
mod tests;

fn run(
    mut command: Command,
    seconds: u64,
    cancelled: &AtomicBool,
) -> Result<Vec<u8>, A1InstallFailure> {
    check_cancelled(cancelled)?;
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME");
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(|_| A1InstallFailure::Python)?;
    let stdout = child.stdout.take().ok_or(A1InstallFailure::Python)?;
    let stderr = child.stderr.take().ok_or(A1InstallFailure::Python)?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let read = |pipe: Box<dyn Read + Send>, is_stdout: bool| {
        let sender = sender.clone();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.take(65_537).read_to_end(&mut bytes);
            let _ = sender.send((is_stdout, bytes));
        })
    };
    let out = read(Box::new(stdout), true);
    let err = read(Box::new(stderr), false);
    let mut output = None;
    let mut error = None;
    let mut exit_status = None;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let result = loop {
        if let Err(error) = check_cancelled(cancelled) {
            break Err(error);
        }
        while let Ok((is_stdout, bytes)) = receiver.try_recv() {
            if is_stdout {
                output = Some(bytes);
            } else {
                error = Some(bytes);
            }
        }
        if exit_status.is_none() {
            match child.try_wait() {
                Ok(status) => exit_status = status,
                Err(_) => break Err(A1InstallFailure::Python),
            }
        }
        if let Some(status) = exit_status {
            if output.is_some() && error.is_some() {
                break if status.success() {
                    Ok(())
                } else {
                    Err(A1InstallFailure::Dependencies)
                };
            }
        }
        if Instant::now() >= deadline {
            break Err(A1InstallFailure::Dependencies);
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    // The process group is created for this command only. Reap its descendants even
    // when the parent exited before a pipe closed; never block in a reader join.
    #[cfg(target_os = "macos")]
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    if result.is_err() && exit_status.is_none() {
        let _ = child.kill();
    }
    let _ = child.wait();
    if out.is_finished() {
        let _ = out.join();
    }
    if err.is_finished() {
        let _ = err.join();
    }
    let output = output.unwrap_or_default();
    let error = error.unwrap_or_default();
    // Only classify stderr. Never log subprocess output or inherited environment values.
    if error.windows(23).any(|w| w == b"No space left on device") {
        return Err(A1InstallFailure::Disk {
            file: "runtime".into(),
            code: Some(28),
        });
    }
    result?;
    if output.len() > 65_536 {
        return Err(A1InstallFailure::Python);
    }
    Ok(output)
}

fn packages_match(python: &Path, cancelled: &AtomicBool) -> Result<bool, A1InstallFailure> {
    if !python.is_absolute() || !python.is_file() {
        return Err(A1InstallFailure::Python);
    }
    let expected: BTreeMap<String, String> = asr::A1_REQUIREMENTS
        .lines()
        .filter_map(|l| l.split_once("=="))
        .map(|(name, version)| (name.into(), version.into()))
        .collect();
    let script="import sys,json,importlib.metadata as m\ndef version(n):\n try:return m.version(n)\n except m.PackageNotFoundError:return None\nnames=json.loads(sys.argv[1]);print(json.dumps({'version':list(sys.version_info[:2]),'packages':{n:version(n) for n in names}}))";
    let mut command = Command::new(python);
    command.args(["-I", "-B", "-c", script]).arg(
        serde_json::to_string(&expected.keys().collect::<Vec<_>>())
            .map_err(|_| A1InstallFailure::Manifest)?,
    );
    let output = run(command, 30, cancelled)?;
    let value: serde_json::Value =
        serde_json::from_slice(&output).map_err(|_| A1InstallFailure::Python)?;
    if value["version"] != serde_json::json!([3, 12]) {
        return Err(A1InstallFailure::PythonVersion);
    }
    Ok(expected
        .iter()
        .all(|(n, v)| value["packages"][n].as_str() == Some(v)))
}

fn save_cache(cached: &Path, python: &Path) -> Result<(), A1InstallFailure> {
    use std::io::Write;
    let parent = cached.parent().ok_or(A1InstallFailure::Manifest)?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| disk(cached, e))?;
    serde_json::to_writer(&mut file, python).map_err(|_| A1InstallFailure::Manifest)?;
    file.flush().map_err(|e| disk(cached, e))?;
    file.as_file().sync_all().map_err(|e| disk(cached, e))?;
    file.persist(cached).map_err(|e| disk(cached, e.error))?;
    Ok(())
}

pub(super) fn prepare(
    root: &Path,
    python: &Path,
    cancelled: &AtomicBool,
) -> Result<PathBuf, A1InstallFailure> {
    if packages_match(python, cancelled)? {
        return Ok(python.to_owned());
    }
    let a1 = root.join("a1");
    fs::create_dir_all(&a1).map_err(|e| disk(&a1, e))?;
    let cached = a1.join("python-runtime.json");
    if let Ok(bytes) = fs::read(&cached) {
        if let Ok(path) = serde_json::from_slice::<PathBuf>(&bytes) {
            if packages_match(&path, cancelled).unwrap_or(false) {
                return Ok(path);
            }
        }
    }
    let temporary = tempfile::Builder::new()
        .prefix("runtime-")
        .tempdir_in(&a1)
        .map_err(|e| disk(&a1, e))?;
    let mut venv = Command::new(python);
    venv.args(["-I", "-B", "-m", "venv"]).arg(temporary.path());
    run(venv, 120, cancelled)?;
    let installed_python = temporary.path().join("bin/python");
    let lock = temporary.path().join("requirements.lock");
    fs::write(&lock, asr::A1_REQUIREMENTS).map_err(|e| disk(&lock, e))?;
    let mut pip = Command::new(&installed_python);
    pip.args([
        "-I",
        "-B",
        "-m",
        "pip",
        "--isolated",
        "install",
        "--quiet",
        "--only-binary=:all:",
        "--index-url",
        "https://pypi.org/simple",
        "--requirement",
    ])
    .arg(&lock);
    run(pip, 900, cancelled)?;
    if !packages_match(&installed_python, cancelled)? {
        return Err(A1InstallFailure::DependencyVersions);
    }
    // Venv scripts contain absolute paths, so retain this verified directory without renaming.
    save_cache(&cached, &installed_python)?;
    let _ = temporary.keep();
    Ok(installed_python)
}
