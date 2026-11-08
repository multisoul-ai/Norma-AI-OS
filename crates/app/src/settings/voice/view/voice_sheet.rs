use super::tts_catalog::zipvoice_reference_controls;
use super::tts_provider::{local_voice_picker, minimax_configuration};
use super::{action_button, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{TtsModelInstallState, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful, Window};
use ui::Theme;

pub(super) fn voice_sheet(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let provider = state
        .tts_picker_provider_id()
        .unwrap_or(tts::LOCAL_TTS_PROVIDER_ID);
    div()
        .id("settings-tts-picker")
        .debug_selector(|| "settings-tts-picker".to_string())
        .track_focus(&inputs.tts_sheet_focus)
        .tab_index(0)
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(14.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x2e)))
        .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
        .capture_key_down(cx.listener(MultisoulApp::on_voice_sheet_key))
        .child(
            div()
                .settings_text(TextRole::Headline, theme)
                .child(ui::i18n::text("voice.choose_a_voice")),
        )
        .child(
            div()
                .id("settings-tts-picker-current-notice")
                .debug_selector(|| "settings-tts-picker-current-notice".to_string())
                .w_full()
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .bg(rgba(with_alpha(theme.accent_blue, 0x12)))
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text(
                    "voice.your_current_voice_stays_active_until_you_choose_use_this_voice",
                )),
        )
        .child(engine_row(theme, provider, cx))
        .child(engine_body(theme, state, inputs, provider, window, cx))
        .child(footer(theme, state, cx))
}

fn engine_row(theme: Theme, provider: &str, cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .w_full()
        .flex()
        .gap(px(6.0))
        .child(engine_choice(
            theme,
            ui::i18n::text("voice.tts_engine_off"),
            "settings-tts-picker-provider-off",
            provider == tts::OFF_TTS_PROVIDER_ID,
            MultisoulApp::on_voice_pick_off_tts,
            cx,
        ))
        .child(engine_choice(
            theme,
            ui::i18n::text("voice.on_device"),
            "settings-tts-picker-provider-local",
            provider == tts::LOCAL_TTS_PROVIDER_ID,
            MultisoulApp::on_voice_pick_local_tts,
            cx,
        ))
        .child(engine_choice(
            theme,
            "MiniMax",
            "settings-tts-picker-provider-minimax",
            provider == tts::MINIMAX_TTS_PROVIDER_ID,
            MultisoulApp::on_voice_pick_minimax_tts,
            cx,
        ))
        .child(engine_choice(
            theme,
            ui::i18n::text("voice.system_narrator"),
            "settings-tts-picker-provider-system",
            provider == tts::SYSTEM_TTS_PROVIDER_ID,
            MultisoulApp::on_voice_pick_system_tts,
            cx,
        ))
}

