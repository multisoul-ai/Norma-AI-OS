use super::models::AsrModelManagerState;
use super::tts_models::TtsModelManagerState;
use asr::AsrModelId;
use std::path::PathBuf;
use storage::SettingsStore;
use tts::{TtsModelId, TtsVoiceId};
use voice_sherpa::{AudioInputDevice, SpeechError};
mod announcements;
mod asr_provider;
mod builders;
mod credentials;
mod doubao_voice;
#[cfg(feature = "e2e-test-support")]
mod e2e;
mod editing;
mod field;
mod gpt_live;
mod gpt_live_credentials;
use gpt_live_credentials::GptLiveCredentialRollback;
mod migration;
mod mode;
mod model_selection;
mod persistence;
mod provider_discovery;
mod provider_values;
mod readiness_state;
mod resync;
pub(crate) use resync::{provider_configuration_differs, tts_configuration_differs};
mod tts_picker;
mod tts_provider;
mod tts_provider_persistence;
mod values;
mod zipvoice_reference;
pub(crate) use asr_provider::resolve_builtin_asr_profile;
use asr_provider::AzureCredentialWriteRollback;
use credentials::CredentialWriteRollback;
pub(crate) use field::VoiceSettingsField;
pub(crate) use mode::{DesktopVoiceMode, DictationTrigger, VoiceInteractionMode};
pub(crate) use model_selection::{resolve_available_asr_model, VoiceSetupDestination};
pub(crate) use persistence::PendingVoiceSettingWrite;
pub(crate) use provider_discovery::ProviderDiscoveryStatus;
pub(crate) use provider_values::AgentProviderKind;
#[cfg(test)]
pub(crate) use provider_values::SavedProviderProfile;
use provider_values::KEY_AGENT_PROVIDER;
pub(crate) use tts_picker::TtsActivationRollback;
pub(crate) use tts_provider::MiniMaxConnectionFailureDisposition;
pub(crate) use values::{EnvVoiceSettings, SavedVoiceSettings, VoiceSettingsValues};
use values::{
    KEY_ASR_MODEL_ID, KEY_DESKTOP_MODE, KEY_INPUT_DEVICE_ID, KEY_INTERACTION_MODE,
    KEY_SINGLE_VOICE_COMMAND_TRIGGER, KEY_TRACE_ENABLED, KEY_TTS_MODEL_ID, KEY_TTS_VOICE_ID,
    KEY_VOICE_SETUP_COMPLETED,
};
use zipvoice_reference::ZipVoiceReferenceSettings;
#[derive(Clone, Debug, Eq, PartialEq)]
struct VoiceSettingsStatus {
    message: ui::i18n::LocalizedText,
    is_error: bool,
}
#[derive(Clone)]
pub(crate) struct VoiceSettingsState {
    values: VoiceSettingsValues,
    saved: SavedVoiceSettings,
    env: EnvVoiceSettings,
    storage: Option<SettingsStore>,
    input_devices: Vec<AudioInputDevice>,
    editing: Option<VoiceSettingsField>,
    draft: String,
    status: Option<VoiceSettingsStatus>,
    pending_writes: Vec<PendingVoiceSettingWrite>,
    persistence_running: bool,
    persistence_epoch: u64,
    resync_generation: u64,
    active_persistence_batch: Option<persistence::VoicePersistenceBatch>,
    credential_write_rollback: Option<CredentialWriteRollback>,
    azure_credential_write_rollback: Option<AzureCredentialWriteRollback>,
    minimax_credential_write_rollback: Option<tts_provider::MiniMaxCredentialWriteRollback>,
    minimax_credential_clear_rollback: Option<tts_provider::MiniMaxCredentialClearRollback>,
    minimax_validation_write_rollback: Option<tts_provider::MiniMaxValidationWriteRollback>,
    minimax_api_key_draft: Option<String>,
    doubao_credential_previous: Option<Option<String>>,
    gpt_live_credential_rollback: Option<GptLiveCredentialRollback>,
    gpt_live_credential_retry: Option<gpt_live_credentials::CredentialIntent>,
    gpt_live_key_expanded: bool,
    gpt_live_diagnostics_expanded: bool,
    doubao_voice_search: String,
    minimax_account_status: tts::RemoteAccountStatus,
    minimax_voice_catalog: tts::ProviderVoiceCatalog,
    minimax_validated_selection: Option<tts::TtsValidatedSelection>,
    minimax_voice_search: String,
    minimax_connection_generation: u64,
    minimax_connection_previous_status: tts::RemoteAccountStatus,
    minimax_profile_generation: u64,
    minimax_preview_generation: Option<u64>,
    tts_picker: Option<tts_picker::TtsPickerDraft>,
    model_root: PathBuf,
    model_manager: AsrModelManagerState,
    asr_model_selection_generation: u64,
    asr_model_scan_generation: u64,
    vad_requirement: super::VoiceRequirementState,
    vad_validation_generation: u64,
    tts_model_manager: TtsModelManagerState,
    zipvoice_reference: ZipVoiceReferenceSettings,
    microphone_requirement: super::VoiceRequirementState,
    provider_requirement: super::VoiceRequirementState,
    provider_discovery: ProviderDiscoveryStatus,
    provider_discovery_generation: u64,
    setup_was_completed: bool,
    #[cfg(feature = "e2e-test-support")]
    readiness_override: Option<super::VoiceEntryState>,
    reviewing_setup: bool,
    input_device_picker_expanded: bool,
    setup_destination: VoiceSetupDestination,
    advanced_expanded: bool,
}
impl VoiceSettingsState {
    pub(crate) fn credential_store(&self) -> Option<SettingsStore> {
        self.storage.clone()
    }

