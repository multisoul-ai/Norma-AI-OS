use crate::{TtsModelDescriptor, TtsModelId};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AvailableTtsModel {
    descriptor: TtsModelDescriptor,
    root: PathBuf,
}

impl AvailableTtsModel {
    pub fn descriptor(&self) -> &TtsModelDescriptor {
        &self.descriptor
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TtsInstallationError {
    #[error("application data directory is unavailable")]
    ApplicationDataUnavailable,
    #[error(transparent)]
    Store(#[from] model_store::InstallationError),
    #[error("required TTS support file is missing: {0}")]
    MissingSupportFile(PathBuf),
    #[error("TTS support artifact count mismatch: expected {expected}, got {actual}")]
    SupportArtifactCountMismatch { expected: usize, actual: usize },
}

pub fn model_installation_root(application_data_root: &Path) -> PathBuf {
    application_data_root.join("models").join("tts")
}

pub fn default_model_installation_root() -> Result<PathBuf, TtsInstallationError> {
    let data_dir = dirs::data_dir().ok_or(TtsInstallationError::ApplicationDataUnavailable)?;
    Ok(model_installation_root(&data_dir.join("Norma OS")))
}

pub fn validate_installation(
    root: &Path,
    descriptor: &TtsModelDescriptor,
) -> Result<AvailableTtsModel, TtsInstallationError> {
    model_store::validate_installation(
        descriptor.id().as_str(),
        descriptor.package().archive(),
        root,
    )?;
    for support in descriptor.package().support_artifacts() {
        let path = root.join(support.destination());
        if !path.is_file() {
            return Err(TtsInstallationError::MissingSupportFile(path));
        }
    }
    Ok(AvailableTtsModel {
        descriptor: descriptor.clone(),
        root: root.to_path_buf(),
    })
}

pub fn install_downloaded_package(
    descriptor: &TtsModelDescriptor,
    archive_path: &Path,
    support_paths: &[PathBuf],
    model_root: &Path,
) -> Result<AvailableTtsModel, TtsInstallationError> {
    if support_paths.len() != descriptor.package().support_artifacts().len() {
        return Err(TtsInstallationError::SupportArtifactCountMismatch {
            expected: descriptor.package().support_artifacts().len(),
            actual: support_paths.len(),
        });
    }
    let support_files = descriptor
        .package()
        .support_artifacts()
        .iter()
        .zip(support_paths)
        .map(|(artifact, source)| {
            model_store::SupplementalFile::new(source, artifact.sha256(), artifact.destination())
        })
        .collect::<Vec<_>>();
    model_store::install_archive_with_support(
        descriptor.id().as_str(),
        descriptor.package().archive(),
        archive_path,
        model_root,
        &support_files,
    )?;
    validate_installation(&model_root.join(descriptor.id().as_str()), descriptor)
}

pub fn remove_installation(
    root: &Path,
    descriptor: &TtsModelDescriptor,
    selected_model: Option<&TtsModelId>,
) -> Result<(), TtsInstallationError> {
    model_store::remove_installation(
        descriptor.id().as_str(),
        root,
        selected_model.map(TtsModelId::as_str),
    )?;
    Ok(())
}
