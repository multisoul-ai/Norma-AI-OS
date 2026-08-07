use super::credential::{
    read_credential, CredentialReadError, ProviderCredential, CREDENTIAL_MANAGER,
    CREDENTIAL_SCHEMA_VERSION,
};
use super::file_ops::{atomic_write, remove_matching_binary, set_mode};
use super::model::{ComponentState, InstallPaths, OrchestrationProvider, ProviderIntegration};
use crate::agent_hook::{
    agent_hooks_installed_for_source, install_hook_config, uninstall_hook_config, AgentHookError,
    OPENCODE_PLUGIN_MARKER,
};
use std::fmt;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

mod reconcile;
mod skill_component;

pub use reconcile::{OrchestrationReconcileFailure, OrchestrationReconcileReport};

pub trait AuthorizationRegistry {
    type Error: fmt::Display;

    fn authorize(
        &mut self,
        provider: OrchestrationProvider,
        token: &str,
        installed_version: &str,
    ) -> Result<(), Self::Error>;

    fn revoke(&mut self, provider: OrchestrationProvider) -> Result<(), Self::Error>;

    fn is_authorized(
        &self,
        provider: OrchestrationProvider,
        token: &str,
    ) -> Result<bool, Self::Error>;
}

#[derive(Debug)]
pub enum InstallerError {
    Io(std::io::Error),
    BinaryInstall {
        source: PathBuf,
        destination: PathBuf,
        error: std::io::Error,
    },
    Json(serde_json::Error),
    Hook(AgentHookError),
    Authorization(String),
    UnsupportedProvider,
}

impl fmt::Display for InstallerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::BinaryInstall {
                source,
                destination,
                error,
            } => write!(
                formatter,
                "failed to install bundled binary from {} to {}: {error}",
                source.display(),
                destination.display()
            ),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::Hook(error) => write!(formatter, "{error}"),
            Self::Authorization(error) => write!(formatter, "{error}"),
            Self::UnsupportedProvider => write!(formatter, "provider is not supported"),
        }
    }
}

impl std::error::Error for InstallerError {}

impl From<std::io::Error> for InstallerError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for InstallerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<AgentHookError> for InstallerError {
    fn from(error: AgentHookError) -> Self {
        Self::Hook(error)
    }
}

#[derive(Clone, Debug)]
pub struct OrchestrationInstaller {
    paths: InstallPaths,
    required_version: String,
}

impl OrchestrationInstaller {
    pub fn new(paths: InstallPaths, required_version: impl Into<String>) -> Self {
        Self {
            paths,
            required_version: required_version.into(),
        }
    }

    pub fn paths(&self) -> &InstallPaths {
        &self.paths
    }

    pub fn inspect<R: AuthorizationRegistry>(
        &self,
        provider: OrchestrationProvider,
        registry: &R,
    ) -> ProviderIntegration {
        if provider == OrchestrationProvider::Grok {
            return ProviderIntegration::unsupported(provider);
        }
        ProviderIntegration::from_components(
            provider,
            self.inspect_skill(provider),
            self.inspect_binary(
                self.paths.managed_cli_path().as_path(),
                self.paths.bundled_cli.as_path(),
            ),
            self.inspect_hook(provider),
            self.inspect_authorization(provider, registry),
        )
    }

    pub fn install_or_repair<R: AuthorizationRegistry>(
        &self,
        provider: OrchestrationProvider,
        registry: &mut R,
    ) -> Result<ProviderIntegration, InstallerError> {
        if provider == OrchestrationProvider::Grok {
            return Err(InstallerError::UnsupportedProvider);
        }
        let before = self.inspect(provider, registry);
        self.install_skills_if_owned(provider)?;
        if !matches!(before.cli_component, ComponentState::Ready { .. }) {
            self.install_binary(
                self.paths.bundled_cli.as_path(),
                self.paths.managed_cli_path().as_path(),
            )?;
        }
        if !matches!(
            before.hook_component,
            ComponentState::Ready { .. } | ComponentState::Conflict { .. }
        ) {
            self.install_binary(
                self.paths.bundled_hook.as_path(),
                self.paths.managed_hook_path().as_path(),
            )?;
            install_hook_config(
                provider
                    .hook_target()
                    .expect("supported orchestration provider has a hook target"),
                self.paths.managed_hook_path().as_path(),
                &self.paths.hook_paths,
            )?;
        }
        if !matches!(before.authorization_component, ComponentState::Ready { .. }) {
            let credential = self.install_or_read_credential(provider)?;
            registry
                .authorize(provider, &credential.token, &self.required_version)
                .map_err(|error| InstallerError::Authorization(error.to_string()))?;
        }
        Ok(self.inspect(provider, registry))
    }

