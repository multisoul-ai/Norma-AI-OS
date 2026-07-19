use super::values::{
    KEY_ASR_AZURE_CREDENTIAL_REF, KEY_ASR_AZURE_LOCALE, KEY_ASR_AZURE_READY, KEY_ASR_AZURE_REGION,
    KEY_ASR_PROVIDER,
};
use super::PendingVoiceSettingWrite;
use super::VoiceSettingsState;
use super::VoiceSettingsValues;
use std::path::Path;

#[derive(Clone)]
pub(super) struct AzureCredentialWriteRollback {
    previous_account: Option<String>,
    new_account: String,
}

impl VoiceSettingsState {
    pub(crate) fn begin_azure_asr_recognition_test(&mut self) {
        self.status = Some(super::ok_status(
            "Testing Azure Speech for 3 seconds… Speak now.",
        ));
    }

    pub(crate) fn record_azure_asr_recognition_test_result(
        &mut self,
        tested_profile: &super::values::AzureAsrProfileValues,
        result: Result<String, String>,
    ) -> bool {
        if self.values.azure_asr_profile() != tested_profile {
            return false;
        }
        self.status = Some(match result {
            Ok(text) if !text.trim().is_empty() => {
                super::ok_status(format!("Azure test recognized: {}", text.trim()))
            }
            Ok(_) => super::error_status("Azure test completed without recognized speech"),
            Err(error) => super::error_status(format!("Azure test failed: {error}")),
        });
        true
    }

    pub(crate) fn record_azure_asr_validation_result(
        &mut self,
        validated_profile: &super::values::AzureAsrProfileValues,
        result: Result<(), String>,
    ) -> bool {
        if self.values.azure_asr_profile() != validated_profile {
            return false;
        }
        match result {
            Ok(()) => {
                self.mark_azure_asr_ready(true);
                self.status = Some(super::ok_status("Azure Speech is ready to activate"));
            }
            Err(error) => {
                self.mark_azure_asr_ready(false);
                self.status = Some(super::error_status(format!(
                    "Could not validate Azure Speech: {error}"
                )));
            }
        }
        true
    }

