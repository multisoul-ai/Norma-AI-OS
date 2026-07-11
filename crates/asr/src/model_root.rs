use crate::AsrInstallationError;
use std::path::{Path, PathBuf};

pub fn model_installation_root(application_data_root: &Path) -> PathBuf {
    application_data_root.join("models")
}

pub fn default_model_installation_root() -> Result<PathBuf, AsrInstallationError> {
    let data_dir = dirs::data_dir().ok_or(AsrInstallationError::ApplicationDataUnavailable)?;
    Ok(model_installation_root(&data_dir.join("Norma OS")))
}
