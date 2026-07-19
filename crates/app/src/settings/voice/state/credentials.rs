use super::provider_values::{
    AgentProviderKind, KEY_AGENT_OLLAMA_API_KEY, KEY_AGENT_OLLAMA_CREDENTIAL_REF,
    KEY_AGENT_OPENAI_API_KEY, KEY_AGENT_OPENAI_CREDENTIAL_REF,
};
use super::values::KEY_OPENAI_API_KEY;
use super::{PendingVoiceSettingWrite, VoiceSettingsState};
use crate::settings::voice::VoiceRequirementState;

#[derive(Clone)]
pub(super) struct CredentialWriteRollback {
    provider: AgentProviderKind,
    previous: super::provider_values::SavedProviderProfile,
    new_account: String,
}

impl VoiceSettingsState {
    pub(crate) fn credential_refs(&self) -> Vec<(AgentProviderKind, String)> {
        [
            (AgentProviderKind::Ollama, &self.saved.ollama_profile),
            (
                AgentProviderKind::OpenAiCompatible,
                &self.saved.openai_profile,
            ),
        ]
        .into_iter()
        .filter_map(|(provider, profile)| {
            (profile.api_key.is_none())
                .then_some(profile.credential_ref.as_ref())
                .flatten()
                .map(|account| (provider, account.clone()))
        })
        .collect()
    }

    pub(crate) fn apply_loaded_credential(
        &mut self,
        provider: AgentProviderKind,
        account: &str,
        result: Result<Option<String>, String>,
    ) -> bool {
        let profile = match provider {
            AgentProviderKind::Ollama => &mut self.saved.ollama_profile,
            AgentProviderKind::OpenAiCompatible => &mut self.saved.openai_profile,
        };
        if profile.credential_ref.as_deref() != Some(account) || profile.api_key.is_some() {
            return false;
        }
        let fallback = profile.legacy_api_key.take();
        match result {
            Ok(Some(secret)) => {
                profile.api_key = Some(secret);
                if fallback.is_some() {
                    self.queue_plaintext_credential_deletes(provider);
                }
            }
            Ok(None) => {
                if let Some(secret) = fallback {
                    self.save_provider_credential(provider, secret);
                } else {
                    self.clear_missing_credential_reference(provider, account);
                    self.provider_requirement = VoiceRequirementState::Failed;
                }
            }
            Err(error) => {
                if let Some(secret) = fallback {
                    self.save_provider_credential(provider, secret);
                }
                self.report_credential_error(error);
            }
        }
        self.refresh_values();
        self.values.active_provider_profile().api_key().is_some()
    }

    pub(crate) fn report_credential_error(&mut self, error: impl Into<String>) {
        self.status = Some(super::error_status(error));
    }

    pub(crate) fn credential_persistence_pending(&self) -> bool {
        self.credential_write_rollback.is_some()
    }

    pub(crate) fn load_credential(account: &str) -> Result<Option<String>, String> {
        crate::settings::voice::credentials::load(account)
    }

    pub(super) fn queue_legacy_api_key_migrations(&mut self) {
        for provider in [
            AgentProviderKind::Ollama,
            AgentProviderKind::OpenAiCompatible,
        ] {
            let profile = match provider {
                AgentProviderKind::Ollama => &self.saved.ollama_profile,
                AgentProviderKind::OpenAiCompatible => &self.saved.openai_profile,
            };
            if profile.api_key.is_some() && profile.credential_ref.is_none() {
                let secret = profile
                    .api_key
                    .clone()
                    .expect("legacy API key checked above");
                self.save_provider_credential(provider, secret);
            }
        }
    }

    pub(super) fn save_active_provider_credential(&mut self, secret: String) {
        self.save_provider_credential(self.values.agent_provider(), secret);
        self.invalidate_provider_requirement();
    }

