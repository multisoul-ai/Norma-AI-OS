use super::{file_hash, packages_match, save_cache};
use crate::settings::voice::a1_install::{
    check_cancelled, disk, A1InstallFailure, InstallCancellation,
};
use model_store::VerifiedFile;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineFile {
    path: String,
    bytes: u64,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineManifest {
    schema: u32,
    minimum_macos_major: u32,
    archive_bytes: u64,
    archive_sha256: String,
    target: String,
    python: String,
    requirements_sha256: String,
    files: Vec<EngineFile>,
}

pub(super) fn prepare(
    root: &Path,
    bundle: &Path,
    cancelled: &impl InstallCancellation,
) -> Result<PathBuf, A1InstallFailure> {
    check_cancelled(cancelled)?;
    let manifest = bundle.join("engine.json");
    let metadata = fs::symlink_metadata(&manifest).map_err(|_| A1InstallFailure::EngineMissing)?;
    if !metadata.is_file() || metadata.len() > 16 * 1024 * 1024 {
        return Err(A1InstallFailure::EngineIntegrity);
    }
    let bytes = fs::read(&manifest).map_err(|e| disk(&manifest, e))?;
    let specification: EngineManifest =
        serde_json::from_slice(&bytes).map_err(|_| A1InstallFailure::EngineIntegrity)?;
    if specification.schema != 1
        || specification.target != "aarch64-apple-darwin"
        || !cfg!(all(target_os = "macos", target_arch = "aarch64"))
        || specification.requirements_sha256
            != format!("{:x}", Sha256::digest(asr::A1_REQUIREMENTS))
    {
        return Err(A1InstallFailure::EngineIncompatible);
    }
    let version = std::process::Command::new("/usr/bin/sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|version| version.split('.').next()?.parse::<u32>().ok());
    if version.is_none_or(|major| major < specification.minimum_macos_major) {
        return Err(A1InstallFailure::EngineIncompatible);
    }
    // No user-selected interpreters, PATH lookup, environment overrides, venv or pip.
    if specification.python != "python/bin/python3.12"
        || !specification
            .files
            .iter()
            .any(|f| f.path == specification.python)
    {
        return Err(A1InstallFailure::EngineIntegrity);
    }
    let files: Vec<_> = specification
        .files
        .iter()
        .map(|f| VerifiedFile::new(&f.path, f.bytes, &f.sha256))
        .collect();
    let engines = root.join("a1/engines");
    let cached = root.join("a1/python-runtime.json");
    let prefix = format!("engine-{:x}", Sha256::digest(&bytes));
    if let Ok(bytes) = fs::read(&cached) {
        if let Ok(python) = serde_json::from_slice::<PathBuf>(&bytes) {
            if let Some(installed) = python.ancestors().nth(3) {
                if installed.parent() == Some(engines.as_path()) {
                    if let Some(id) = installed.file_name().and_then(|n| n.to_str()) {
                        if python == installed.join(&specification.python)
                            && id.starts_with(&format!("{prefix}-"))
                            && model_store::validate_file_contents(&files, installed).is_ok()
                        {
                            check_cancelled(cancelled)?;
                            match packages_match(&python, cancelled) {
                                Ok(true) => return Ok(python),
                                Err(A1InstallFailure::Cancelled) => {
                                    return Err(A1InstallFailure::Cancelled)
                                }
                                _ => {} // Damaged executable/permissions: extract a new immutable installation.
                            }
                        }
                    }
                }
            }
        }
    }
    check_cancelled(cancelled)?;
    let id = format!("{prefix}-{}", uuid::Uuid::new_v4());
    let archive = bundle.join("engine.tar.bz2");
    if fs::metadata(&archive)
        .map_err(|_| A1InstallFailure::EngineMissing)?
        .len()
        != specification.archive_bytes
    {
        return Err(A1InstallFailure::EngineIntegrity);
    }
    let artifact = model_store::ModelArtifact::new(
        "",
        &specification.archive_sha256,
        specification.archive_bytes,
        files.iter().map(|f| f.path.as_str()).collect(),
    );
    let installed =
        model_store::install_archive(&id, &artifact, &archive, &engines).map_err(|error| {
            match error {
                model_store::InstallationError::Io { path, .. } => A1InstallFailure::Disk {
                    file: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into(),
                    code: None,
                },
                _ => A1InstallFailure::EngineIntegrity,
            }
        })?;
    let python = installed.root().join(specification.python);
    let ready = (|| {
        check_cancelled(cancelled)?;
        model_store::validate_file_contents(&files, installed.root())
            .map_err(|_| A1InstallFailure::EngineIntegrity)?;
        if !packages_match(&python, cancelled)? {
            return Err(A1InstallFailure::DependencyVersions);
        }
        let expected = files
            .iter()
            .find(|f| f.path == "python/bin/python3.12")
            .ok_or(A1InstallFailure::EngineIntegrity)?;
        if file_hash(&python)? != expected.sha256 {
            return Err(A1InstallFailure::EngineIntegrity);
        }
        check_cancelled(cancelled)?;
        save_cache(&cached, &python)
    })();
    if ready.is_err() {
        let _ = fs::remove_dir_all(installed.root());
    }
    ready?;
    Ok(python)
}
