use super::super::credential_row::{credential_row, CredentialRowSpec};
use super::*;
use crate::settings::voice::VoiceSettingsField;

/// OpenAI key row. The editor stays on screen as an empty password field and never echoes the key.
pub(super) fn key_card(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let configured = state.values().gpt_live_profile().credential_ref().is_some();
    let pending = state.credential_persistence_pending();
    let retry = state.has_gpt_live_credential_retry();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(credential_row(
            theme,
            CredentialRowSpec {
                label: "voice.live_key_label",
                hint: "voice.live_key_hint",
                field: VoiceSettingsField::GptLiveApiKey,
                configured,
                pending,
                status_id: "settings-live-key-status",
                save_id: "settings-live-key-save",
                clear_id: "settings-live-key-clear",
            },
            inputs,
            cx,
            |app, window, cx| {
                app.focus_handle.focus(window);
                app.cancel_voice_text_input(VoiceSettingsField::GptLiveApiKey, cx);
                app.stop_gpt_live_voice(cx);
                app.voice_settings.clear_gpt_live_credential();
                app.voice_settings_inputs.sync_field_from_state(
                    VoiceSettingsField::GptLiveApiKey,
                    &app.voice_settings,
                    cx,
                );
                app.persist_voice_settings_in_background(cx);
            },
        ))
        .when(retry, |card| {
            card.child(super::live_action(
                theme,
                "settings-live-key-retry",
                ui::i18n::text("voice.live_retry_key"),
                false,
                pending,
                cx,
                |app, _, cx| {
                    app.stop_gpt_live_voice(cx);
                    app.voice_settings.retry_gpt_live_credential_change();
                    app.persist_voice_settings_in_background(cx);
                    cx.notify();
                },
            ))
        })
}
