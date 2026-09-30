use super::values::{
    KEY_TTS_ACTIVE_PROVIDER_ID, KEY_TTS_MINIMAX_CREDENTIAL_REF, KEY_TTS_MINIMAX_MODEL_ID,
    KEY_TTS_MINIMAX_VALIDATED_CREDENTIAL_REF, KEY_TTS_MINIMAX_VALIDATED_MODEL_ID,
    KEY_TTS_MINIMAX_VALIDATED_VOICE_ID, KEY_TTS_MINIMAX_VOICE_ID, KEY_TTS_MINIMAX_VOICE_PROVENANCE,
};
use super::{PendingVoiceSettingWrite, VoiceSettingsState};
use tts::{
    ProviderVoiceCatalog, RemoteAccountStatus, RemoteTtsReadiness, RemoteTtsValidation, TtsModelId,
    TtsProviderId, TtsProviderProfile, TtsValidatedSelection, TtsVoiceId, VoiceSelectionProvenance,
};

#[derive(Clone)]
pub(super) struct MiniMaxCredentialWriteRollback {
    pub(super) previous_account: Option<String>,
    pub(super) previous_secret: Option<String>,
    pub(super) previous_account_status: RemoteAccountStatus,
    pub(super) previous_validation: Option<TtsValidatedSelection>,
    pub(super) previous_validated_credential_ref: Option<String>,
    pub(super) previous_validated_model_id: Option<TtsModelId>,
    pub(super) previous_validated_voice_id: Option<TtsVoiceId>,
    pub(super) new_account: String,
}

#[derive(Clone)]
pub(super) struct MiniMaxCredentialClearRollback {
    pub(super) account: String,
}

#[derive(Clone)]
pub(super) struct MiniMaxValidationWriteRollback {
    pub(super) previous_validation: Option<TtsValidatedSelection>,
    pub(super) previous_credential_ref: Option<String>,
    pub(super) previous_model_id: Option<TtsModelId>,
    pub(super) previous_voice_id: Option<TtsVoiceId>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MiniMaxConnectionFailureDisposition {
    Stale,
    CandidateRejected,
    ActiveCredentialFailed,
}

impl VoiceSettingsState {
    pub(super) fn minimax_credential_write_account(&self) -> Option<&str> {
        self.minimax_credential_write_rollback
            .as_ref()
            .map(|rollback| rollback.new_account.as_str())
    }

    pub(super) fn minimax_credential_clear_account(&self) -> Option<&str> {
        self.minimax_credential_clear_rollback
            .as_ref()
            .map(|rollback| rollback.account.as_str())
    }

    pub(crate) fn minimax_credential_configured(&self) -> bool {
        self.minimax_api_key_draft.is_some()
            || self.saved.minimax_tts_credential_ref.is_some()
            || self.saved.minimax_tts_api_key.is_some()
    }

    pub(crate) fn minimax_credential_draft_pending(&self) -> bool {
        self.minimax_api_key_draft.is_some()
    }

    pub(super) fn discard_minimax_credential_candidate(&mut self) {
        if self.minimax_api_key_draft.take().is_none() {
            return;
        }
        self.minimax_connection_generation = self.minimax_connection_generation.wrapping_add(1);
        self.minimax_profile_generation = self.minimax_profile_generation.wrapping_add(1);
        self.minimax_account_status = if self.saved.minimax_tts_credential_ref.is_some()
            && self.saved.minimax_tts_api_key.is_some()
        {
            if self.minimax_account_status == RemoteAccountStatus::Checking {
                self.minimax_connection_previous_status
            } else {
                self.minimax_account_status
            }
        } else {
            RemoteAccountStatus::Unchecked
        };
        self.refresh_values();
    }

    pub(crate) fn minimax_persistence_blocks_preview(&self) -> bool {
        self.minimax_credential_write_rollback.is_some()
            || self.minimax_credential_clear_rollback.is_some()
            || self.minimax_validation_write_rollback.is_some()
    }

    pub(crate) fn accepts_minimax_connection_generation(&self, generation: u64) -> bool {
        generation == self.minimax_connection_generation
    }

    pub(crate) fn begin_minimax_preview(&mut self, generation: u64) {
        self.minimax_preview_generation = Some(generation);
        self.status = Some(super::ok_status("Generating MiniMax preview…"));
    }

    pub(crate) fn finish_minimax_preview(&mut self, generation: u64) {
        if self.minimax_preview_generation == Some(generation) {
            self.minimax_preview_generation = None;
        }
    }

    pub(crate) fn minimax_preview_in_progress(&self) -> bool {
        self.minimax_preview_generation.is_some()
    }

    #[cfg(test)]
    pub(crate) fn minimax_credential_clear_pending(&self) -> bool {
        self.minimax_credential_clear_rollback.is_some()
    }

