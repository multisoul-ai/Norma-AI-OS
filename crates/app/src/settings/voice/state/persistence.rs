use storage::SettingsStore;

use super::values::{
    KEY_TTS_MINIMAX_VALIDATED_CREDENTIAL_REF, KEY_TTS_MINIMAX_VALIDATED_MODEL_ID,
    KEY_TTS_MINIMAX_VALIDATED_VOICE_ID,
};
use super::{error_status, ok_status, VoiceSettingsState};

#[derive(Clone)]
pub(crate) enum PendingVoiceSettingWrite {
    Set {
        store: SettingsStore,
        key: String,
        value: String,
    },
    Delete {
        store: SettingsStore,
        key: String,
    },
    StoreCredential {
        account: String,
        secret: String,
    },
    DeleteCredential {
        account: String,
    },
}

#[derive(Clone, Copy, Default)]
pub(super) struct VoicePersistenceBatch {
    agent_credential: bool,
    azure_credential: bool,
    minimax_credential: bool,
    minimax_clear: bool,
    minimax_validation: bool,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct VoicePersistenceCompletion {
    pub(crate) agent_credential: bool,
    pub(crate) minimax_credential: bool,
    pub(crate) minimax_clear: bool,
    pub(crate) minimax_validation: bool,
}

impl PendingVoiceSettingWrite {
    pub(crate) fn execute(self) -> Result<(), String> {
        match self {
            Self::Set { store, key, value } => store
                .set_string(&key, &value)
                .map_err(|error| error.to_string()),
            Self::Delete { store, key } => {
                store.delete_string(&key).map_err(|error| error.to_string())
            }
            Self::StoreCredential { account, secret } => {
                crate::settings::voice::credentials::store(&account, &secret)
            }
            Self::DeleteCredential { account } => {
                crate::settings::voice::credentials::delete(&account)
            }
        }
    }

    pub(crate) fn execute_settings_atomically(writes: Vec<Self>) -> Result<(), String> {
        let mut store = None;
        let mut changes = Vec::with_capacity(writes.len());
        for write in writes {
            match write {
                Self::Set {
                    store: write_store,
                    key,
                    value,
                } => {
                    store.get_or_insert(write_store);
                    changes.push((key, Some(value)));
                }
                Self::Delete {
                    store: write_store,
                    key,
                } => {
                    store.get_or_insert(write_store);
                    changes.push((key, None));
                }
                Self::StoreCredential { .. } | Self::DeleteCredential { .. } => {
                    return Err(
                        "TTS activation transaction cannot contain credential writes.".to_string(),
                    );
                }
            }
        }
        let Some(store) = store else {
            return Ok(());
        };
        store
            .apply_string_changes(&changes)
            .map_err(|error| error.to_string())
    }

    pub(super) fn key(&self) -> &str {
        match self {
            Self::Set { key, .. } | Self::Delete { key, .. } => key,
            Self::StoreCredential { account, .. } | Self::DeleteCredential { account } => account,
        }
    }

    fn stores_credential(&self, expected_account: Option<&str>) -> bool {
        matches!(
            (self, expected_account),
            (Self::StoreCredential { account, .. }, Some(expected)) if account == expected
        )
    }

    fn deletes_credential(&self, expected_account: Option<&str>) -> bool {
        matches!(
            (self, expected_account),
            (Self::DeleteCredential { account }, Some(expected)) if account == expected
        )
    }

    fn writes_minimax_validation(&self) -> bool {
        let key = match self {
            Self::Set { key, .. } | Self::Delete { key, .. } => key.as_str(),
            Self::StoreCredential { .. } | Self::DeleteCredential { .. } => return false,
        };
        matches!(
            key,
            KEY_TTS_MINIMAX_VALIDATED_CREDENTIAL_REF
                | KEY_TTS_MINIMAX_VALIDATED_MODEL_ID
                | KEY_TTS_MINIMAX_VALIDATED_VOICE_ID
        )
    }
}

impl VoiceSettingsState {
    pub(crate) fn settings_persistence_pending(&self) -> bool {
        self.persistence_running || !self.pending_writes.is_empty()
    }

    pub(crate) fn take_pending_writes(&mut self) -> Vec<PendingVoiceSettingWrite> {
        if self.persistence_running || self.pending_writes.is_empty() {
            return Vec::new();
        }
        self.persistence_running = true;
        let agent_account = self.agent_credential_write_account();
        let azure_account = self.azure_credential_write_account();
        let minimax_account = self.minimax_credential_write_account();
        let minimax_clear_account = self.minimax_credential_clear_account();
        let minimax_validation_pending = self.minimax_validation_write_rollback.is_some();
        self.active_persistence_batch = Some(VoicePersistenceBatch {
            agent_credential: self
                .pending_writes
                .iter()
                .any(|write| write.stores_credential(agent_account)),
            azure_credential: self
                .pending_writes
                .iter()
                .any(|write| write.stores_credential(azure_account)),
            minimax_credential: self
                .pending_writes
                .iter()
                .any(|write| write.stores_credential(minimax_account)),
            minimax_clear: self
                .pending_writes
                .iter()
                .any(|write| write.deletes_credential(minimax_clear_account)),
            minimax_validation: minimax_validation_pending
                && self
                    .pending_writes
                    .iter()
                    .any(PendingVoiceSettingWrite::writes_minimax_validation),
        });
        std::mem::take(&mut self.pending_writes)
    }

    pub(crate) fn record_persistence_result(
        &mut self,
        result: Result<(), String>,
    ) -> VoicePersistenceCompletion {
        self.persistence_running = false;
        let batch = self.active_persistence_batch.take().unwrap_or_default();
        if batch.agent_credential {
            self.finish_credential_persistence(&result);
        }
        if batch.azure_credential {
            self.finish_azure_credential_persistence(&result);
        }
        if batch.minimax_credential {
            self.finish_minimax_credential_write_persistence(&result);
        }
        if batch.minimax_clear {
            self.finish_minimax_credential_clear_persistence(&result);
        }
        if batch.minimax_validation {
            self.finish_minimax_validation_persistence(&result);
        }
        self.status = Some(match result {
            Ok(()) => ok_status("Saved"),
            Err(error) => error_status(format!("Could not save setting: {error}")),
        });
        VoicePersistenceCompletion {
            agent_credential: batch.agent_credential,
            minimax_credential: batch.minimax_credential,
            minimax_clear: batch.minimax_clear,
            minimax_validation: batch.minimax_validation,
        }
    }
}
