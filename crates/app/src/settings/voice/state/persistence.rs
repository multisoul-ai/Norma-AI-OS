use storage::SettingsStore;

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

    pub(super) fn key(&self) -> &str {
        match self {
            Self::Set { key, .. } | Self::Delete { key, .. } => key,
            Self::StoreCredential { account, .. } | Self::DeleteCredential { account } => account,
        }
    }
}

impl VoiceSettingsState {
    pub(crate) fn take_pending_writes(&mut self) -> Vec<PendingVoiceSettingWrite> {
        if self.persistence_running || self.pending_writes.is_empty() {
            return Vec::new();
        }
        self.persistence_running = true;
        std::mem::take(&mut self.pending_writes)
    }

    pub(crate) fn record_persistence_result(&mut self, result: Result<(), String>) {
        self.persistence_running = false;
        self.finish_credential_persistence(&result);
        self.status = Some(match result {
            Ok(()) => ok_status("Saved"),
            Err(error) => error_status(format!("Could not save setting: {error}")),
        });
    }
}