    pub(crate) fn set_minimax_tts_model(&mut self, model_id: TtsModelId) -> bool {
        if self.saved.minimax_tts_model_id.as_ref() == Some(&model_id) {
            return false;
        }
        self.minimax_profile_generation = self.minimax_profile_generation.wrapping_add(1);
        self.saved.minimax_tts_model_id = Some(model_id.clone());
        self.clear_minimax_validation();
        self.persist_string(KEY_TTS_MINIMAX_MODEL_ID, model_id.as_str());
        self.refresh_values();
        true
    }

    pub(crate) fn select_tts_provider(&mut self, provider_id: Option<TtsProviderId>) {
        self.saved.active_tts_provider_id = provider_id.clone();
        match provider_id {
            Some(provider_id) => {
                self.persist_string(KEY_TTS_ACTIVE_PROVIDER_ID, provider_id.as_str())
            }
            None => self.delete_string(KEY_TTS_ACTIVE_PROVIDER_ID),
        }
        self.refresh_values();
    }

    pub(crate) fn set_minimax_tts_selection(
        &mut self,
        model_id: TtsModelId,
        voice_id: TtsVoiceId,
        provenance: VoiceSelectionProvenance,
    ) -> bool {
        let changed = self.saved.minimax_tts_model_id.as_ref() != Some(&model_id)
            || self.saved.minimax_tts_voice_id.as_ref() != Some(&voice_id)
            || self.saved.minimax_tts_voice_provenance != Some(provenance);
        if !changed {
            return false;
        }
        self.minimax_profile_generation = self.minimax_profile_generation.wrapping_add(1);
        self.saved.minimax_tts_model_id = Some(model_id.clone());
        self.saved.minimax_tts_voice_id = Some(voice_id.clone());
        self.saved.minimax_tts_voice_provenance = Some(provenance);
        self.clear_minimax_validation();
        self.persist_string(KEY_TTS_MINIMAX_MODEL_ID, model_id.as_str());
        self.persist_string(KEY_TTS_MINIMAX_VOICE_ID, voice_id.as_str());
        self.persist_string(
            KEY_TTS_MINIMAX_VOICE_PROVENANCE,
            match provenance {
                VoiceSelectionProvenance::Catalog => "catalog",
                VoiceSelectionProvenance::Manual => "manual",
            },
        );
        self.refresh_values();
        true
    }

    pub(crate) fn minimax_voice_catalog(&self) -> &ProviderVoiceCatalog {
        &self.minimax_voice_catalog
    }

    pub(crate) fn filtered_minimax_voices(&self) -> Vec<&tts::ProviderVoiceDescriptor> {
        let query = self.minimax_voice_search().trim().to_lowercase();
        self.minimax_voice_catalog
            .voices()
            .iter()
            .filter(|voice| {
                query.is_empty()
                    || voice.display_name().to_lowercase().contains(&query)
                    || voice.id().as_str().to_lowercase().contains(&query)
            })
            .collect()
    }

    pub(crate) fn minimax_account_status(&self) -> RemoteAccountStatus {
        self.minimax_account_status
    }

    pub(crate) fn minimax_readiness(&self) -> RemoteTtsReadiness {
        if !self.minimax_credential_configured() {
            return RemoteTtsReadiness::Unconfigured;
        }
        let profile = self.minimax_profile();
        let validation = RemoteTtsValidation::new(
            self.minimax_account_status,
            self.values.minimax_tts_credential_ref().unwrap_or_default(),
            self.minimax_voice_catalog
                .voices()
                .iter()
                .map(|voice| voice.id().clone())
                .collect(),
            self.minimax_validated_selection.clone(),
        );
        tts::evaluate_remote_readiness(profile.as_ref(), &validation)
    }

    pub(crate) fn update_minimax_api_key_draft(&mut self, value: String) {
        let draft = Some(value.trim().to_string()).filter(|key| !key.is_empty());
        if draft != self.minimax_api_key_draft {
            self.minimax_connection_generation = self.minimax_connection_generation.wrapping_add(1);
            self.minimax_profile_generation = self.minimax_profile_generation.wrapping_add(1);
            if self.minimax_account_status == RemoteAccountStatus::Checking
                && self.saved.minimax_tts_credential_ref.is_some()
                && self.saved.minimax_tts_api_key.is_some()
            {
                self.minimax_account_status = self.minimax_connection_previous_status;
            }
        }
        self.minimax_api_key_draft = draft;
        if self.saved.minimax_tts_credential_ref.is_none()
            || self.saved.minimax_tts_api_key.is_none()
        {
            self.minimax_account_status = RemoteAccountStatus::Unchecked;
        }
    }