    pub(crate) fn load_from_store(store: SettingsStore) -> Self {
        match SavedVoiceSettings::load(&store) {
            Ok(saved) => {
                let mut state = Self::from_sources(saved, EnvVoiceSettings::from_env());
                state.storage = Some(store);
                state.queue_legacy_single_voice_command_migration();
                state.queue_legacy_api_key_migrations();
                state
            }
            Err(error) => {
                let mut state =
                    Self::from_sources(SavedVoiceSettings::default(), EnvVoiceSettings::from_env());
                state.storage = Some(store);
                state.status = Some(error_status(ui::i18n::message(
                    "voice.could_not_load_voice_settings_error",
                    &[("error", (error).to_string())],
                )));
                state
            }
        }
    }
    pub(crate) fn from_sources(saved: SavedVoiceSettings, env: EnvVoiceSettings) -> Self {
        let model_root = asr::default_model_installation_root().unwrap_or_default();
        Self::from_sources_with_model_root(saved, env, model_root)
    }
    pub(crate) fn from_sources_with_model_root(
        saved: SavedVoiceSettings,
        env: EnvVoiceSettings,
        model_root: PathBuf,
    ) -> Self {
        let values = VoiceSettingsValues::resolve(&saved, &env);
        let minimax_validated_selection = resync::minimax_validated_selection(&saved);
        let setup_was_completed = saved.setup_completed.unwrap_or(false);
        let model_manager = AsrModelManagerState::unscanned(&asr::builtin_model_catalog());
        let tts_model_manager = TtsModelManagerState::unscanned(&tts::builtin_model_catalog());
        Self {
            values,
            saved,
            env,
            storage: None,
            input_devices: Vec::new(),
            editing: None,
            draft: String::new(),
            status: None,
            pending_writes: Vec::new(),
            persistence_running: false,
            persistence_epoch: 0,
            resync_generation: 0,
            active_persistence_batch: None,
            credential_write_rollback: None,
            azure_credential_write_rollback: None,
            minimax_credential_write_rollback: None,
            minimax_credential_clear_rollback: None,
            minimax_validation_write_rollback: None,
            minimax_api_key_draft: None,
            doubao_credential_previous: None,
            gpt_live_credential_rollback: None,
            gpt_live_credential_retry: None,
            gpt_live_key_expanded: false,
            gpt_live_diagnostics_expanded: false,
            doubao_voice_search: String::new(),
            minimax_account_status: tts::RemoteAccountStatus::Unchecked,
            minimax_voice_catalog: tts::ProviderVoiceCatalog::default(),
            minimax_validated_selection,
            minimax_voice_search: String::new(),
            minimax_connection_generation: 0,
            minimax_connection_previous_status: tts::RemoteAccountStatus::Unchecked,
            minimax_profile_generation: 0,
            minimax_preview_generation: None,
            tts_picker: None,
            model_root,
            model_manager,
            asr_model_selection_generation: 0,
            asr_model_scan_generation: 0,
            vad_requirement: super::VoiceRequirementState::Missing,
            vad_validation_generation: 0,
            tts_model_manager,
            zipvoice_reference: ZipVoiceReferenceSettings::default(),
            microphone_requirement: super::VoiceRequirementState::Missing,
            provider_requirement: super::VoiceRequirementState::Missing,
            provider_discovery: ProviderDiscoveryStatus::Idle,
            provider_discovery_generation: 0,
            setup_was_completed,
            #[cfg(feature = "e2e-test-support")]
            readiness_override: None,
            reviewing_setup: false,
            input_device_picker_expanded: false,
            setup_destination: VoiceSetupDestination::Overview,
            advanced_expanded: false,
        }
    }
    pub(crate) fn values(&self) -> &VoiceSettingsValues {
        &self.values
    }
    pub(crate) fn editing_field(&self) -> Option<VoiceSettingsField> {
        self.editing
    }
    pub(crate) fn active_agent_model(&self) -> &str {
        self.values.active_provider_profile().model()
    }
    pub(crate) fn reviewing_setup(&self) -> bool {
        self.reviewing_setup
    }
    pub(crate) fn set_reviewing_setup(&mut self, reviewing: bool) {
        self.reviewing_setup = reviewing;
        if !reviewing {
            self.setup_destination = VoiceSetupDestination::Overview;
        }
    }
    pub(crate) fn input_devices(&self) -> &[AudioInputDevice] {
        &self.input_devices
    }
    pub(crate) fn input_device_picker_expanded(&self) -> bool {
        self.input_device_picker_expanded
    }

