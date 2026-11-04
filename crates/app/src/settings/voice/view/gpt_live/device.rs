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
    let ready = permission && device.is_some();
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
    div()
        .id("settings-live-system-microphone")
        .debug_selector(|| "settings-live-system-microphone".into())
        .w_full()
        .flex()
        .flex_wrap()
        .items_start()
        .justify_between()
        .gap(px(14.0))
        .p(px(20.0))
        .child(
            div()
                .flex_1()
                .min_w(px(180.0))
                .flex()
                .flex_col()
                .gap(px(7.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(9.0))
                        .child(icon("icons/microphone.svg", theme.text_secondary, 17.0))
                        .child(
                            div()
                                .settings_text(TextRole::RowTitle, theme)
                                .child(ui::i18n::text("voice.microphone")),
                        ),
                )
                .child(
                    div()
                        .pl(px(26.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_none()
                                .size(px(5.0))
                                .rounded_full()
                                .bg(rgba(if ready {
                                    theme.accent_green
                                } else {
                                    theme.accent_warm
                                })),
                        )
                        .child(helper(
                            theme,
                            hint.trim_start_matches(['●', ' ']).to_string(),
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(3.0))
                .when(!permission, |actions| {
                    actions.child(live_action(
                        theme,
                        "settings-live-microphone",
                        ui::i18n::text("voice.grant_access"),
                        false,
                        false,
                        cx,
                        |app, _, cx| app.request_voice_microphone_access(cx),
                    ))
                })
                .child(text_button(
                    theme,
                    "settings-live-sound-settings",
                    ui::i18n::text("voice.live_open_sound_settings"),
                    Some("icons/external-link.svg"),
                    cx,
                    |_, _, cx| {
                        cx.open_url("x-apple.systempreferences:com.apple.Sound-Settings.extension")
                    },
                ))
                .child(
                    div()
                        .id("settings-live-input-refresh")
                        .debug_selector(|| "settings-live-input-refresh".into())
                        .child(
                            Button::new("settings-live-input-refresh")
                                .ghost()
                                .compact()
                                .tooltip(ui::i18n::text("voice.live_refresh_microphone"))
                                .child(icon("icons/refresh.svg", theme.text_secondary, 16.0))
                                .on_click(cx.listener(|app, _, _, cx| {
                                    app.voice_settings.refresh_input_devices();
                                    cx.notify();
                                })),
                        ),
                ),
        )
}
