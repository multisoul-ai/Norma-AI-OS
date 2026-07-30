use super::{AgentProviderKind, VoiceSettingsField, VoiceSettingsState};
use crate::settings::voice::VoiceRequirementState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ProviderDiscoveryStatus {
    Idle,
    Connecting,
    Models(Vec<String>),
    ManualRequired(String),
    ManualValidated(String),
    Failed(String),
}

impl VoiceSettingsState {
    pub(crate) fn provider_discovery(&self) -> &ProviderDiscoveryStatus {
        &self.provider_discovery
    }

    pub(crate) fn begin_provider_discovery(&mut self) -> u64 {
        self.provider_discovery_generation = self.provider_discovery_generation.wrapping_add(1);
        self.provider_discovery = ProviderDiscoveryStatus::Connecting;
        self.provider_requirement = VoiceRequirementState::Installing {
            completed: 0,
            total: 1,
        };
        self.provider_discovery_generation
    }

    pub(crate) fn accepts_provider_discovery_generation(&self, generation: u64) -> bool {
        self.provider_discovery_generation == generation
    }

    pub(crate) fn record_provider_models(&mut self, mut models: Vec<String>) {
        models.retain(|model| !model.trim().is_empty());
        models.sort();
        models.dedup();
        if models.is_empty() {
            self.record_provider_discovery_failure(
                "Provider returned no selectable models; enter a model manually.",
            );
            return;
        }
        self.provider_requirement = VoiceRequirementState::Missing;
        self.provider_discovery = ProviderDiscoveryStatus::Models(models);
    }

    pub(crate) fn record_provider_discovery_failure(&mut self, error: impl Into<String>) {
        self.provider_discovery = ProviderDiscoveryStatus::Failed(error.into());
        self.provider_requirement = VoiceRequirementState::Failed;
    }

    pub(crate) fn record_provider_catalog_error(&mut self, error: agent::ProviderCatalogError) {
        let allows_manual = error.allows_manual_model();
        let preserve_completed_configuration =
            error.is_temporary() && self.completed_agent_provider_configuration_is_present();
        let message = error.to_string();
        self.provider_discovery = if allows_manual {
            ProviderDiscoveryStatus::ManualRequired(message)
        } else {
            ProviderDiscoveryStatus::Failed(message)
        };
        self.provider_requirement = if preserve_completed_configuration {
            VoiceRequirementState::Ready
        } else {
            VoiceRequirementState::Failed
        };
    }

    pub(crate) fn record_manual_provider_model_validated(&mut self, model: String) {
        self.provider_discovery = ProviderDiscoveryStatus::ManualValidated(model);
        self.provider_requirement = VoiceRequirementState::Ready;
    }

    pub(crate) fn select_discovered_provider_model(&mut self, model: String) {
        let discovered_models = match &self.provider_discovery {
            ProviderDiscoveryStatus::Models(models) if models.contains(&model) => {
                Some(models.clone())
            }
            _ => None,
        };
        self.begin_edit_with_value(VoiceSettingsField::Model, model);
        let _ = self.commit_edit_value(VoiceSettingsField::Model, self.draft.clone());
        if let Some(models) = discovered_models {
            self.provider_discovery = ProviderDiscoveryStatus::Models(models);
            self.provider_requirement = VoiceRequirementState::Missing;
        }
    }

    pub(super) fn reset_provider_discovery(&mut self) {
        self.provider_discovery_generation = self.provider_discovery_generation.wrapping_add(1);
        self.provider_discovery = ProviderDiscoveryStatus::Idle;
    }

    fn completed_agent_provider_configuration_is_present(&self) -> bool {
        if !self.setup_was_completed {
            return false;
        }
        let profile = self.values.active_provider_profile();
        !profile.base_url().trim().is_empty()
            && !profile.model().trim().is_empty()
            && (self.values.agent_provider() != AgentProviderKind::OpenAiCompatible
                || profile.credential_configured())
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/provider_discovery_state_tests.rs"]
mod provider_discovery_state_tests;