    pub(crate) fn toggle_input_device_picker(&mut self) {
        self.input_device_picker_expanded = !self.input_device_picker_expanded;
    }

    pub(crate) fn close_input_device_picker(&mut self) {
        self.input_device_picker_expanded = false;
    }

    pub(crate) fn select_interaction_mode(&mut self, mode: VoiceInteractionMode) {
        self.saved.interaction_mode = Some(mode);
        self.persist_string(KEY_INTERACTION_MODE, mode.as_str());
        self.refresh_values();
    }

    pub(crate) fn select_desktop_mode(&mut self, mode: DesktopVoiceMode) {
        if self.values.desktop_mode() != mode {
            self.status = None;
            #[cfg(feature = "e2e-test-support")]
            {
                self.readiness_override = None;
            }
        }
        self.saved.desktop_mode = Some(mode);
        self.persist_string(KEY_DESKTOP_MODE, mode.as_str());
        self.refresh_values();
    }

    fn save_doubao_credential(&mut self, secret: String) {
        const ACCOUNT: &str = "doubao-desktop";
        let must_store_reference = self.saved.doubao_credential_ref.as_deref() != Some(ACCOUNT);
        if self.doubao_credential_previous.is_none() {
            self.doubao_credential_previous = Some(self.saved.doubao_credential_ref.clone());
        }
        self.queue_pending_write(PendingVoiceSettingWrite::StoreCredential {
            store: self.storage.clone(),
            account: ACCOUNT.to_string(),
            secret,
        });
        self.saved.doubao_credential_ref = Some(ACCOUNT.to_string());
        if must_store_reference {
            self.persist_string(values::KEY_DOUBAO_CREDENTIAL_REF, ACCOUNT);
        }
        self.refresh_values();
    }

    pub(crate) fn clear_doubao_credential(&mut self) {
        self.doubao_credential_previous = None;
        self.saved.doubao_credential_ref = None;
        self.delete_string(values::KEY_DOUBAO_CREDENTIAL_REF);
        self.queue_pending_write(PendingVoiceSettingWrite::DeleteCredential {
            store: self.storage.clone(),
            account: "doubao-desktop".to_string(),
        });
        self.refresh_values();
    }

    pub(crate) fn select_dictation_trigger(&mut self, trigger: DictationTrigger) {
        self.saved.dictation_trigger = Some(trigger);
        self.persist_string(KEY_SINGLE_VOICE_COMMAND_TRIGGER, trigger.as_str());
        self.refresh_values();
    }

    pub(crate) fn select_asr_model(&mut self, model_id: Option<AsrModelId>) {
        self.saved.asr_model_id = model_id.clone();
        match model_id {
            Some(model_id) => self.persist_string(KEY_ASR_MODEL_ID, model_id.as_str()),
            None => self.delete_string(KEY_ASR_MODEL_ID),
        }
        self.refresh_values();
    }

    pub(crate) fn select_tts_model_and_voice(
        &mut self,
        model_id: Option<TtsModelId>,
        voice_id: Option<TtsVoiceId>,
    ) {
        self.saved.tts_model_id = model_id.clone();
        self.saved.tts_voice_id = voice_id.clone();
        match model_id {
            Some(id) => self.persist_string(KEY_TTS_MODEL_ID, id.as_str()),
            None => self.delete_string(KEY_TTS_MODEL_ID),
        }
        match voice_id {
            Some(id) => self.persist_string(KEY_TTS_VOICE_ID, id.as_str()),
            None => self.delete_string(KEY_TTS_VOICE_ID),
        }
        self.refresh_values();
    }

