use super::{InstallPaths, InstallerError, OrchestrationProvider};
use crate::protocol::ProviderAuthorization;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub(super) const CREDENTIAL_SCHEMA_VERSION: u8 = 1;
pub(super) const CREDENTIAL_MANAGER: &str = "Norma OS";

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct ProviderCredential {
    pub(super) managed_by: String,
    pub(super) provider: String,
    pub(super) token: String,
    pub(super) schema_version: u8,
    pub(super) installed_version: String,
}

pub(super) enum CredentialReadError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Unmanaged(String),
}

impl From<CredentialReadError> for InstallerError {
    fn from(error: CredentialReadError) -> Self {
        match error {
            CredentialReadError::Io(error) => Self::Io(error),
            CredentialReadError::Json(error) => Self::Json(error),
            CredentialReadError::Unmanaged(message) => Self::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message,
            )),
        }
    }
}

pub fn load_provider_authorization(
    paths: &InstallPaths,
    provider: OrchestrationProvider,
) -> Result<Option<ProviderAuthorization>, InstallerError> {
    let Some(credential) = read_credential(paths.credential_path(provider).as_path())? else {
        return Ok(None);
    };
    if credential.provider != provider.slug() {
        return Err(InstallerError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "credential belongs to a different Provider",
        )));
    }
    Ok(Some(ProviderAuthorization {
        provider: provider.profile(),
        token: credential.token,
    }))
}

pub(super) fn read_credential(
    path: &Path,
) -> Result<Option<ProviderCredential>, CredentialReadError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(CredentialReadError::Io(error)),
    };
    let credential: ProviderCredential =
        serde_json::from_slice(&bytes).map_err(CredentialReadError::Json)?;
    if credential.managed_by != CREDENTIAL_MANAGER {
        return Err(CredentialReadError::Unmanaged(
            "Existing credential is not managed by Norma OS.".to_string(),
        ));
    }
    Ok(Some(credential))
}
