use crate::installation::{validate_model_id, InstallationManifest};
use crate::{validate_installation, AvailableModel, InstallationError, ModelArtifact};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Release-pinned file metadata. Network access belongs to the caller.
#[derive(Clone, Debug, Serialize)]
pub struct VerifiedFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

impl VerifiedFile {
    pub fn new(path: impl Into<String>, bytes: u64, sha256: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            bytes,
            sha256: sha256.into(),
        }
    }
}

fn io_error(path: &Path, error: io::Error) -> InstallationError {
    InstallationError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

fn artifact(files: &[VerifiedFile]) -> Result<ModelArtifact, InstallationError> {
    let mut names = std::collections::BTreeSet::new();
    if files.is_empty()
        || files.iter().any(|f| {
            f.path.is_empty()
                || Path::new(&f.path).components().next().is_some_and(|c| {
                    c.as_os_str().to_str().is_some_and(|name| {
                        name.eq_ignore_ascii_case(crate::INSTALLATION_MANIFEST_FILE)
                    })
                })
                || Path::new(&f.path)
                    .components()
                    .any(|c| !matches!(c, Component::Normal(_)))
                || f.path.contains('\\')
                || f.sha256.len() != 64
                || !f.sha256.bytes().all(|b| b.is_ascii_hexdigit())
                || !names.insert(f.path.as_str())
        })
    {
        return Err(InstallationError::InvalidManifest(
            "unsafe or duplicate model file".into(),
        ));
    }
    let bytes =
        serde_json::to_vec(files).map_err(|e| InstallationError::InvalidManifest(e.to_string()))?;
    let total = files
        .iter()
        .try_fold(0u64, |sum, file| sum.checked_add(file.bytes))
        .ok_or_else(|| {
            InstallationError::InvalidManifest("model file size total overflow".into())
        })?;
    Ok(ModelArtifact::new(
        "",
        format!("{:x}", Sha256::digest(bytes)),
        total,
        files.iter().map(|f| f.path.as_str()).collect(),
    ))
}

fn validate_file(root: &Path, file: &VerifiedFile) -> Result<PathBuf, InstallationError> {
    let mut path = root.to_path_buf();
    // Never follow symlinks, including intermediate directories, into other installations.
    for component in Path::new(&file.path).components() {
        path.push(component);
        let metadata = fs::symlink_metadata(&path).map_err(|e| io_error(&path, e))?;
        if metadata.file_type().is_symlink() {
            return Err(InstallationError::InvalidManifest(
                "model files cannot be symlinks".into(),
            ));
        }
    }
    let metadata = fs::metadata(&path).map_err(|e| io_error(&path, e))?;
    if !metadata.is_file() || metadata.len() != file.bytes {
        return Err(InstallationError::FileIntegrityMismatch(path));
    }
    let mut input = fs::File::open(&path).map_err(|e| io_error(&path, e))?;
    let mut hash = Sha256::new();
    io::copy(&mut input, &mut hash).map_err(|e| io_error(&path, e))?;
    if format!("{:x}", hash.finalize()) != file.sha256 {
        return Err(InstallationError::FileIntegrityMismatch(path));
    }
    Ok(path)
}

/// Verify curated contents independently of the archive's installation manifest.
pub fn validate_file_contents(
    files: &[VerifiedFile],
    root: &Path,
) -> Result<(), InstallationError> {
    artifact(files)?;
    for file in files {
        validate_file(root, file)?;
    }
    Ok(())
}

pub fn validate_verified_files(
    model_id: &str,
    files: &[VerifiedFile],
    root: &Path,
) -> Result<AvailableModel, InstallationError> {
    let artifact = artifact(files)?;
    let installed = validate_installation(model_id, &artifact, root)?;
    validate_file_contents(files, root)?;
    Ok(installed)
}

/// Verify the complete release-pinned set before atomic promotion. Existing installations
/// are immutable: retries use a new ID and only switch the caller's manifest after success.
pub fn install_verified_files(
    model_id: &str,
    files: &[VerifiedFile],
    source: &Path,
    model_root: &Path,
) -> Result<AvailableModel, InstallationError> {
    validate_model_id(model_id)?;
    let artifact = artifact(files)?;
    fs::create_dir_all(model_root).map_err(|e| io_error(model_root, e))?;
    let final_root = model_root.join(model_id);
    if final_root.exists() {
        return Err(InstallationError::AlreadyInstalled(final_root));
    }
    let temporary = tempfile::Builder::new()
        .prefix(".verified-partial-")
        .tempdir_in(model_root)
        .map_err(|e| io_error(model_root, e))?;
    for file in files {
        let input = validate_file(source, file)?;
        let output = temporary.path().join(&file.path);
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent).map_err(|e| io_error(parent, e))?;
        }
        fs::copy(&input, &output).map_err(|e| io_error(&output, e))?;
        validate_file(temporary.path(), file)?;
    }
    InstallationManifest::for_artifact(model_id, &artifact).write_to(temporary.path())?;
    validate_verified_files(model_id, files, temporary.path())?;
    fs::rename(temporary.path(), &final_root).map_err(|e| io_error(&final_root, e))?;
    validate_installation(model_id, &artifact, &final_root)
}