    pub(crate) fn select_input_device(&mut self, id: Option<String>) {
        self.saved.input_device_id = id
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty());
        match self.saved.input_device_id.clone() {
            Some(id) => self.persist_string(KEY_INPUT_DEVICE_ID, &id),
            None => self.delete_string(KEY_INPUT_DEVICE_ID),
        }
        self.refresh_values();
    }

    pub(crate) fn refresh_input_devices(&mut self) {
        self.refresh_input_devices_from_result(
            voice_sherpa::list_audio_input_devices().map_err(SpeechError::from),
        );
    }

    pub(crate) fn refresh_input_devices_from_result(
        &mut self,
        result: Result<Vec<AudioInputDevice>, SpeechError>,
    ) {
        match result {
            Ok(devices) => {
                self.input_devices = devices;
                self.status = Some(ok_status(ui::i18n::LocalizedText::new(
                    "voice.input_devices_refreshed",
                )));
            }
            Err(error) => {
                self.status = Some(error_status(ui::i18n::message(
                    "voice.could_not_refresh_input_devices_error",
                    &[("error", (error).to_string())],
                )));
            }
        }
    }

    pub(crate) fn toggle_trace(&mut self) {
        let next = !self.values.trace_enabled;
        self.saved.trace_enabled = Some(next);
        self.persist_string(KEY_TRACE_ENABLED, if next { "true" } else { "false" });
        self.refresh_values();
    }

    pub(crate) fn select_agent_provider(&mut self, provider: AgentProviderKind) {
        self.saved.agent_provider = Some(provider);
        self.persist_string(KEY_AGENT_PROVIDER, provider.as_str());
        self.cancel_edit();
        self.invalidate_provider_requirement();
        self.refresh_values();
    }

    pub(crate) fn clear_agent_api_key(&mut self) {
        self.cancel_edit_field(VoiceSettingsField::ApiKey);
        self.clear_active_provider_credential();
        self.refresh_values();
    }

    pub(crate) fn status(&self) -> Option<(&str, bool)> {
        self.status
            .as_ref()
            .map(|status| (status.message.as_str(), status.is_error))
    }

    fn persist_string(&mut self, key: &str, value: &str) {
        if let Some(store) = self.storage.clone() {
            self.queue_pending_write(PendingVoiceSettingWrite::Set {
                store,
                key: key.to_string(),
                value: value.to_string(),
            });
            self.status = Some(ok_status(ui::i18n::LocalizedText::new("voice.saving")));
        } else {
            self.status = Some(ok_status(ui::i18n::LocalizedText::new(
                "voice.saved_for_this_session",
            )));
        }
    }

    fn delete_string(&mut self, key: &str) {
        if let Some(store) = self.storage.clone() {
            self.queue_pending_write(PendingVoiceSettingWrite::Delete {
                store,
                key: key.to_string(),
            });
            self.status = Some(ok_status(ui::i18n::LocalizedText::new("voice.saving")));
        } else {
            self.status = Some(ok_status(ui::i18n::LocalizedText::new(
                "voice.using_default_for_this_session",
            )));
        }
    }

    fn queue_pending_write(&mut self, write: PendingVoiceSettingWrite) {
        self.pending_writes
            .retain(|pending| pending.key() != write.key());
        self.pending_writes.push(write);
    }

    fn refresh_values(&mut self) {
        self.values = VoiceSettingsValues::resolve(&self.saved, &self.env);
    }
}

fn ok_status(message: impl Into<ui::i18n::LocalizedText>) -> VoiceSettingsStatus {
    VoiceSettingsStatus {
        message: message.into(),
        is_error: false,
    }
}

fn error_status(message: impl Into<ui::i18n::LocalizedText>) -> VoiceSettingsStatus {
    VoiceSettingsStatus {
        message: message.into(),
        is_error: true,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_tests.rs"]
mod voice_settings_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_doubao_tests.rs"]
mod voice_settings_doubao_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_asr_tests.rs"]
mod voice_settings_asr_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_persistence_tests.rs"]
mod voice_settings_persistence_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_provider_persistence_tests.rs"]
mod voice_settings_provider_persistence_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_disclosure_tests.rs"]
mod voice_settings_disclosure_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_tts_provider_tests.rs"]
mod voice_settings_tts_provider_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_tts_picker_tests.rs"]
mod voice_settings_tts_picker_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_tts_activation_tests.rs"]
mod voice_settings_tts_activation_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_tts_credential_tests.rs"]
mod voice_settings_tts_credential_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_tts_persistence_tests.rs"]
mod voice_settings_tts_persistence_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_resync_tests.rs"]
mod voice_settings_resync_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_runtime_sync_tests.rs"]
mod runtime_sync_tests;

#[cfg(test)]
#[path = "../../../tests/unit/voice_settings_gpt_live_tests.rs"]
mod voice_settings_gpt_live_tests;
