use super::components::VoiceAction;
use super::credential_row::{credential_row, CredentialRowSpec};
use super::{settings_group, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{
    DesktopVoiceMode, VoiceRequirement, VoiceRequirementState, VoiceSettingsField,
    VoiceSettingsState,
};
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful, Window};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;
mod catalog;
mod item;
mod menu_state;
mod voices;

pub(super) fn desktop_mode_picker(
    theme: Theme,
    state: &VoiceSettingsState,
    shortcut: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mode = state.values().desktop_mode();
    let hint = match mode {
        DesktopVoiceMode::Legacy => ui::i18n::format(
            "voice.mode_hint_command",
            &[("shortcut", shortcut.to_string())],
        ),
        _ => ui::i18n::text("voice.have_a_real_time_conversation_with_norma").to_string(),
    };
    div()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .p(px(2.0))
                .rounded(px(22.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.text_primary, 0x24)))
                .children(
                    [
                        DesktopVoiceMode::Legacy,
                        DesktopVoiceMode::Doubao,
                        DesktopVoiceMode::GptLive,
                    ]
                    .map(|candidate| mode_tab(theme, candidate, mode == candidate, cx)),
                ),
        )
        .child(
            div()
                .px(px(4.0))
                .settings_text(TextRole::Helper, theme)
                .child(hint),
        )
}

fn mode_tab(
    theme: Theme,
    mode: DesktopVoiceMode,
    selected: bool,
    cx: &Context<MultisoulApp>,
) -> Stateful<Div> {
    let (selector, title) = match mode {
        DesktopVoiceMode::Legacy => (
            "settings-voice-desktop-mode-legacy",
            ui::i18n::text("voice.voice_commands"),
        ),
        DesktopVoiceMode::Doubao => (
            "settings-voice-desktop-mode-doubao",
            ui::i18n::text("voice.doubao_2_0_voice"),
        ),
        DesktopVoiceMode::GptLive => (
            "settings-voice-desktop-mode-gpt-live",
            ui::i18n::text("voice.live_name"),
        ),
    };
    let variant = ButtonCustomVariant::new(cx)
        .color(
            rgba(if selected {
                with_alpha(theme.accent_blue, 0x38)
            } else {
                0x00000000
            })
            .into(),
        )
        .foreground(rgba(theme.text_primary).into())
        .border(rgba(0x00000000).into())
        .hover(rgba(with_alpha(theme.accent_blue, 0x25)).into())
        .active(rgba(with_alpha(theme.accent_blue, 0x38)).into());
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .relative()
        .flex_auto()
        .min_w(px(0.0))
        .px(px(14.0))
        .h(px(40.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            Button::new(selector)
                .debug_selector(move || format!("{selector}-button"))
                .absolute()
                .inset_0()
                .size_full()
                .rounded(px(18.0))
                .custom(variant)
                .tooltip(title)
                .on_activate(cx.listener(move |app, _, _, cx| {
                    app.select_desktop_voice_mode(mode, cx);
                })),
        )
        .child(
            div()
                .relative()
                .settings_font(TextRole::RowTitle)
                .child(if selected {
                    format!("✓ {title}")
                } else {
                    title.to_string()
                }),
        )
}

pub(super) fn doubao_settings(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let configured = state.values().doubao_credential_ref().is_some();
    let microphone = state.requirement_state(VoiceRequirement::Microphone);
    let microphone_ready = microphone == VoiceRequirementState::Ready;
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(
            settings_group(theme, ui::i18n::text("voice.doubao_connection"))
                .child(
                    div()
                        .id("settings-voice-doubao-readiness")
                        .debug_selector(|| "settings-voice-doubao-readiness".to_string())
                        .settings_font(TextRole::Label)
                        .text_color(rgba(if configured && microphone_ready {
                            theme.accent_green
                        } else {
                            theme.accent_orange
                        }))
                        .child(if !microphone_ready {
                            ui::i18n::text("voice.microphone_permission_required")
                        } else if !configured {
                            ui::i18n::text("voice.api_key_required")
                        } else {
                            ui::i18n::text("voice.ready_to_connect")
                        }),
                )
                .child(
                    div()
                        .id("settings-doubao-api-key")
                        .debug_selector(|| "settings-doubao-api-key".to_string())
                        .child(credential_row(
                            theme,
                            CredentialRowSpec {
                                label: "voice.ark_api_key",
                                hint: "voice.stored_in_this_mac_s_local_database_never_written_to_logs",
                                field: VoiceSettingsField::DoubaoApiKey,
                                configured,
                                pending: state.credential_persistence_pending(),
                                status_id: "settings-doubao-api-key-status",
                                save_id: "settings-doubao-api-key-save",
                                clear_id: "settings-doubao-api-key-clear",
                            },
                            inputs,
                            cx,
                            |app, _, cx| {
                                app.stop_doubao_voice(cx);
                                app.voice_settings.clear_doubao_credential();
                                app.persist_voice_settings_in_background(cx);
                            },
                        )),
                ),
        )
        .child(
            settings_group(theme, ui::i18n::text("voice.audio"))
                .child(
                    div()
                        .id("settings-voice-doubao-system-input")
                        .debug_selector(|| "settings-voice-doubao-system-input".to_string())
                        .settings_text(TextRole::Body, theme)
                        .child(ui::i18n::text("voice.microphone_macos_system_default")),
                )
                .child(
                    div()
                        .id("settings-voice-doubao-microphone-status")
                        .debug_selector(|| "settings-voice-doubao-microphone-status".to_string())
                        .settings_font(TextRole::Label)
                        .text_color(rgba(if microphone_ready {
                            theme.accent_green
                        } else {
                            theme.accent_orange
                        }))
                        .child(if microphone_ready {
                            ui::i18n::text("voice.microphone_permission_authorized")
                        } else {
                            ui::i18n::text("voice.microphone_permission_required_2")
                        }),
                )
                .when(
                    matches!(
                        microphone,
                        VoiceRequirementState::Missing | VoiceRequirementState::Failed
                    ),
                    |group| {
                        group.child(
                            super::action_button(
                                theme,
                                if microphone == VoiceRequirementState::Failed {
                                    ui::i18n::text("chrome.open_microphone_settings")
                                } else {
                                    ui::i18n::text("voice.grant_access")
                                },
                                theme.accent_blue,
                            )
                            .id("settings-voice-doubao-microphone-action")
                            .debug_selector(|| {
                                "settings-voice-doubao-microphone-action".to_string()
                            })
                            .on_click(cx.listener(MultisoulApp::on_voice_setup_request_microphone)),
                        )
                    },
                ),
        )
        .child(voices::voice_picker(theme, state, window, cx))
}
