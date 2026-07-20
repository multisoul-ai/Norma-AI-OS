use super::{status_badge, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, Stateful};
use ui::Theme;

pub(super) fn provider_list(
    theme: Theme,
    state: &VoiceSettingsState,
    selected_provider: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let active = state
        .values()
        .active_tts_provider_id()
        .map(tts::TtsProviderId::as_str);
    let local_ready = state.tts_model_manager().entries().iter().any(|entry| {
        matches!(
            entry.installation(),
            crate::settings::voice::TtsModelInstallState::Ready
        )
    });
    let minimax_status = match state.minimax_account_status() {
        tts::RemoteAccountStatus::Ready => "Connected account",
        tts::RemoteAccountStatus::Checking => "Connecting…",
        tts::RemoteAccountStatus::InvalidCredential
        | tts::RemoteAccountStatus::TemporarilyUnavailable => "Needs attention",
        tts::RemoteAccountStatus::Unchecked => {
            if state.minimax_credential_configured() {
                "Connect to verify"
            } else {
                "Not connected"
            }
        }
    };
    div()
        .w(px(150.0))
        .flex()
        .flex_none()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_tertiary))
                .child("PROVIDERS"),
        )
        .child(provider_button(
            theme,
            "On-device",
            if local_ready {
                "Ready offline"
            } else {
                "Models required"
            },
            "settings-tts-picker-provider-local",
            selected_provider == tts::LOCAL_TTS_PROVIDER_ID,
            active == Some(tts::LOCAL_TTS_PROVIDER_ID),
            cx.listener(MultisoulApp::on_voice_pick_local_tts),
        ))
        .child(provider_button(
            theme,
            "MiniMax",
            minimax_status,
            "settings-tts-picker-provider-minimax",
            selected_provider == tts::MINIMAX_TTS_PROVIDER_ID,
            active == Some(tts::MINIMAX_TTS_PROVIDER_ID),
            cx.listener(MultisoulApp::on_voice_pick_minimax_tts),
        ))
}

fn provider_button(
    theme: Theme,
    title: &'static str,
    detail: &'static str,
    selector: &'static str,
    selected: bool,
    active: bool,
    listener: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .p(px(10.0))
        .rounded(px(9.0))
        .border_1()
        .border_color(rgba(with_alpha(
            if selected {
                theme.accent_blue
            } else {
                theme.edge_highlight
            },
            if selected { 0x72 } else { 0x35 },
        )))
        .bg(rgba(with_alpha(
            if selected {
                theme.accent_blue
            } else {
                theme.panel
            },
            if selected { 0x16 } else { 0x78 },
        )))
        .cursor_pointer()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(12.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child(title)
                .when(active, |row| {
                    row.child(status_badge("CURRENT", theme.accent_green))
                }),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(rgba(theme.text_tertiary))
                .child(detail),
        )
        .on_click(listener)
}
