use super::*;
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState};

pub(super) fn row(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> gpui::Stateful<Div> {
    let permission =
        state.requirement_state(VoiceRequirement::Microphone) == VoiceRequirementState::Ready;
    let device = state
        .input_devices()
        .iter()
        .find(|device| device.is_default);
    let hint = if !permission {
        ui::i18n::text("voice.microphone_permission_required_2").to_string()
    } else if state.input_devices().is_empty() {
        ui::i18n::text("voice.live_microphone_missing").to_string()
    } else if let Some(device) = device {
        ui::i18n::format(
            "voice.live_system_microphone",
            &[("device", device.name.clone())],
        )
    } else {
        ui::i18n::text("voice.live_microphone_hint").to_string()
    };
    // Capture follows the system default; do not expose a device selector that capture ignores.
    super::row(theme, ui::i18n::text("voice.microphone"), &hint)
        .id("settings-live-system-microphone")
        .debug_selector(|| "settings-live-system-microphone".into())
        .child(live_action(
            theme,
            "settings-live-sound-settings",
            ui::i18n::text("voice.live_open_sound_settings"),
            false,
            false,
            cx,
            |_, _, cx| cx.open_url("x-apple.systempreferences:com.apple.Sound-Settings.extension"),
        ))
        .child(live_action(
            theme,
            "settings-live-input-refresh",
            ui::i18n::text("voice.refresh"),
            false,
            false,
            cx,
            |app, _, cx| {
                app.voice_settings.refresh_input_devices();
                cx.notify();
            },
        ))
        .when(!permission, |row| {
            row.child(live_action(
                theme,
                "settings-live-microphone",
                ui::i18n::text("voice.grant_access"),
                false,
                false,
                cx,
                |app, _, cx| app.request_voice_microphone_access(cx),
            ))
        })
}
