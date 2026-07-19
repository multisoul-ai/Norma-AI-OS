use super::MultisoulApp;
use crate::settings::voice::{TtsModelInstallState, VoiceSettingsState};
#[cfg(feature = "e2e-test-support")]
use gpui::Context;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsModelE2eSnapshot {
    pub id: String,
    pub display_name: String,
    pub voices: Vec<String>,
    pub installation: &'static str,
    pub selected: bool,
    pub action: &'static str,
    pub deletable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TtsProviderE2eSnapshot {
    pub active_provider_id: Option<String>,
    pub picker_open: bool,
    pub picker_provider_id: Option<String>,
    pub picker_local_model_id: Option<String>,
    pub picker_local_voice_id: Option<String>,
    pub picker_minimax_model_id: Option<String>,
    pub picker_minimax_voice_id: Option<String>,
    pub picker_minimax_voice_provenance: &'static str,
    pub picker_minimax_readiness: &'static str,
    pub minimax_model_id: Option<String>,
    pub minimax_voice_id: Option<String>,
    pub minimax_voice_provenance: &'static str,
    pub minimax_readiness: &'static str,
    pub minimax_visible_voice_ids: Vec<String>,
}

pub(super) fn snapshot_tts_provider(state: &VoiceSettingsState) -> TtsProviderE2eSnapshot {
    TtsProviderE2eSnapshot {
        active_provider_id: state
            .values()
            .active_tts_provider_id()
            .map(|id| id.as_str().to_string()),
        picker_open: state.tts_picker_open(),
        picker_provider_id: state.tts_picker_provider_id().map(ToOwned::to_owned),
        picker_local_model_id: state
            .tts_picker_local_model_id()
            .map(|id| id.as_str().to_string()),
        picker_local_voice_id: state
            .tts_picker_local_voice_id()
            .map(|id| id.as_str().to_string()),
        picker_minimax_model_id: state
            .tts_picker_minimax_model_id()
            .map(|id| id.as_str().to_string()),
        picker_minimax_voice_id: state
            .tts_picker_minimax_voice_id()
            .map(|id| id.as_str().to_string()),
        picker_minimax_voice_provenance: match state.tts_picker_minimax_provenance() {
            tts::VoiceSelectionProvenance::Catalog => "catalog",
            tts::VoiceSelectionProvenance::Manual => "manual",
        },
        picker_minimax_readiness: readiness_name(state.tts_picker_minimax_readiness()),
        minimax_model_id: state
            .values()
            .minimax_tts_model_id()
            .map(|id| id.as_str().to_string()),
        minimax_voice_id: state
            .values()
            .minimax_tts_voice_id()
            .map(|id| id.as_str().to_string()),
        minimax_voice_provenance: match state.values().minimax_tts_voice_provenance() {
            tts::VoiceSelectionProvenance::Catalog => "catalog",
            tts::VoiceSelectionProvenance::Manual => "manual",
        },
        minimax_readiness: readiness_name(state.minimax_readiness()),
        minimax_visible_voice_ids: state
            .filtered_minimax_voices()
            .into_iter()
            .map(|voice| voice.id().as_str().to_string())
            .collect(),
    }
}

fn readiness_name(readiness: tts::RemoteTtsReadiness) -> &'static str {
    match readiness {
        tts::RemoteTtsReadiness::Unconfigured => "unconfigured",
        tts::RemoteTtsReadiness::Configured => "configured",
        tts::RemoteTtsReadiness::Checking => "checking",
        tts::RemoteTtsReadiness::Ready => "ready",
        tts::RemoteTtsReadiness::TemporarilyUnavailable => "temporarily_unavailable",
        tts::RemoteTtsReadiness::InvalidCredential => "invalid_credential",
        tts::RemoteTtsReadiness::VoiceUnavailable => "voice_unavailable",
        tts::RemoteTtsReadiness::VoiceValidationRequired => "voice_validation_required",
    }
}

pub(super) fn snapshot_tts_models(state: &VoiceSettingsState) -> Vec<TtsModelE2eSnapshot> {
    state
        .tts_model_manager()
        .entries()
        .iter()
        .map(|entry| {
            let descriptor = entry.descriptor();
            let selected = if state.tts_picker_provider_id() == Some(tts::LOCAL_TTS_PROVIDER_ID) {
                state.tts_picker_local_model_id() == Some(descriptor.id())
            } else {
                state.values().tts_model_id() == Some(descriptor.id())
            };
            TtsModelE2eSnapshot {
                id: descriptor.id().as_str().to_string(),
                display_name: descriptor.display_name().to_string(),
                voices: descriptor
                    .voices()
                    .iter()
                    .map(|voice| voice.id().as_str().to_string())
                    .collect(),
                installation: installation_name(entry.installation()),
                selected,
                action: installation_action(entry.installation(), selected),
                deletable: matches!(entry.installation(), TtsModelInstallState::Ready) && !selected,
            }
        })
        .collect()
}

fn installation_name(installation: &TtsModelInstallState) -> &'static str {
    match installation {
        TtsModelInstallState::NotInstalled => "not_installed",
        TtsModelInstallState::Installing { .. } => "installing",
        TtsModelInstallState::Cancelling { .. } => "cancelling",
        TtsModelInstallState::Ready => "ready",
        TtsModelInstallState::Failed(_) => "failed",
    }
}

fn installation_action(installation: &TtsModelInstallState, selected: bool) -> &'static str {
    match installation {
        TtsModelInstallState::NotInstalled => "install",
        TtsModelInstallState::Installing { .. } => "cancel",
        TtsModelInstallState::Cancelling { .. } => "cancelling",
        TtsModelInstallState::Ready if selected => "selected",
        TtsModelInstallState::Ready => "select",
        TtsModelInstallState::Failed(_) => "retry",
    }
}

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn assume_minimax_connected_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.voice_settings.assume_minimax_connected_for_e2e();
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn assume_minimax_connected_with_system_voices_for_e2e(
        &mut self,
        count: usize,
        cx: &mut Context<Self>,
    ) {
        self.voice_settings
            .assume_minimax_connected_with_system_voices_for_e2e(count);
        cx.notify();
    }

    pub fn load_tts_selection_for_e2e(
        store: storage::SettingsStore,
    ) -> (Option<String>, Option<String>) {
        let state = VoiceSettingsState::load_from_store(store);
        (
            state
                .values()
                .tts_model_id()
                .map(|id| id.as_str().to_string()),
            state
                .values()
                .tts_voice_id()
                .map(|id| id.as_str().to_string()),
        )
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn set_fake_tts_transport_for_e2e(
        &mut self,
        enabled: bool,
        hold_install: bool,
        cx: &mut Context<Self>,
    ) {
        crate::settings::voice::tts_models::set_e2e_fake_tts_transport(enabled, hold_install);
        cx.notify();
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn release_fake_tts_transport_for_e2e(&mut self, cx: &mut Context<Self>) {
        crate::settings::voice::tts_models::release_e2e_fake_tts_transport();
        cx.notify();
    }
}
