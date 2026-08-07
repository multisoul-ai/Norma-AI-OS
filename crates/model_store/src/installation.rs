use crate::ModelArtifact;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const INSTALLATION_MANIFEST_FILE: &str = "installation.json";
const MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstallationManifest {
    schema_version: u32,
    model_id: String,
    artifact_sha256: String,
}

impl InstallationManifest {
    fn for_artifact(model_id: &str, artifact: &ModelArtifact) -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            model_id: model_id.to_string(),
            artifact_sha256: artifact.sha256().to_string(),
        }
    }

    fn write_to(&self, root: &Path) -> Result<(), InstallationError> {
        let path = root.join(INSTALLATION_MANIFEST_FILE);
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| InstallationError::InvalidManifest(error.to_string()))?;
        fs::write(&path, bytes).map_err(|error| InstallationError::Io {
            path,
            message: error.to_string(),
        })
    }

    fn read_from(root: &Path) -> Result<Self, InstallationError> {
        let path = root.join(INSTALLATION_MANIFEST_FILE);
        let bytes = fs::read(&path).map_err(|error| InstallationError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|error| InstallationError::InvalidManifest(error.to_string()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableModel {
    root: PathBuf,
}

impl AvailableModel {
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplementalFile {
    source: PathBuf,
    sha256: String,
    destination: PathBuf,
}

impl SupplementalFile {
    pub fn new(
        source: impl Into<PathBuf>,
        sha256: impl Into<String>,
        destination: impl Into<PathBuf>,
    ) -> Self {
        Self {
            source: source.into(),
            sha256: sha256.into(),
            destination: destination.into(),
        }
    }
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum InstallationError {
    #[error("model id cannot be blank")]
    BlankModelId,
    #[error("model id must be a single safe directory name: {0}")]
    UnsafeModelId(String),
    #[error("model installation I/O failed at {path}: {message}")]
    Io { path: PathBuf, message: String },
    #[error("invalid installation manifest: {0}")]
    InvalidManifest(String),
    #[error("installation manifest schema {actual} does not match supported schema {expected}")]
    SchemaMismatch { expected: u32, actual: u32 },
    #[error("installation model id {actual} does not match catalog id {expected}")]
    ModelIdMismatch { expected: String, actual: String },
    #[error("installation artifact hash does not match the catalog")]
    ArtifactHashMismatch,
    #[error("model archive SHA-256 does not match the catalog")]
    ArchiveHashMismatch,
    #[error("required model file is missing: {0}")]
    MissingRequiredFile(PathBuf),
    #[error("invalid model archive: {0}")]
    InvalidArchive(String),
    #[error("model is already installed at {0}")]
    AlreadyInstalled(PathBuf),
    #[error("selected model cannot be removed: {0}")]
    SelectedModel(String),
}

pub fn install_archive(
    model_id: &str,
    artifact: &ModelArtifact,
    archive_path: &Path,
    model_root: &Path,
) -> Result<AvailableModel, InstallationError> {
    install_archive_with_support(model_id, artifact, archive_path, model_root, &[])
}

pub fn install_archive_with_support(
    model_id: &str,
    artifact: &ModelArtifact,
    archive_path: &Path,
    model_root: &Path,
    support_files: &[SupplementalFile],
) -> Result<AvailableModel, InstallationError> {
    validate_model_id(model_id)?;
    if sha256_file(archive_path)? != artifact.sha256() {
        return Err(InstallationError::ArchiveHashMismatch);
    }
    for support in support_files {
        validate_support_file(support)?;
    }
    fs::create_dir_all(model_root).map_err(|error| InstallationError::Io {
        path: model_root.to_path_buf(),
        message: error.to_string(),
    })?;
    let final_root = model_root.join(model_id);
    if final_root.exists() {
        return Err(InstallationError::AlreadyInstalled(final_root));
    }
    let temporary = tempfile::Builder::new()
        .prefix(&format!(".{model_id}-partial-"))
        .tempdir_in(model_root)
        .map_err(|error| InstallationError::Io {
            path: model_root.to_path_buf(),
            message: error.to_string(),
        })?;
    extract_required_files(artifact, archive_path, temporary.path())?;
    for support in support_files {
        copy_support_file(support, temporary.path())?;
    }
    InstallationManifest::for_artifact(model_id, artifact).write_to(temporary.path())?;
    fs::rename(temporary.path(), &final_root).map_err(|error| InstallationError::Io {
        path: final_root.clone(),
        message: error.to_string(),
    })?;
    validate_installation(model_id, artifact, &final_root)
}

pub fn validate_installation(
    model_id: &str,
    artifact: &ModelArtifact,
    root: &Path,
) -> Result<AvailableModel, InstallationError> {
    validate_model_id(model_id)?;
    let manifest = InstallationManifest::read_from(root)?;
    if manifest.schema_version != MANIFEST_SCHEMA_VERSION {
        return Err(InstallationError::SchemaMismatch {
            expected: MANIFEST_SCHEMA_VERSION,
            actual: manifest.schema_version,
        });
    }
    if manifest.model_id != model_id {
        return Err(InstallationError::ModelIdMismatch {
            expected: model_id.to_string(),
            actual: manifest.model_id,
        });
    }
    if manifest.artifact_sha256 != artifact.sha256() {
        return Err(InstallationError::ArtifactHashMismatch);
    }
    for file in artifact.required_files() {
        let path = root.join(file);
        if !path.is_file() {
            return Err(InstallationError::MissingRequiredFile(path));
        }
    }
    Ok(AvailableModel {
        root: root.to_path_buf(),
    })
}

pub fn remove_installation(
    model_id: &str,
    root: &Path,
    selected_model: Option<&str>,
) -> Result<(), InstallationError> {
    if selected_model == Some(model_id) {
        return Err(InstallationError::SelectedModel(model_id.to_string()));
    }
    fs::remove_dir_all(root).map_err(|error| InstallationError::Io {
        path: root.to_path_buf(),
        message: error.to_string(),
    })
}

fn validate_model_id(model_id: &str) -> Result<(), InstallationError> {
    let trimmed = model_id.trim();
    if trimmed.is_empty() {
        return Err(InstallationError::BlankModelId);
    }
    let mut components = Path::new(trimmed).components();
    let safe_name = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && trimmed == model_id
        && !trimmed.contains(['/', '\\']);
    if !safe_name {
        return Err(InstallationError::UnsafeModelId(model_id.to_string()));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, InstallationError> {
    let mut file = File::open(path).map_err(|error| InstallationError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).map_err(|error| InstallationError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_support_file(support: &SupplementalFile) -> Result<(), InstallationError> {
    if support.destination.is_absolute()
        || support.destination.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(InstallationError::InvalidArchive(format!(
            "unsafe support destination: {}",
            support.destination.display()
        )));
    }
    if sha256_file(&support.source)? != support.sha256 {
        return Err(InstallationError::ArchiveHashMismatch);
    }
    Ok(())
}

fn copy_support_file(
    support: &SupplementalFile,
    destination_root: &Path,
) -> Result<(), InstallationError> {
    let destination = destination_root.join(&support.destination);
    let parent = destination.parent().ok_or_else(|| {
        InstallationError::InvalidArchive("support destination has no parent directory".to_string())
    })?;
    fs::create_dir_all(parent).map_err(|error| InstallationError::Io {
        path: parent.to_path_buf(),
        message: error.to_string(),
    })?;
    fs::copy(&support.source, &destination).map_err(|error| InstallationError::Io {
        path: destination,
        message: error.to_string(),
    })?;
    Ok(())
}

fn extract_required_files(
    artifact: &ModelArtifact,
    archive_path: &Path,
    destination: &Path,
) -> Result<(), InstallationError> {
    let archive_file = File::open(archive_path).map_err(|error| InstallationError::Io {
        path: archive_path.to_path_buf(),
        message: error.to_string(),
    })?;
    let decoder = bzip2::read::BzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| InstallationError::InvalidArchive(error.to_string()))?;
    for entry in entries {
        let mut entry =
            entry.map_err(|error| InstallationError::InvalidArchive(error.to_string()))?;
        let entry_path = entry
            .path()
            .map_err(|error| InstallationError::InvalidArchive(error.to_string()))?;
        let relative = archive_relative_path(&entry_path)?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let output = destination.join(relative);
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(&output).map_err(|error| InstallationError::Io {
                path: output,
                message: error.to_string(),
            })?;
            continue;
        }
        if !entry.header().entry_type().is_file() {
            return Err(InstallationError::InvalidArchive(format!(
                "unsupported archive entry type at {}",
                entry_path.display()
            )));
        }
        let parent = output.parent().ok_or_else(|| {
            InstallationError::InvalidArchive("archive entry has no parent directory".to_string())
        })?;
        fs::create_dir_all(parent).map_err(|error| InstallationError::Io {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
        entry
            .unpack(&output)
            .map_err(|error| InstallationError::Io {
                path: output,
                message: error.to_string(),
            })?;
    }
    for required in artifact.required_files() {
        let path = destination.join(required);
        if !path.is_file() {
            return Err(InstallationError::MissingRequiredFile(path));
        }
    }
    Ok(())
}

fn archive_relative_path(path: &Path) -> Result<PathBuf, InstallationError> {
    let mut components = path.components();
    let _top_level = components.next().ok_or_else(|| {
        InstallationError::InvalidArchive("archive entry path is empty".to_string())
    })?;
    let mut relative = PathBuf::new();
    for component in components {
        match component {
            Component::Normal(value) => relative.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(InstallationError::InvalidArchive(format!(
                    "unsafe archive entry path: {}",
                    path.display()
                )));
            }
        }
    }
    Ok(relative)
}