fn engine_choice(
    theme: Theme,
    label: &'static str,
    selector: &'static str,
    selected: bool,
    listener: fn(&mut MultisoulApp, &gpui::ClickEvent, &mut Window, &mut Context<MultisoulApp>),
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let hover = with_alpha(theme.text_primary, 0x10);
    let pressed = with_alpha(theme.text_primary, 0x1c);
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .flex_1()
        .min_w(px(0.0))
        .px(px(8.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(if selected {
            theme.accent_blue
        } else {
            with_alpha(theme.edge_highlight, 0x55)
        }))
        .bg(rgba(if selected {
            with_alpha(theme.accent_blue, 0x18)
        } else {
            0x00000000
        }))
        .cursor_pointer()
        .tab_index(0)
        .hover(move |row| row.bg(rgba(hover)))
        .active(move |row| row.bg(rgba(pressed)))
        .focus(move |row| row.border_color(rgba(theme.accent_blue)))
        .child(div().settings_text(TextRole::RowTitle, theme).child(label))
        .on_click(cx.listener(listener))
}

fn engine_body(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    provider: &str,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    if provider == tts::OFF_TTS_PROVIDER_ID {
        return note(theme, ui::i18n::text("voice.tts_off_detail"));
    }
    if provider == tts::SYSTEM_TTS_PROVIDER_ID {
        return system_voices(theme, state, cx);
    }
    if provider == tts::MINIMAX_TTS_PROVIDER_ID {
        return minimax_configuration(theme, state, inputs, cx);
    }
    local_body(theme, state, window, cx)
}

fn note(theme: Theme, copy: &'static str) -> Div {
    div()
        .w_full()
        .p(px(12.0))
        .rounded(px(8.0))
        .bg(rgba(with_alpha(theme.text_primary, 0x08)))
        .settings_text(TextRole::Helper, theme)
        .child(copy)
}

fn local_body(
    theme: Theme,
    state: &VoiceSettingsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(local_voice_picker(theme, state, cx))
        .children(zipvoice_draft(theme, state, window, cx))
        .child(pending_models(theme, state, cx))
}

fn zipvoice_draft(
    theme: Theme,
    state: &VoiceSettingsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Option<Div> {
    let model_id = state.tts_picker_local_model_id()?;
    let (index, entry) = state
        .tts_model_manager()
        .entries()
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.descriptor().id() == model_id)?;
    if !entry.descriptor().capabilities().requires_voice_reference() {
        return None;
    }
    zipvoice_reference_controls(
        theme,
        state,
        entry.descriptor(),
        entry.installation(),
        index,
        window,
        cx,
    )
}

fn pending_models(theme: Theme, state: &VoiceSettingsState, cx: &mut Context<MultisoulApp>) -> Div {
    let mut rows = div().w_full().flex().flex_col().gap(px(6.0));
    for (index, entry) in state.tts_model_manager().entries().iter().enumerate() {
        if matches!(entry.installation(), TtsModelInstallState::Ready) {
            continue;
        }
        let id = entry.descriptor().id().clone();
        let installation = entry.installation().clone();
        rows = rows.child(
            div()
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x32)))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .settings_text(TextRole::RowTitle, theme)
                                .child(entry.descriptor().display_name().to_string()),
                        )
                        .child(
                            div()
                                .settings_text(TextRole::Caption, theme)
                                .child(install_label(&installation)),
                        ),
                )
                .child(install_action(theme, id, &installation, index, cx)),
        );
    }
    rows
}

fn install_label(installation: &TtsModelInstallState) -> &'static str {
    match installation {
        TtsModelInstallState::NotInstalled => ui::i18n::text("settings.not_installed"),
        TtsModelInstallState::Installing { .. } => ui::i18n::text("settings.installing"),
        TtsModelInstallState::Cancelling { .. } => ui::i18n::text("voice.cancelling_2"),
        TtsModelInstallState::Ready => ui::i18n::text("common.ready"),
        TtsModelInstallState::Failed(_) => ui::i18n::text("voice.installation_failed"),
    }
}

fn install_action(
    theme: Theme,
    id: tts::TtsModelId,
    installation: &TtsModelInstallState,
    index: usize,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    match installation {
        TtsModelInstallState::Installing { .. } => div().child(
            action_button(theme, ui::i18n::text("common.cancel"), theme.destructive)
                .id(("settings-tts-sheet-cancel", index))
                .debug_selector(move || format!("settings-tts-sheet-cancel-{index}"))
                .on_click(cx.listener(move |app, _, _, cx| app.cancel_tts_install(id.clone(), cx))),
        ),
        TtsModelInstallState::Cancelling { .. } | TtsModelInstallState::Ready => div(),
        TtsModelInstallState::Failed(_) => div().child(
            action_button(theme, ui::i18n::text("common.retry"), theme.destructive)
                .id(("settings-tts-sheet-retry", index))
                .debug_selector(move || format!("settings-tts-sheet-retry-{index}"))
                .on_click(cx.listener(move |app, _, _, cx| app.install_tts_model(id.clone(), cx))),
        ),
        TtsModelInstallState::NotInstalled => div().child(
            action_button(theme, ui::i18n::text("common.install"), theme.accent_orange)
                .id(("settings-tts-sheet-install", index))
                .debug_selector(move || format!("settings-tts-sheet-install-{index}"))
                .on_click(cx.listener(move |app, _, _, cx| app.install_tts_model(id.clone(), cx))),
        ),
    }
}

