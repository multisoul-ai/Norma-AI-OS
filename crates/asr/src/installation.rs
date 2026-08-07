use crate::{AsrModelDescriptor, AsrModelId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

pub const INSTALLATION_MANIFEST_FILE: &str = "installation.json";
const INSTALLATION_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InstallationManifest {
    schema_version: u32,
    model_id: AsrModelId,
    artifact_sha256: String,
}

impl InstallationManifest {
    pub fn for_descriptor(descriptor: &AsrModelDescriptor) -> Self {
        Self {
            schema_version: INSTALLATION_MANIFEST_SCHEMA_VERSION,
            model_id: descriptor.id().clone(),
            artifact_sha256: descriptor.artifact().sha256().to_string(),
        }
    }

    pub fn write_to(&self, root: &Path) -> Result<(), AsrInstallationError> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| AsrInstallationError::InvalidManifest(error.to_string()))?;
        fs::write(root.join(INSTALLATION_MANIFEST_FILE), bytes).map_err(|error| {
            AsrInstallationError::Io {
                path: root.join(INSTALLATION_MANIFEST_FILE),
                message: error.to_string(),
            }
        })
    }

    fn read_from(root: &Path) -> Result<Self, AsrInstallationError> {
        let path = root.join(INSTALLATION_MANIFEST_FILE);
        let bytes = fs::read(&path).map_err(|error| AsrInstallationError::Io {
            path: path.clone(),
            message: error.to_string(),
        })?;
        serde_json::from_slice(&bytes)
            .map_err(|error| AsrInstallationError::InvalidManifest(error.to_string()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableAsrModel {
    descriptor: AsrModelDescriptor,
    root: PathBuf,
}

impl AvailableAsrModel {
    pub fn descriptor(&self) -> &AsrModelDescriptor {
        &self.descriptor
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, Eq, PartialEq, thiserror::Error)]
pub enum AsrInstallationError {
    #[error("application data directory is unavailable")]
    ApplicationDataUnavailable,
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
    #[error("required model file is missing: {0}")]
    MissingRequiredFile(PathBuf),
    #[error(
        "model {model_id} requires an installation update because {missing_file} is missing; open Voice Settings and click Retry to reinstall the verified model archive"
    )]
    UpgradeRequired {
        model_id: String,
        missing_file: PathBuf,
    },
    #[error("model archive SHA-256 does not match the catalog")]
    ArchiveHashMismatch,
    #[error("shared support artifact SHA-256 does not match the catalog")]
    SupportArtifactHashMismatch,
    #[error("invalid model archive: {0}")]
    InvalidArchive(String),
    #[error("model is already installed at {0}")]
    AlreadyInstalled(PathBuf),
    #[error("selected model cannot be removed: {0}")]
    SelectedModel(String),
}

pub fn remove_installation(
    root: &Path,
    descriptor: &AsrModelDescriptor,
    selected_model: Option<&AsrModelId>,
) -> Result<(), AsrInstallationError> {
    if selected_model == Some(descriptor.id()) {
        return Err(AsrInstallationError::SelectedModel(
            descriptor.id().as_str().to_string(),
        ));
    }
    fs::remove_dir_all(root).map_err(|error| AsrInstallationError::Io {
        path: root.to_path_buf(),
        message: error.to_string(),
    })
}

pub fn install_model_archive(
    descriptor: &AsrModelDescriptor,
    archive_path: &Path,
    model_root: &Path,
) -> Result<AvailableAsrModel, AsrInstallationError> {
    let actual_sha256 = sha256_file(archive_path)?;
    if actual_sha256 != descriptor.artifact().sha256() {
        return Err(AsrInstallationError::ArchiveHashMismatch);
    }

    fs::create_dir_all(model_root).map_err(|error| AsrInstallationError::Io {
        path: model_root.to_path_buf(),
        message: error.to_string(),
    })?;
    let final_root = model_root.join(descriptor.id().as_str());
    if final_root.exists() {
        return Err(AsrInstallationError::AlreadyInstalled(final_root));
    }

    let temp = tempfile::Builder::new()
        .prefix(&format!(".{}-partial-", descriptor.id().as_str()))
        .tempdir_in(model_root)
        .map_err(|error| AsrInstallationError::Io {
            path: model_root.to_path_buf(),
            message: error.to_string(),
        })?;
    extract_required_files(descriptor, archive_path, temp.path())?;
    InstallationManifest::for_descriptor(descriptor).write_to(temp.path())?;
    fs::rename(temp.path(), &final_root).map_err(|error| AsrInstallationError::Io {
        path: final_root.clone(),
        message: error.to_string(),
    })?;
    validate_installation(&final_root, descriptor)
}

pub(crate) fn sha256_file(path: &Path) -> Result<String, AsrInstallationError> {
    let mut file = File::open(path).map_err(|error| AsrInstallationError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher).map_err(|error| AsrInstallationError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_required_files(
    descriptor: &AsrModelDescriptor,
    archive_path: &Path,
    destination: &Path,
) -> Result<(), AsrInstallationError> {
    let archive_file = File::open(archive_path).map_err(|error| AsrInstallationError::Io {
        path: archive_path.to_path_buf(),
        message: error.to_string(),
    })?;
    let decoder = bzip2::read::BzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let entries = archive
        .entries()
        .map_err(|error| AsrInstallationError::InvalidArchive(error.to_string()))?;
    for entry_result in entries {
        let mut entry = entry_result
            .map_err(|error| AsrInstallationError::InvalidArchive(error.to_string()))?;
        let entry_path = entry
            .path()
            .map_err(|error| AsrInstallationError::InvalidArchive(error.to_string()))?;
        let Some(file_name) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if descriptor
            .artifact()
            .required_files()
            .iter()
            .any(|required| required == file_name)
        {
            let output = destination.join(file_name);
            entry
                .unpack(&output)
                .map_err(|error| AsrInstallationError::Io {
                    path: output,
                    message: error.to_string(),
                })?;
        }
    }
    for required_file in descriptor.artifact().required_files() {
        let path = destination.join(required_file);
        if !path.is_file() {
            return Err(AsrInstallationError::MissingRequiredFile(path));
        }
    }
    Ok(())
}

pub fn validate_installation(
    root: &Path,
    descriptor: &AsrModelDescriptor,
) -> Result<AvailableAsrModel, AsrInstallationError> {
    let manifest = InstallationManifest::read_from(root)?;
    if manifest.schema_version != INSTALLATION_MANIFEST_SCHEMA_VERSION {
        return Err(AsrInstallationError::SchemaMismatch {
            expected: INSTALLATION_MANIFEST_SCHEMA_VERSION,
            actual: manifest.schema_version,
        });
    }
    if &manifest.model_id != descriptor.id() {
        return Err(AsrInstallationError::ModelIdMismatch {
            expected: descriptor.id().as_str().to_string(),
            actual: manifest.model_id.as_str().to_string(),
        });
    }
    if manifest.artifact_sha256 != descriptor.artifact().sha256() {
        return Err(AsrInstallationError::ArtifactHashMismatch);
    }
    for required_file in descriptor.artifact().required_files() {
        let path = root.join(required_file);
        if !path.is_file() {
            if is_legacy_bilingual_bpe_upgrade(root, descriptor, required_file) {
                return Err(AsrInstallationError::UpgradeRequired {
                    model_id: descriptor.id().as_str().to_string(),
                    missing_file: path,
                });
            }
            return Err(AsrInstallationError::MissingRequiredFile(path));
        }
    }
    Ok(AvailableAsrModel {
        descriptor: descriptor.clone(),
        root: root.to_path_buf(),
    })
}

fn is_legacy_bilingual_bpe_upgrade(
    root: &Path,
    descriptor: &AsrModelDescriptor,
    missing_file: &str,
) -> bool {
    descriptor.id().as_str() == crate::ZIPFORMER_BILINGUAL_MODEL_ID
        && missing_file == "bpe.vocab"
        && descriptor
            .artifact()
            .required_files()
            .iter()
            .filter(|required| required.as_str() != "bpe.vocab")
            .all(|required| root.join(required).is_file())
}