    pub(crate) fn save_azure_asr_credential(&mut self, secret: String) {
        let secret = secret.trim().to_string();
        if secret.is_empty() {
            return;
        }
        let previous_account = self.saved.azure_asr_credential_ref.clone();
        let account = crate::settings::voice::credentials::new_account("asr-azure");
        self.queue_pending_write(PendingVoiceSettingWrite::StoreCredential {
            account: account.clone(),
            secret,
        });
        self.saved.azure_asr_credential_ref = Some(account.clone());
        self.azure_credential_write_rollback = Some(AzureCredentialWriteRollback {
            previous_account: previous_account.clone(),
            new_account: account.clone(),
        });
        self.saved.azure_asr_ready = Some(false);
        self.persist_string(KEY_ASR_AZURE_CREDENTIAL_REF, &account);
        self.persist_string(KEY_ASR_AZURE_READY, "false");
        if let Some(previous_account) = previous_account {
            self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
                account: previous_account,
            });
        }
        self.refresh_values();
    }

    pub(crate) fn clear_azure_asr_credential(&mut self) {
        if let Some(account) = self.saved.azure_asr_credential_ref.take() {
            self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential { account });
        }
        self.saved.azure_asr_ready = Some(false);
        self.delete_string(KEY_ASR_AZURE_CREDENTIAL_REF);
        self.persist_string(KEY_ASR_AZURE_READY, "false");
        if self.saved.asr_provider == Some(asr::AsrProviderId::CloudAzure) {
            self.activate_local_sherpa_asr();
        }
        self.refresh_values();
    }

    pub(crate) fn load_azure_asr_secret(account: &str) -> Result<Option<String>, String> {
        crate::settings::voice::credentials::load(account)
    }

    pub(super) fn finish_azure_credential_persistence(&mut self, result: &Result<(), String>) {
        let Some(rollback) = self.azure_credential_write_rollback.take() else {
            return;
        };
        if result.is_ok() {
            return;
        }
        self.saved.azure_asr_credential_ref = rollback.previous_account.clone();
        self.saved.azure_asr_ready = Some(false);
        match rollback.previous_account {
            Some(account) => self.persist_string(KEY_ASR_AZURE_CREDENTIAL_REF, &account),
            None => self.delete_string(KEY_ASR_AZURE_CREDENTIAL_REF),
        }
        self.persist_string(KEY_ASR_AZURE_READY, "false");
        self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
            account: rollback.new_account,
        });
        if self.saved.asr_provider == Some(asr::AsrProviderId::CloudAzure) {
            self.activate_local_sherpa_asr();
        }
        self.refresh_values();
    }
    pub(crate) fn update_azure_asr_draft(
        &mut self,
        region: &str,
        locale: &str,
        credential_ref: Option<String>,
    ) {
        let region = region.trim().to_ascii_lowercase();
        let locale = locale.trim().to_string();
        self.saved.azure_asr_region = (!region.is_empty()).then(|| region.clone());
        self.saved.azure_asr_locale = Some(locale.clone());
        self.saved.azure_asr_credential_ref = credential_ref
            .map(|reference| reference.trim().to_string())
            .filter(|reference| !reference.is_empty());
        self.saved.azure_asr_ready = Some(false);
        if region.is_empty() {
            self.delete_string(KEY_ASR_AZURE_REGION);
        } else {
            self.persist_string(KEY_ASR_AZURE_REGION, &region);
        }
        self.persist_string(KEY_ASR_AZURE_LOCALE, &locale);
        match self.saved.azure_asr_credential_ref.clone() {
            Some(reference) => self.persist_string(KEY_ASR_AZURE_CREDENTIAL_REF, &reference),
            None => self.delete_string(KEY_ASR_AZURE_CREDENTIAL_REF),
        }
        self.persist_string(KEY_ASR_AZURE_READY, "false");
        self.refresh_values();
    }

    pub(crate) fn mark_azure_asr_ready(&mut self, ready: bool) {
        self.saved.azure_asr_ready = Some(ready);
        self.persist_string(KEY_ASR_AZURE_READY, if ready { "true" } else { "false" });
        self.refresh_values();
    }

    pub(crate) fn activate_azure_asr(&mut self) -> bool {
        if !self.values.azure_asr_profile().is_ready() {
            return false;
        }
        self.saved.asr_provider = Some(asr::AsrProviderId::CloudAzure);
        self.persist_string(KEY_ASR_PROVIDER, asr::AsrProviderId::CloudAzure.as_str());
        self.refresh_values();
        true
    }

    pub(crate) fn activate_local_sherpa_asr(&mut self) {
        self.saved.asr_provider = Some(asr::AsrProviderId::LocalSherpa);
        self.persist_string(KEY_ASR_PROVIDER, asr::AsrProviderId::LocalSherpa.as_str());
        self.refresh_values();
    }
}

pub(crate) fn resolve_builtin_asr_profile(
    values: &VoiceSettingsValues,
    model_root: &Path,
) -> Result<asr::BuiltinAsrProfile, String> {
    match values.asr_provider() {
        asr::AsrProviderId::LocalSherpa => {
            let model = super::resolve_available_asr_model(values, model_root)
                .map_err(|error| error.to_string())?;
            Ok(asr::BuiltinAsrProfile::LocalSherpa(model))
        }
        asr::AsrProviderId::CloudAzure => {
            let profile = values.azure_asr_profile();
            if !profile.is_ready() {
                return Err("Azure Speech is not validated".to_string());
            }
            let credential_ref = profile
                .credential_ref()
                .ok_or_else(|| "Azure Speech API Key is not configured".to_string())?;
            Ok(asr::BuiltinAsrProfile::CloudAzure(
                asr::AzureSpeechProfile {
                    region: profile.region().to_string(),
                    locale: profile.locale().to_string(),
                    credential_ref: credential_ref.to_string(),
                },
            ))
        }
    }
}