    pub fn uninstall<R: AuthorizationRegistry>(
        &self,
        provider: OrchestrationProvider,
        registry: &mut R,
    ) -> Result<ProviderIntegration, InstallerError> {
        if provider == OrchestrationProvider::Grok {
            return Err(InstallerError::UnsupportedProvider);
        }
        registry
            .revoke(provider)
            .map_err(|error| InstallerError::Authorization(error.to_string()))?;
        self.remove_managed_skills(provider)?;
        uninstall_hook_config(
            provider
                .hook_target()
                .expect("supported orchestration provider has a hook target"),
            &self.paths.hook_paths,
        )?;
        self.remove_managed_credential(provider)?;
        if !self.has_other_credentials(provider)? {
            remove_matching_binary(
                self.paths.managed_cli_path().as_path(),
                self.paths.bundled_cli.as_path(),
            )?;
            remove_matching_binary(
                self.paths.managed_hook_path().as_path(),
                self.paths.bundled_hook.as_path(),
            )?;
        }
        Ok(self.inspect(provider, registry))
    }

    fn inspect_binary(&self, installed: &Path, bundled: &Path) -> ComponentState {
        let installed_bytes = match fs::read(installed) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return ComponentState::Missing;
            }
            Err(error) => {
                return ComponentState::Failed {
                    error_code: "component_inspection_failed".to_string(),
                    message: error.to_string(),
                };
            }
        };
        match fs::read(bundled) {
            Ok(bundled_bytes) if bundled_bytes == installed_bytes => ComponentState::Ready {
                version: self.required_version.clone(),
            },
            Ok(_) => ComponentState::Outdated {
                installed_version: "unknown".to_string(),
                required_version: self.required_version.clone(),
            },
            Err(error) => ComponentState::Failed {
                error_code: "bundled_component_missing".to_string(),
                message: error.to_string(),
            },
        }
    }

    fn inspect_hook(&self, provider: OrchestrationProvider) -> ComponentState {
        if provider == OrchestrationProvider::OpenCode {
            let path = self.paths.hook_paths.opencode_plugin.clone();
            match fs::read_to_string(&path) {
                Ok(source) if !source.contains(OPENCODE_PLUGIN_MARKER) => {
                    return ComponentState::Conflict {
                        path,
                        reason:
                            "Existing OpenCode plugin at the managed path is not owned by Norma OS."
                                .to_string(),
                    };
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return ComponentState::Failed {
                        error_code: "hook_read_failed".to_string(),
                        message: error.to_string(),
                    };
                }
            }
        }
        let binary = self.inspect_binary(
            self.paths.managed_hook_path().as_path(),
            self.paths.bundled_hook.as_path(),
        );
        if !matches!(binary, ComponentState::Ready { .. }) {
            return binary;
        }
        let source = provider
            .hook_source()
            .expect("supported orchestration provider has a hook source");
        if agent_hooks_installed_for_source(source, &self.paths.hook_paths) {
            ComponentState::Ready {
                version: self.required_version.clone(),
            }
        } else {
            ComponentState::Missing
        }
    }

    fn inspect_authorization<R: AuthorizationRegistry>(
        &self,
        provider: OrchestrationProvider,
        registry: &R,
    ) -> ComponentState {
        let path = self.paths.credential_path(provider);
        let credential = match read_credential(path.as_path()) {
            Ok(Some(credential)) => credential,
            Ok(None) => return ComponentState::Missing,
            Err(CredentialReadError::Unmanaged(reason)) => {
                return ComponentState::Conflict { path, reason };
            }
            Err(CredentialReadError::Io(error)) => {
                return ComponentState::Failed {
                    error_code: "credential_read_failed".to_string(),
                    message: error.to_string(),
                };
            }
            Err(CredentialReadError::Json(error)) => {
                return ComponentState::Conflict {
                    path,
                    reason: error.to_string(),
                };
            }
        };
        if credential.provider != provider.slug() {
            return ComponentState::Conflict {
                path,
                reason: "Credential belongs to a different Provider.".to_string(),
            };
        }
        let credential_mode = match fs::metadata(&path) {
            Ok(metadata) => metadata.permissions().mode() & 0o777,
            Err(error) => {
                return ComponentState::Failed {
                    error_code: "credential_metadata_failed".to_string(),
                    message: error.to_string(),
                };
            }
        };
        if credential_mode != 0o600 {
            return ComponentState::Failed {
                error_code: "credential_permissions_insecure".to_string(),
                message: format!(
                    "Provider credential permissions are {credential_mode:#o}; required 0o600."
                ),
            };
        }
        match registry.is_authorized(provider, &credential.token) {
            Ok(true) => ComponentState::Ready {
                version: credential.installed_version,
            },
            Ok(false) => ComponentState::Failed {
                error_code: "authorization_revoked".to_string(),
                message: "Provider authorization is missing or revoked.".to_string(),
            },
            Err(error) => ComponentState::Failed {
                error_code: "authorization_check_failed".to_string(),
                message: error.to_string(),
            },
        }
    }

    fn install_binary(&self, bundled: &Path, installed: &Path) -> Result<(), InstallerError> {
        let binary_error = |error| InstallerError::BinaryInstall {
            source: bundled.to_path_buf(),
            destination: installed.to_path_buf(),
            error,
        };
        let bundled_bytes = fs::read(bundled).map_err(binary_error)?;
        if fs::read(installed).is_ok_and(|bytes| bytes == bundled_bytes) {
            return Ok(());
        }
        atomic_write(installed, &bundled_bytes, 0o755).map_err(binary_error)?;
        Ok(())
    }

    fn install_or_read_credential(
        &self,
        provider: OrchestrationProvider,
    ) -> Result<ProviderCredential, InstallerError> {
        let path = self.paths.credential_path(provider);
        match read_credential(path.as_path()) {
            Ok(Some(credential)) if credential.provider == provider.slug() => {
                set_mode(path.as_path(), 0o600)?;
                return Ok(credential);
            }
            Ok(Some(_))
            | Err(CredentialReadError::Unmanaged(_))
            | Err(CredentialReadError::Json(_)) => {
                return Err(InstallerError::Io(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("credential conflict at {}", path.display()),
                )));
            }
            Err(CredentialReadError::Io(error)) => return Err(InstallerError::Io(error)),
            Ok(None) => {}
        }
        let credential = ProviderCredential {
            managed_by: CREDENTIAL_MANAGER.to_string(),
            provider: provider.slug().to_string(),
            token: generate_token()?,
            schema_version: CREDENTIAL_SCHEMA_VERSION,
            installed_version: self.required_version.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&credential)?;
        atomic_write(path.as_path(), &bytes, 0o600)?;
        Ok(credential)
    }

    fn remove_managed_credential(
        &self,
        provider: OrchestrationProvider,
    ) -> Result<(), InstallerError> {
        let path = self.paths.credential_path(provider);
        if read_credential(path.as_path()).is_ok_and(|credential| credential.is_some()) {
            fs::remove_file(path)?;
        }
        Ok(())
    }

    fn has_other_credentials(
        &self,
        provider: OrchestrationProvider,
    ) -> Result<bool, InstallerError> {
        for candidate in OrchestrationProvider::ALL {
            if candidate == provider || candidate == OrchestrationProvider::Grok {
                continue;
            }
            if read_credential(self.paths.credential_path(candidate).as_path())?.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

fn generate_token() -> Result<String, std::io::Error> {
    let mut bytes = [0_u8; 32];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
