use super::{InstallerError, OrchestrationInstaller};
use crate::orchestration::file_ops::atomic_write;
use crate::orchestration::model::{ComponentState, OrchestrationProvider};
use crate::orchestration::skill::{
    managed_marker, managed_version, skill_source, ManagedSkillKind,
};
use std::fs;
use std::path::{Path, PathBuf};

impl OrchestrationInstaller {
    pub(super) fn inspect_skill(&self, provider: OrchestrationProvider) -> ComponentState {
        let Some(paths) = self.skill_paths(provider) else {
            return ComponentState::Unsupported {
                reason: "Provider does not support orchestration".to_string(),
            };
        };
        combine_skill_states(
            paths
                .iter()
                .map(|(kind, path)| self.inspect_skill_file(provider, *kind, path))
                .collect(),
            &self.required_version,
        )
    }

    pub(super) fn install_skills_if_owned(
        &self,
        provider: OrchestrationProvider,
    ) -> Result<(), InstallerError> {
        let paths = self
            .skill_paths(provider)
            .ok_or(InstallerError::UnsupportedProvider)?;
        for (kind, path) in paths {
            let state = self.inspect_skill_file(provider, kind, &path);
            if matches!(
                state,
                ComponentState::Ready { .. } | ComponentState::Conflict { .. }
            ) {
                continue;
            }
            let source = skill_source(
                kind,
                provider,
                &self.required_version,
                self.paths.managed_cli_path().as_path(),
            );
            atomic_write(&path, source.as_bytes(), 0o644)?;
        }
        Ok(())
    }

    pub(super) fn remove_managed_skills(
        &self,
        provider: OrchestrationProvider,
    ) -> Result<(), InstallerError> {
        let paths = self
            .skill_paths(provider)
            .ok_or(InstallerError::UnsupportedProvider)?;
        for (kind, path) in paths {
            if fs::read_to_string(&path).is_ok_and(|source| source.contains(managed_marker(kind))) {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    fn inspect_skill_file(
        &self,
        provider: OrchestrationProvider,
        kind: ManagedSkillKind,
        path: &Path,
    ) -> ComponentState {
        let source = match fs::read_to_string(path) {
            Ok(source) => source,
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
        let Some(installed_version) = managed_version(&source, kind) else {
            return ComponentState::Conflict {
                path: path.to_path_buf(),
                reason: "Existing file is not managed by Norma OS.".to_string(),
            };
        };
        let expected_source = skill_source(
            kind,
            provider,
            &self.required_version,
            self.paths.managed_cli_path().as_path(),
        );
        if installed_version == self.required_version && source == expected_source {
            ComponentState::Ready {
                version: self.required_version.clone(),
            }
        } else {
            ComponentState::Outdated {
                installed_version: installed_version.to_string(),
                required_version: self.required_version.clone(),
            }
        }
    }

    fn skill_paths(
        &self,
        provider: OrchestrationProvider,
    ) -> Option<[(ManagedSkillKind, PathBuf); 3]> {
        Some([
            (
                ManagedSkillKind::Orchestration,
                self.paths.skill_path(provider)?,
            ),
            (
                ManagedSkillKind::Presentation,
                self.paths.presentation_skill_path(provider)?,
            ),
            (
                ManagedSkillKind::AskNorma,
                self.paths.ask_norma_skill_path(provider)?,
            ),
        ])
    }
}

fn combine_skill_states(states: Vec<ComponentState>, required_version: &str) -> ComponentState {
    if states
        .iter()
        .all(|state| matches!(state, ComponentState::Ready { .. }))
    {
        return ComponentState::Ready {
            version: required_version.to_string(),
        };
    }
    for state in &states {
        if matches!(state, ComponentState::Conflict { .. }) {
            return state.clone();
        }
    }
    for state in &states {
        if matches!(state, ComponentState::Failed { .. }) {
            return state.clone();
        }
    }
    for state in &states {
        if matches!(state, ComponentState::Outdated { .. }) {
            return state.clone();
        }
    }
    ComponentState::Missing
}
