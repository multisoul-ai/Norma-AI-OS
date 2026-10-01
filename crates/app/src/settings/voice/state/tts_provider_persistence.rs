use super::values::{
    KEY_TTS_MINIMAX_CREDENTIAL_REF, KEY_TTS_MINIMAX_VALIDATED_CREDENTIAL_REF,
    KEY_TTS_MINIMAX_VALIDATED_MODEL_ID, KEY_TTS_MINIMAX_VALIDATED_VOICE_ID,
};
use super::{PendingVoiceSettingWrite, VoiceSettingsState};
use tts::RemoteAccountStatus;

impl VoiceSettingsState {
    pub(crate) fn finish_minimax_credential_write_persistence(
        &mut self,
        result: &Result<(), String>,
    ) {
        if let Some(rollback) = self.minimax_credential_write_rollback.take() {
            if result.is_ok() {
                if let Some(previous_account) = rollback.previous_account {
                    self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
                        store: self.storage.clone(),
                        account: previous_account,
                    });
                }
            } else {
                self.minimax_profile_generation = self.minimax_profile_generation.wrapping_add(1);
                self.saved.minimax_tts_credential_ref = rollback.previous_account.clone();
                self.saved.minimax_tts_api_key = rollback.previous_secret;
                self.minimax_account_status = if rollback.previous_account.is_some()
                    && self.saved.minimax_tts_api_key.is_some()
                {
                    rollback.previous_account_status
                } else {
                    RemoteAccountStatus::InvalidCredential
                };
                self.minimax_validated_selection = rollback.previous_validation;
                self.saved.minimax_validated_credential_ref =
                    rollback.previous_validated_credential_ref;
                self.saved.minimax_validated_model_id = rollback.previous_validated_model_id;
                self.saved.minimax_validated_voice_id = rollback.previous_validated_voice_id;
                match rollback.previous_account.as_deref() {
                    Some(account) => self.persist_string(KEY_TTS_MINIMAX_CREDENTIAL_REF, account),
                    None => self.delete_string(KEY_TTS_MINIMAX_CREDENTIAL_REF),
                }
                self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
                    store: self.storage.clone(),
                    account: rollback.new_account,
                });
                self.restore_minimax_validation_persistence();
                self.refresh_values();
            }
        }
    }

    pub(crate) fn finish_minimax_credential_clear_persistence(
        &mut self,
        result: &Result<(), String>,
    ) {
        let Some(clear) = self.minimax_credential_clear_rollback.take() else {
            return;
        };
        if result.is_ok() {
            self.saved.minimax_tts_credential_ref = None;
            self.saved.minimax_tts_api_key = None;
            self.minimax_api_key_draft = None;
            self.minimax_voice_catalog = tts::ProviderVoiceCatalog::default();
            self.minimax_account_status = RemoteAccountStatus::Unchecked;
            self.clear_minimax_validation();
            self.refresh_values();
        } else {
            self.persist_string(KEY_TTS_MINIMAX_CREDENTIAL_REF, &clear.account);
        }
    }

    pub(crate) fn finish_minimax_validation_persistence(&mut self, result: &Result<(), String>) {
        let Some(rollback) = self.minimax_validation_write_rollback.take() else {
            return;
        };
        if result.is_ok() {
            return;
        }
        self.minimax_validated_selection = rollback.previous_validation;
        self.saved.minimax_validated_credential_ref = rollback.previous_credential_ref;
        self.saved.minimax_validated_model_id = rollback.previous_model_id;
        self.saved.minimax_validated_voice_id = rollback.previous_voice_id;
        self.restore_minimax_validation_persistence();
    }

    fn restore_minimax_validation_persistence(&mut self) {
        self.restore_optional_string(
            KEY_TTS_MINIMAX_VALIDATED_CREDENTIAL_REF,
            self.saved.minimax_validated_credential_ref.clone(),
        );
        self.restore_optional_string(
            KEY_TTS_MINIMAX_VALIDATED_MODEL_ID,
            self.saved
                .minimax_validated_model_id
                .as_ref()
                .map(|value| value.as_str().to_string()),
        );
        self.restore_optional_string(
            KEY_TTS_MINIMAX_VALIDATED_VOICE_ID,
            self.saved
                .minimax_validated_voice_id
                .as_ref()
                .map(|value| value.as_str().to_string()),
        );
    }

    fn restore_optional_string(&mut self, key: &str, value: Option<String>) {
        match value {
            Some(value) => self.persist_string(key, &value),
            None => self.delete_string(key),
        }
    }
}