    pub(super) fn clear_active_provider_credential(&mut self) {
        let provider = self.values.agent_provider();
        let (account, ref_key, plaintext_key, legacy_key) = match provider {
            AgentProviderKind::Ollama => (
                self.saved.ollama_profile.credential_ref.take(),
                KEY_AGENT_OLLAMA_CREDENTIAL_REF,
                KEY_AGENT_OLLAMA_API_KEY,
                None,
            ),
            AgentProviderKind::OpenAiCompatible => (
                self.saved.openai_profile.credential_ref.take(),
                KEY_AGENT_OPENAI_CREDENTIAL_REF,
                KEY_AGENT_OPENAI_API_KEY,
                Some(KEY_OPENAI_API_KEY),
            ),
        };
        match provider {
            AgentProviderKind::Ollama => {
                self.saved.ollama_profile.api_key = None;
                self.saved.ollama_profile.legacy_api_key = None;
            }
            AgentProviderKind::OpenAiCompatible => {
                self.saved.openai_profile.api_key = None;
                self.saved.openai_profile.legacy_api_key = None;
            }
        }
        if let Some(account) = account {
            self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential { account });
        }
        self.delete_string(ref_key);
        self.delete_string(plaintext_key);
        if let Some(key) = legacy_key {
            self.delete_string(key);
        }
        self.invalidate_provider_requirement();
    }

    fn save_provider_credential(&mut self, provider: AgentProviderKind, secret: String) {
        let previous = match provider {
            AgentProviderKind::Ollama => self.saved.ollama_profile.clone(),
            AgentProviderKind::OpenAiCompatible => self.saved.openai_profile.clone(),
        };
        let account = crate::settings::voice::credentials::new_account(provider.as_str());
        let (account, ref_key, plaintext_key, legacy_key) = match provider {
            AgentProviderKind::Ollama => {
                let profile = &mut self.saved.ollama_profile;
                profile.api_key = Some(secret.clone());
                profile.legacy_api_key = None;
                profile.credential_ref = Some(account.clone());
                (
                    account,
                    KEY_AGENT_OLLAMA_CREDENTIAL_REF,
                    KEY_AGENT_OLLAMA_API_KEY,
                    None,
                )
            }
            AgentProviderKind::OpenAiCompatible => {
                let profile = &mut self.saved.openai_profile;
                profile.api_key = Some(secret.clone());
                profile.legacy_api_key = None;
                profile.credential_ref = Some(account.clone());
                (
                    account,
                    KEY_AGENT_OPENAI_CREDENTIAL_REF,
                    KEY_AGENT_OPENAI_API_KEY,
                    Some(KEY_OPENAI_API_KEY),
                )
            }
        };
        self.credential_write_rollback = Some(CredentialWriteRollback {
            provider,
            previous: previous.clone(),
            new_account: account.clone(),
        });
        self.queue_pending_write(PendingVoiceSettingWrite::StoreCredential {
            account: account.clone(),
            secret,
        });
        self.persist_string(ref_key, &account);
        self.delete_string(plaintext_key);
        if let Some(key) = legacy_key {
            self.delete_string(key);
        }
        if let Some(previous_account) = previous.credential_ref {
            self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
                account: previous_account,
            });
        }
    }

    pub(super) fn finish_credential_persistence(&mut self, result: &Result<(), String>) {
        let Some(rollback) = self.credential_write_rollback.take() else {
            return;
        };
        if result.is_ok() {
            return;
        }
        let (ref_key, plaintext_key, legacy_key) = match rollback.provider {
            AgentProviderKind::Ollama => (
                KEY_AGENT_OLLAMA_CREDENTIAL_REF,
                KEY_AGENT_OLLAMA_API_KEY,
                None,
            ),
            AgentProviderKind::OpenAiCompatible => (
                KEY_AGENT_OPENAI_CREDENTIAL_REF,
                KEY_AGENT_OPENAI_API_KEY,
                Some(KEY_OPENAI_API_KEY),
            ),
        };
        match rollback.provider {
            AgentProviderKind::Ollama => self.saved.ollama_profile = rollback.previous.clone(),
            AgentProviderKind::OpenAiCompatible => {
                self.saved.openai_profile = rollback.previous.clone()
            }
        }
        match rollback.previous.credential_ref.as_deref() {
            Some(account) => self.persist_string(ref_key, account),
            None => self.delete_string(ref_key),
        }
        if rollback.previous.credential_ref.is_none() {
            if let Some(secret) = rollback.previous.api_key.as_deref() {
                self.persist_string(plaintext_key, secret);
                if let Some(key) = legacy_key {
                    self.persist_string(key, secret);
                }
            }
        }
        self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
            account: rollback.new_account,
        });
        self.provider_requirement = VoiceRequirementState::Failed;
        self.refresh_values();
    }

    fn queue_plaintext_credential_deletes(&mut self, provider: AgentProviderKind) {
        match provider {
            AgentProviderKind::Ollama => self.delete_string(KEY_AGENT_OLLAMA_API_KEY),
            AgentProviderKind::OpenAiCompatible => {
                self.delete_string(KEY_AGENT_OPENAI_API_KEY);
                self.delete_string(KEY_OPENAI_API_KEY);
            }
        }
    }

    fn clear_missing_credential_reference(&mut self, provider: AgentProviderKind, account: &str) {
        match provider {
            AgentProviderKind::Ollama => {
                self.saved.ollama_profile.credential_ref = None;
                self.delete_string(KEY_AGENT_OLLAMA_CREDENTIAL_REF);
            }
            AgentProviderKind::OpenAiCompatible => {
                self.saved.openai_profile.credential_ref = None;
                self.delete_string(KEY_AGENT_OPENAI_CREDENTIAL_REF);
            }
        }
        self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
            account: account.to_string(),
        });
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/voice_credentials_tests.rs"]
mod voice_credentials_tests;