fn system_voices(theme: Theme, state: &VoiceSettingsState, cx: &mut Context<MultisoulApp>) -> Div {
    let selected = state.tts_picker_system_voice().map(tts::TtsVoiceId::as_str);
    let names = state.system_voice_names();
    if !state.system_voice_names_loaded() {
        return note(theme, ui::i18n::text("voice.tts_system_detail"));
    }
    if names.is_empty() {
        return note(theme, ui::i18n::text("voice.tts_system_unavailable"));
    }
    let mut rows = div().w_full().flex().flex_col().gap(px(6.0));
    for (index, name) in names.iter().enumerate() {
        let voice_name = name.clone();
        let preview_name = name.clone();
        let chosen = selected == Some(name.as_str());
        let hover = with_alpha(theme.text_primary, 0x10);
        let pressed = with_alpha(theme.text_primary, 0x1c);
        rows = rows.child(
            div()
                .id(("settings-tts-picker-system-voice", index))
                .debug_selector(move || format!("settings-tts-picker-system-voice-{index}"))
                .w_full()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(rgba(if chosen {
                    theme.accent_blue
                } else {
                    with_alpha(theme.edge_highlight, 0x32)
                }))
                .cursor_pointer()
                .tab_index(0)
                .hover(move |row| row.bg(rgba(hover)))
                .active(move |row| row.bg(rgba(pressed)))
                .focus(move |row| row.border_color(rgba(theme.accent_blue)))
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.on_voice_pick_system_voice(voice_name.clone(), cx)
                }))
                .child(
                    div()
                        .flex_1()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(name.clone()),
                )
                .child({
                    let voice = preview_name;
                    action_button(theme, ui::i18n::text("voice.preview"), theme.accent_blue)
                        .id(("settings-tts-picker-system-preview", index))
                        .debug_selector(move || {
                            format!("settings-tts-picker-system-preview-{index}")
                        })
                        .on_click(cx.listener(move |app, _, _, cx| {
                            cx.stop_propagation();
                            if let Ok(voice_id) = tts::TtsVoiceId::parse(voice.clone()) {
                                app.preview_system_voice(voice_id, cx);
                            }
                        }))
                }),
        );
    }
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("voice.tts_system_detail")),
        )
        .child(rows)
}

fn footer(theme: Theme, state: &VoiceSettingsState, cx: &mut Context<MultisoulApp>) -> Div {
    let ready = state.tts_picker_ready();
    div()
        .w_full()
        .flex()
        .items_center()
        .justify_end()
        .gap(px(8.0))
        .child(
            action_button(theme, ui::i18n::text("common.cancel"), theme.text_secondary)
                .id("settings-tts-picker-cancel")
                .debug_selector(|| "settings-tts-picker-cancel".to_string())
                .tab_index(0)
                .on_click(cx.listener(MultisoulApp::on_voice_cancel_tts_picker)),
        )
        .child({
            action_button(
                theme,
                if ready {
                    ui::i18n::text("voice.use_this_voice")
                } else {
                    ui::i18n::text("voice.choose_a_ready_voice")
                },
                if ready {
                    theme.accent_blue
                } else {
                    theme.text_tertiary
                },
            )
            .id("settings-tts-picker-apply")
            .debug_selector(|| "settings-tts-picker-apply".to_string())
            .tab_index(0)
            .when(ready, |button| {
                button.on_click(cx.listener(MultisoulApp::on_voice_apply_tts_picker))
            })
        })
}
