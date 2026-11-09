use super::{check_cancelled, disk, A1InstallFailure, InstallCancellation};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
    cancelled: &impl InstallCancellation,
) -> Result<Vec<u8>, A1InstallFailure> {
    check_cancelled(cancelled)?;
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("PYTHONPATH")
        .env_remove("PYTHONHOME")
        .env_remove("DYLD_LIBRARY_PATH")
        .env_remove("DYLD_FALLBACK_LIBRARY_PATH")
        .env("HF_HUB_OFFLINE", "1")
        .env("TRANSFORMERS_OFFLINE", "1");
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

fn packages_match(
    python: &Path,
    cancelled: &impl InstallCancellation,
) -> Result<bool, A1InstallFailure> {
    if !python.is_absolute() || !python.is_file() {
        return Err(A1InstallFailure::Python);
    }
    let expected: BTreeMap<String, String> = asr::A1_REQUIREMENTS
        .lines()
        .filter_map(|l| l.split_once("=="))
        .map(|(name, version)| (name.into(), version.into()))
        .collect();
    let script="import sys,json,importlib.metadata as m\nimport mlx.core,numpy,sherpa_onnx\nfrom mlx_audio.stt.utils import load_model\ndef version(n):\n try:return m.version(n)\n except m.PackageNotFoundError:return None\nnames=json.loads(sys.argv[1]);print(json.dumps({'version':list(sys.version_info[:2]),'packages':{n:version(n) for n in names}}))";
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

mod managed;

mod remote;

#[cfg(test)]
pub(super) fn prepare(
    root: &Path,
    bundle: &Path,
    cancelled: &impl InstallCancellation,
) -> Result<PathBuf, A1InstallFailure> {
    prepare_with_progress(root, bundle, cancelled, |_, _, _| {})
}

pub(super) fn prepare_cached(
    root: &Path,
    bundle: &Path,
    cancelled: &impl InstallCancellation,
) -> Result<PathBuf, A1InstallFailure> {
    remote::prepare(root, bundle, cancelled, false, |_, _, _| {})
}

pub(super) fn prepare_with_progress(
    root: &Path,
    bundle: &Path,
    cancelled: &impl InstallCancellation,
    progress: impl FnMut(&str, u64, u64),
) -> Result<PathBuf, A1InstallFailure> {
    remote::prepare(root, bundle, cancelled, true, progress)
}
