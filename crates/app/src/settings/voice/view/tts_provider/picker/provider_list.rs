use super::{status_badge, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful};
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
        tts::RemoteAccountStatus::Ready => ui::i18n::text("voice.connected_account"),
        tts::RemoteAccountStatus::Checking => ui::i18n::text("voice.connecting"),
        tts::RemoteAccountStatus::InvalidCredential
        | tts::RemoteAccountStatus::TemporarilyUnavailable => {
            ui::i18n::text("guide.needs_attention")
        }
        tts::RemoteAccountStatus::Unchecked => {
            if state.minimax_credential_configured() {
                ui::i18n::text("voice.connect_to_verify")
            } else {
                ui::i18n::text("voice.not_connected")
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
                .settings_text(TextRole::Eyebrow, theme)
                .child(ui::i18n::text("voice.providers")),
        )
        .child(provider_button(
            theme,
            ui::i18n::text("voice.on_device"),
            if local_ready {
                ui::i18n::text("voice.ready_offline")
            } else {
                ui::i18n::text("voice.models_required")
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
                .settings_text(TextRole::RowTitle, theme)
                .child(title)
                .when(active, |row| {
                    row.child(status_badge(
                        ui::i18n::text("voice.current"),
                        theme.accent_green,
                    ))
                }),
        )
        .child(div().settings_text(TextRole::Caption, theme).child(detail))
        .on_click(listener)
}