    pub(crate) fn clear_minimax_credential(&mut self) {
        if self.minimax_credential_write_rollback.is_some()
            || self.minimax_credential_clear_rollback.is_some()
        {
            self.status = Some(super::error_status(
                "Wait for the current MiniMax credential operation to finish.",
            ));
            return;
        }
        self.minimax_connection_generation = self.minimax_connection_generation.wrapping_add(1);
        self.minimax_profile_generation = self.minimax_profile_generation.wrapping_add(1);
        self.minimax_api_key_draft = None;
        let Some(account) = self.saved.minimax_tts_credential_ref.clone() else {
            self.minimax_account_status = RemoteAccountStatus::Unchecked;
            self.clear_minimax_validation();
            self.refresh_values();
            return;
        };
        self.minimax_credential_clear_rollback = Some(MiniMaxCredentialClearRollback {
            account: account.clone(),
        });
        // Delete the recoverable SQLite reference first. If credential deletion then fails,
        // the completion handler restores this reference while the secret remains in memory.
        self.delete_string(KEY_TTS_MINIMAX_CREDENTIAL_REF);
        self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
            store: self.storage.clone(),
            account,
        });
    }

    pub(crate) fn begin_minimax_connection(&mut self) -> Result<(u64, String), String> {
        if self.minimax_credential_write_rollback.is_some()
            || self.minimax_credential_clear_rollback.is_some()
        {
            return Err("Wait for the current MiniMax credential operation to finish.".to_string());
        }
        let api_key = self
            .minimax_api_key_draft
            .clone()
            .or_else(|| self.saved.minimax_tts_api_key.clone())
            .ok_or_else(|| "Add a MiniMax API Key before connecting.".to_string())?;
        if self.minimax_account_status != RemoteAccountStatus::Checking {
            self.minimax_connection_previous_status = self.minimax_account_status;
        }
        self.minimax_connection_generation = self.minimax_connection_generation.wrapping_add(1);
        self.minimax_account_status = RemoteAccountStatus::Checking;
        Ok((self.minimax_connection_generation, api_key))
    }

    pub(crate) fn record_minimax_connection_success(
        &mut self,
        generation: u64,
        api_key: String,
        catalog: ProviderVoiceCatalog,
    ) -> bool {
        if generation != self.minimax_connection_generation {
            return false;
        }
        if self.minimax_api_key_draft.is_some() || self.saved.minimax_tts_credential_ref.is_none() {
            self.save_minimax_credential(api_key);
        }
        self.minimax_api_key_draft = None;
        self.minimax_voice_catalog = catalog;
        self.minimax_account_status = RemoteAccountStatus::Ready;
        self.refresh_values();
        true
    }

    pub(crate) fn record_minimax_connection_failure(
        &mut self,
        generation: u64,
        kind: tts::TtsErrorKind,
        message: impl Into<String>,
    ) -> MiniMaxConnectionFailureDisposition {
        if generation != self.minimax_connection_generation {
            return MiniMaxConnectionFailureDisposition::Stale;
        }
        let rejected_replacement = self.minimax_api_key_draft.is_some()
            && self.saved.minimax_tts_credential_ref.is_some()
            && self.saved.minimax_tts_api_key.is_some();
        if rejected_replacement {
            self.minimax_account_status = self.minimax_connection_previous_status;
            self.status = Some(super::error_status(message));
            return MiniMaxConnectionFailureDisposition::CandidateRejected;
        }
        self.minimax_account_status = if kind == tts::TtsErrorKind::Authentication {
            RemoteAccountStatus::InvalidCredential
        } else {
            RemoteAccountStatus::TemporarilyUnavailable
        };
        self.status = Some(super::error_status(message));
        MiniMaxConnectionFailureDisposition::ActiveCredentialFailed
    }

    pub(crate) fn minimax_profile_generation(&self) -> u64 {
        self.tts_picker_profile_generation()
            .unwrap_or(self.minimax_profile_generation)
    }

    pub(crate) fn mark_minimax_manual_voice_validated(
        &mut self,
        expected_profile_generation: u64,
    ) -> bool {
        if let Some(validated) = self.mark_tts_picker_minimax_validated(expected_profile_generation)
        {
            return validated;
        }
        if expected_profile_generation != self.minimax_profile_generation {
            return false;
        }
        let Some(profile) = self.minimax_profile() else {
            return false;
        };
        let Some(credential_ref) = self
            .values
            .minimax_tts_credential_ref()
            .map(ToOwned::to_owned)
        else {
            return false;
        };
        if self.storage.is_some() && self.minimax_validation_write_rollback.is_some() {
            return false;
        }
        if self.storage.is_some() {
            self.minimax_validation_write_rollback = Some(MiniMaxValidationWriteRollback {
                previous_validation: self.minimax_validated_selection.clone(),
                previous_credential_ref: self.saved.minimax_validated_credential_ref.clone(),
                previous_model_id: self.saved.minimax_validated_model_id.clone(),
                previous_voice_id: self.saved.minimax_validated_voice_id.clone(),
            });
        }
        self.minimax_validated_selection = Some(TtsValidatedSelection::from_profile(
            &credential_ref,
            &profile,
        ));
        self.saved.minimax_validated_credential_ref = Some(credential_ref.clone());
        self.saved.minimax_validated_model_id = Some(profile.model_id().clone());
        self.saved.minimax_validated_voice_id = Some(profile.voice_id().clone());
        self.persist_string(KEY_TTS_MINIMAX_VALIDATED_CREDENTIAL_REF, &credential_ref);
        self.persist_string(
            KEY_TTS_MINIMAX_VALIDATED_MODEL_ID,
            profile.model_id().as_str(),
        );
        self.persist_string(
            KEY_TTS_MINIMAX_VALIDATED_VOICE_ID,
            profile.voice_id().as_str(),
        );
        true
    }

    pub(crate) fn minimax_credential_ref_to_load(&self) -> Option<String> {
        self.saved
            .minimax_tts_api_key
            .is_none()
            .then(|| self.saved.minimax_tts_credential_ref.clone())
            .flatten()
    }

    pub(crate) fn apply_loaded_minimax_credential(
        &mut self,
        account: &str,
        result: Result<Option<String>, String>,
    ) -> bool {
        if self.saved.minimax_tts_credential_ref.as_deref() != Some(account)
            || self.saved.minimax_tts_api_key.is_some()
        {
            return false;
        }
        match result {
            Ok(Some(secret)) => self.saved.minimax_tts_api_key = Some(secret),
            Ok(None) => {
                self.saved.minimax_tts_credential_ref = None;
                self.clear_minimax_validation();
                self.delete_string(KEY_TTS_MINIMAX_CREDENTIAL_REF);
                self.minimax_account_status = RemoteAccountStatus::InvalidCredential;
            }
            Err(error) => {
                self.status = Some(super::error_status(error));
                self.minimax_account_status = RemoteAccountStatus::TemporarilyUnavailable;
            }
        }
        self.refresh_values();
        self.saved.minimax_tts_api_key.is_some()
    }

    fn minimax_profile(&self) -> Option<TtsProviderProfile> {
        let provider_id = TtsProviderId::parse(tts::MINIMAX_TTS_PROVIDER_ID)
            .expect("built-in MiniMax TTS Provider ID must be valid");
        Some(TtsProviderProfile::new(
            provider_id,
            self.values.minimax_tts_model_id()?.clone(),
            self.values.minimax_tts_voice_id()?.clone(),
            self.values.minimax_tts_voice_provenance(),
        ))
    }

    fn save_minimax_credential(&mut self, secret: String) {
        let previous_account = self.saved.minimax_tts_credential_ref.clone();
        let previous_secret = self.saved.minimax_tts_api_key.clone();
        let previous_account_status = self.minimax_connection_previous_status;
        let previous_validation = self.minimax_validated_selection.clone();
        let previous_validated_credential_ref = self.saved.minimax_validated_credential_ref.clone();
        let previous_validated_model_id = self.saved.minimax_validated_model_id.clone();
        let previous_validated_voice_id = self.saved.minimax_validated_voice_id.clone();
        self.clear_minimax_validation();
        let account = crate::settings::voice::credentials::new_account("tts-minimax");
        self.saved.minimax_tts_credential_ref = Some(account.clone());
        self.saved.minimax_tts_api_key = Some(secret.clone());
        self.minimax_credential_write_rollback = Some(MiniMaxCredentialWriteRollback {
            previous_account: previous_account.clone(),
            previous_secret,
            previous_account_status,
            previous_validation,
            previous_validated_credential_ref,
            previous_validated_model_id,
            previous_validated_voice_id,
            new_account: account.clone(),
        });
        self.queue_pending_write(PendingVoiceSettingWrite::StoreCredential {
            store: self.storage.clone(),
            account: account.clone(),
            secret,
        });
        self.persist_string(KEY_TTS_MINIMAX_CREDENTIAL_REF, &account);
    }

    pub(super) fn clear_minimax_validation(&mut self) {
        let had_validation = self.minimax_validated_selection.is_some()
            || self.saved.minimax_validated_credential_ref.is_some()
            || self.saved.minimax_validated_model_id.is_some()
            || self.saved.minimax_validated_voice_id.is_some();
        self.minimax_validated_selection = None;
        self.saved.minimax_validated_credential_ref = None;
        self.saved.minimax_validated_model_id = None;
        self.saved.minimax_validated_voice_id = None;
        if had_validation {
            self.delete_string(KEY_TTS_MINIMAX_VALIDATED_CREDENTIAL_REF);
            self.delete_string(KEY_TTS_MINIMAX_VALIDATED_MODEL_ID);
            self.delete_string(KEY_TTS_MINIMAX_VALIDATED_VOICE_ID);
        }
    }
}
