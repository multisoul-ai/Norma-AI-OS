use super::{values::KEY_GPT_LIVE_CREDENTIAL_REF, PendingVoiceSettingWrite, VoiceSettingsState};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) enum CredentialIntent {
    Save(String),
    Clear,
}

type CredentialOutcome = Arc<Mutex<Option<(bool, Option<String>)>>>;

#[derive(Clone)]
pub(crate) struct GptLiveCredentialWrite {
    store: Option<storage::SettingsStore>,
    previous_ref: Option<String>,
    new_account: Option<String>,
    secret: Option<String>,
    outcome: CredentialOutcome,
}

impl GptLiveCredentialWrite {
    pub(super) fn execute(self) -> Result<(), String> {
        let store = self.store.ok_or("credential SQLite store unavailable")?;
        let mut changes = vec![(KEY_GPT_LIVE_CREDENTIAL_REF.into(), self.new_account)];
        if let (Some(account), Some(secret)) = (&changes[0].1, self.secret) {
            changes.push((
                crate::settings::voice::credentials::key(account),
                Some(secret),
            ));
        }
        if let Some(account) = self
            .previous_ref
            .as_deref()
            .filter(|account| super::gpt_live::owns_account(account))
        {
            changes.push((crate::settings::voice::credentials::key(account), None));
        }
        let outcome = store
            .apply_string_changes_if_current(
                KEY_GPT_LIVE_CREDENTIAL_REF,
                self.previous_ref.as_deref(),
                &changes,
            )
            .map_err(|error| error.to_string())?;
        let applied = outcome.0;
        *self
            .outcome
            .lock()
            .map_err(|_| "Live credential result unavailable")? = Some(outcome);
        if applied {
            Ok(())
        } else {
            Err("GPT-Live key changed in another window; review and retry.".into())
        }
    }
}

#[derive(Clone)]
pub(super) struct GptLiveCredentialRollback {
    previous_ref: Option<String>,
    outcome: CredentialOutcome,
    attempted: CredentialIntent,
    queued: Option<CredentialIntent>,
}

impl VoiceSettingsState {
    pub(super) fn save_gpt_live_credential(&mut self, secret: String) -> bool {
        self.change_gpt_live_credential(CredentialIntent::Save(secret))
    }

    pub(crate) fn clear_gpt_live_credential(&mut self) -> bool {
        self.change_gpt_live_credential(CredentialIntent::Clear)
    }

    pub(crate) fn has_gpt_live_credential_retry(&self) -> bool {
        self.gpt_live_credential_retry.is_some()
    }

    pub(crate) fn retry_gpt_live_credential_change(&mut self) -> bool {
        let Some(intent) = self.gpt_live_credential_retry.take() else {
            return false;
        };
        self.change_gpt_live_credential(intent)
    }

    fn change_gpt_live_credential(&mut self, intent: CredentialIntent) -> bool {
        self.gpt_live_credential_retry = None;
        self.status = Some(super::ok_status(ui::i18n::LocalizedText::new(
            "voice.saving",
        )));
        if let Some(active) = self.gpt_live_credential_rollback.as_mut() {
            if self.persistence_running {
                active.queued = Some(intent);
                return true;
            }
            self.saved.gpt_live_profile.credential_ref = active.previous_ref.clone();
            self.pending_writes
                .retain(|write| !matches!(write, PendingVoiceSettingWrite::GptLiveCredential(_)));
            self.gpt_live_credential_rollback = None;
        }
        let previous_ref = self.saved.gpt_live_profile.credential_ref.clone();
        let attempted = intent.clone();
        let (new_account, secret) = match intent {
            CredentialIntent::Save(secret) => (
                Some(crate::settings::voice::credentials::new_account(
                    "gpt-live-desktop",
                )),
                Some(secret),
            ),
            CredentialIntent::Clear if previous_ref.is_none() => return false,
            CredentialIntent::Clear => (None, None),
        };
        let outcome = Arc::new(Mutex::new(None));
        self.gpt_live_credential_rollback = Some(GptLiveCredentialRollback {
            previous_ref: previous_ref.clone(),
            outcome: outcome.clone(),
            attempted,
            queued: None,
        });
        self.queue_pending_write(PendingVoiceSettingWrite::GptLiveCredential(
            GptLiveCredentialWrite {
                store: self.storage.clone(),
                previous_ref,
                new_account: new_account.clone(),
                secret,
                outcome,
            },
        ));
        self.saved.gpt_live_profile.credential_ref = new_account;
        self.refresh_values();
        true
    }

    pub(super) fn finish_gpt_live_credential_persistence(&mut self) {
        let Some(active) = self.gpt_live_credential_rollback.take() else {
            return;
        };
        // The worker reports this transaction separately from unrelated writes in the batch.
        let outcome = active
            .outcome
            .try_lock()
            .ok()
            .and_then(|mut result| result.take());
        let applied = outcome.as_ref().is_some_and(|(applied, _)| *applied);
        self.saved.gpt_live_profile.credential_ref = outcome
            .map(|(_, current)| current)
            .unwrap_or(active.previous_ref);
        self.refresh_values();
        if applied {
            if let Some(intent) = active.queued {
                self.change_gpt_live_credential(intent);
            }
        } else {
            // An explicit retry preserves the latest intent without stealing the active editor.
            self.gpt_live_credential_retry = Some(active.queued.unwrap_or(active.attempted));
        }
    }
}
