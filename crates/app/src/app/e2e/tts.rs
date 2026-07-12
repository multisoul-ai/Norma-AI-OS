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

pub(super) fn snapshot_tts_models(state: &VoiceSettingsState) -> Vec<TtsModelE2eSnapshot> {
    state
        .tts_model_manager()
        .entries()
        .iter()
        .map(|entry| {
            let descriptor = entry.descriptor();
            let selected = state.values().tts_model_id() == Some(descriptor.id());
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
}
