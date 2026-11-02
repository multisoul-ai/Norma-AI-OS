#[cfg(test)]
#[path = "../../../tests/unit/voice_a1_download_tests.rs"]
mod tests;
use model_store::VerifiedFile;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
mod runtime;

struct PendingPackage(Option<PathBuf>);

impl PendingPackage {
    fn new(path: PathBuf) -> Self {
        Self(Some(path))
    }

    fn commit(&mut self) {
        self.0 = None;
    }
}

impl Drop for PendingPackage {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum A1InstallProgress {
    Runtime,
    Download {
        file: String,
        completed: u64,
        total: u64,
    },
    Verify,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum A1InstallFailure {
    Cancelled,
    EngineMissing,
    EngineIntegrity,
    EngineIncompatible,
    Python,
    PythonVersion,
    Dependencies,
    DependencyVersions,
    Network(String),
    Http { file: String, status: u16 },
    Disk { file: String, code: Option<i32> },
    Integrity(String),
    Manifest,
}

fn disk(path: &Path, error: std::io::Error) -> A1InstallFailure {
    A1InstallFailure::Disk {
        file: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        code: error.raw_os_error(),
    }
}

pub(crate) trait InstallCancellation {
    fn is_cancelled(&self) -> bool;
}
impl InstallCancellation for AtomicBool {
    fn is_cancelled(&self) -> bool {
        self.load(Ordering::Acquire)
    }
}
impl InstallCancellation for asr::RecognitionCancellation {
    fn is_cancelled(&self) -> bool {
        self.is_cancelled()
    }
}

fn check_cancelled(cancelled: &impl InstallCancellation) -> Result<(), A1InstallFailure> {
    if cancelled.is_cancelled() {
        Err(A1InstallFailure::Cancelled)
    } else {
        Ok(())
    }
}

/// Explicit setup/activation upgrade: retain verified models and atomically adopt the current App engine.
pub(crate) fn ensure_runtime(
    root: &Path,
    cancelled: &impl InstallCancellation,
) -> Result<asr::A1Config, asr::AsrError> {
    let mut config = asr::A1Config::from_model_root(root)?;
    let python = runtime::prepare(root, &engine_bundle(), cancelled)
        .map_err(|e| asr::AsrError::Provider(format!("A1 managed engine: {e:?}")))?;
    if config.python == python {
        config.validate()?;
    } else {
        config.python_sha256 = runtime::file_hash(&python)
            .map_err(|e| asr::AsrError::Provider(format!("A1 managed engine: {e:?}")))?;
        config.python = python;
        config.install_with_gate(root, || {
            check_cancelled(cancelled).map_err(|_| asr::AsrError::SessionClosed)
        })?;
    }
    check_cancelled(cancelled).map_err(|_| asr::AsrError::SessionClosed)?;
    Ok(config)
}

pub(crate) fn engine_bundle() -> PathBuf {
    if let Some(resources) = crate::app::wallpaper_resolvers::bundle_resources_dir() {
        return resources.join("a1-engine");
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/a1-engine")
}

/// No model loading and no provider switch. Publish runtime.json only after every check.
pub(crate) fn install_a1(
    root: &Path,
    bundle: &Path,
    cancelled: &impl InstallCancellation,
    mut progress: impl FnMut(A1InstallProgress),
) -> Result<asr::A1Config, A1InstallFailure> {
    check_cancelled(cancelled)?;
    progress(A1InstallProgress::Runtime);
    let python = runtime::prepare(root, bundle, cancelled)?;
    let artifacts = asr::a1_model_artifacts().map_err(|_| A1InstallFailure::Manifest)?;
    let files: Vec<_> = artifacts
        .iter()
        .map(|a| VerifiedFile::new(a.relative_path(), a.bytes, &a.sha256))
        .collect();
    let package_root = root.join("a1/packages");
    fs::create_dir_all(&package_root).map_err(|e| disk(&package_root, e))?;
    let staged = tempfile::Builder::new()
        .prefix(".download-")
        .tempdir_in(&package_root)
        .map_err(|e| disk(&package_root, e))?;
    let total = artifacts.iter().map(|a| a.bytes).sum();
    let mut completed = 0;
    for (artifact, file) in artifacts.iter().zip(&files) {
        check_cancelled(cancelled)?;
        download_verified_file(
            &artifact.download_url(),
            file,
            staged.path(),
            cancelled,
            |done, _| {
                progress(A1InstallProgress::Download {
                    file: file.path.clone(),
                    completed: completed + done,
                    total,
                });
            },
        )?;
        completed += file.bytes;
    }
    progress(A1InstallProgress::Verify);
    check_cancelled(cancelled)?;
    let id = format!("a1-{}", uuid::Uuid::new_v4());
    let installed = model_store::install_verified_files(&id, &files, staged.path(), &package_root)
        .map_err(|error| match error {
            model_store::InstallationError::FileIntegrityMismatch(path) => {
                A1InstallFailure::Integrity(
                    path.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                )
            }
            model_store::InstallationError::Io { path, .. } => A1InstallFailure::Disk {
                file: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                code: None,
            },
            _ => A1InstallFailure::Manifest,
        })?;
    let mut pending = PendingPackage::new(installed.root().to_owned());
    check_cancelled(cancelled)?;
    let config = asr::A1Config {
        protocol: 1,
        python_sha256: runtime::file_hash(&python)?,
        python,
        zipformer: installed.root().join("zipformer"),
        qwen: installed.root().join("qwen"),
    };
    config
        .install_with_gate(root, || {
            check_cancelled(cancelled)
                .map_err(|_| asr::AsrError::Provider("A1 installation cancelled".into()))
        })
        .map_err(|_| {
            if cancelled.is_cancelled() {
                A1InstallFailure::Cancelled
            } else {
                A1InstallFailure::Manifest
            }
        })?;
    pending.commit();
    Ok(config)
}

/// Runs on a background worker. Bytes are bounded and hashed before publishing the file.
pub(crate) fn download_verified_file(
    url: &str,
    file: &VerifiedFile,
    destination: &Path,
    cancelled: &impl InstallCancellation,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, A1InstallFailure> {
    check_cancelled(cancelled)?;
    let final_path = destination.join(&file.path);
    let parent = final_path.parent().ok_or(A1InstallFailure::Manifest)?;
    fs::create_dir_all(parent).map_err(|e| disk(parent, e))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(|e| disk(parent, e))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| A1InstallFailure::Network(file.path.clone()))?;
    runtime.block_on(async {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .read_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(1800));
        if url.starts_with("http://127.0.0.1:") {
            builder = builder.no_proxy();
        }
        let client = builder
            .build()
            .map_err(|_| A1InstallFailure::Network(file.path.clone()))?;
        let mut response = client
            .get(url)
            .send()
            .await
            .map_err(|_| A1InstallFailure::Network(file.path.clone()))?;
        if !response.status().is_success() {
            return Err(A1InstallFailure::Http {
                file: file.path.clone(),
                status: response.status().as_u16(),
            });
        }
        if response
            .content_length()
            .is_some_and(|length| length != file.bytes)
        {
            return Err(A1InstallFailure::Integrity(file.path.clone()));
        }
        let mut hash = Sha256::new();
        let mut completed = 0u64;
        let mut gate = super::download_progress::DownloadProgressGate::new(file.bytes);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| A1InstallFailure::Network(file.path.clone()))?
        {
            check_cancelled(cancelled)?;
            completed += chunk.len() as u64;
            if completed > file.bytes {
                return Err(A1InstallFailure::Integrity(file.path.clone()));
            }
            temporary
                .write_all(&chunk)
                .map_err(|e| disk(&final_path, e))?;
            hash.update(&chunk);
            if gate.should_publish(completed) {
                progress(completed, file.bytes);
            }
        }
        check_cancelled(cancelled)?;
        if completed != file.bytes || format!("{:x}", hash.finalize()) != file.sha256 {
            return Err(A1InstallFailure::Integrity(file.path.clone()));
        }
        temporary
            .as_file()
            .sync_all()
            .map_err(|e| disk(&final_path, e))?;
        if gate.should_publish_final(completed) {
            progress(completed, file.bytes);
        }
        Ok(())
    })?;
    temporary
        .persist(&final_path)
        .map_err(|e| disk(&final_path, e.error))?;
    Ok(final_path)
}
