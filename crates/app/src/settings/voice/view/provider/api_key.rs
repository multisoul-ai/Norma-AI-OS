use super::super::credential_row::{credential_row, CredentialRowSpec};
use super::super::VoiceSettingsInputs;
use crate::app::MultisoulApp;
use crate::settings::voice::state::{VoiceSettingsField, VoiceSettingsState};
use gpui::{Context, Div};
use ui::Theme;

pub(super) fn api_key_row(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let configured = state
        .values()
        .active_provider_profile()
        .credential_configured();
    credential_row(
        theme,
        CredentialRowSpec {
            label: "voice.understanding_api_key",
            hint: "voice.stored_in_this_mac_s_local_database_never_written_to_logs",
            field: VoiceSettingsField::ApiKey,
            configured,
            pending: state.credential_persistence_pending(),
            status_id: "settings-agent-provider-api-key-status",
            save_id: "settings-agent-provider-api-key-save",
            clear_id: "settings-agent-provider-api-key-clear",
        },
        inputs,
        cx,
        |app, _, cx| {
            app.voice_settings.clear_agent_api_key();
            app.voice_settings_inputs
                .sync_from_state(&app.voice_settings, cx);
            app.persist_voice_settings_in_background(cx);
        },
    )
}
