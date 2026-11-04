use super::*;
use crate::settings::voice::VoiceSettingsField;
use gpui::Focusable;

/// API Key row: collapsed once saved; the masked editor expands in place and never echoes the key.
pub(super) fn key_card(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let configured = state.values().gpt_live_profile().credential_ref().is_some();
    let pending = state.credential_persistence_pending();
    let editing = state.gpt_live_key_expanded()
        && state.editing_field() == Some(VoiceSettingsField::GptLiveApiKey);
    let input = inputs.input(VoiceSettingsField::GptLiveApiKey);
    let retry = state.has_gpt_live_credential_retry();
    let (status, status_color, status_icon) = if pending {
        ("voice.saving", theme.text_secondary, None)
    } else if configured {
        (
            "voice.live_key_saved",
            theme.accent_green,
            Some("icons/check.svg"),
        )
    } else {
        ("voice.api_key_required", theme.accent_warm, None)
    };
    let row = div()
        .w_full()
        .flex()
        .items_center()
        .gap(px(10.0))
        .px(px(20.0))
        .py(px(12.0))
        .child(icon("icons/key.svg", theme.text_secondary, 18.0))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .settings_text(TextRole::Body, theme)
                .child(ui::i18n::text("voice.live_key_label")),
        )
        .child(
            div()
                .id("settings-live-key-status")
                .debug_selector(|| "settings-live-key-status".into())
                .flex()
                .items_center()
                .gap(px(5.0))
                .settings_font(TextRole::Caption)
                .text_color(rgba(status_color))
                .children(status_icon.map(|path| icon(path, status_color, 15.0)))
                .child(ui::i18n::text(status)),
        )
        .when(configured, |row| {
            row.child(
                text_button(
                    theme,
                    "settings-live-key-edit",
                    ui::i18n::text(if editing {
                        "voice.live_key_done"
                    } else {
                        "voice.live_key_change"
                    }),
                    None,
                    cx,
                    move |app, window, cx| {
                        if editing {
                            app.focus_handle.focus(window);
                            app.cancel_voice_text_input(VoiceSettingsField::GptLiveApiKey, cx);
                            app.flush_voice_text_input_autosave(cx);
                            app.voice_settings_inputs.sync_field_from_state(
                                VoiceSettingsField::GptLiveApiKey,
                                &app.voice_settings,
                                cx,
                            );
                        } else {
                            app.begin_voice_text_input(
                                VoiceSettingsField::GptLiveApiKey,
                                String::new(),
                                cx,
                            );
                            input.read(cx).focus_handle(cx).focus(window);
                        }
                        app.voice_settings.set_gpt_live_key_expanded(!editing);
                        cx.notify();
                    },
                )
                // Keep the editor focused until the click lands so blur cannot collapse it first.
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            )
        });
    super::card(theme)
        .child(row)
        .when(!configured || editing || retry, |card| {
            card.child(divider(theme)).child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .px(px(20.0))
                    .py(px(14.0))
                    .child(inputs.input(VoiceSettingsField::GptLiveApiKey))
                    .when(configured || retry, |editor| {
                        editor.child(actions(theme, configured, retry, pending, cx))
                    }),
            )
        })
}

fn actions(
    theme: Theme,
    configured: bool,
    retry: bool,
    pending: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex()
        .flex_wrap()
        .gap(px(8.0))
        .when(configured, |actions| {
            actions.child(
                live_action(
                    theme,
                    "settings-live-key-clear",
                    ui::i18n::text("common.remove"),
                    false,
                    pending,
                    cx,
                    |app, window, cx| {
                        app.focus_handle.focus(window);
                        app.cancel_voice_text_input(VoiceSettingsField::GptLiveApiKey, cx);
                        app.voice_settings.set_gpt_live_key_expanded(false);
                        app.stop_gpt_live_voice(cx);
                        app.voice_settings.clear_gpt_live_credential();
                        app.voice_settings_inputs.sync_field_from_state(
                            VoiceSettingsField::GptLiveApiKey,
                            &app.voice_settings,
                            cx,
                        );
                        app.persist_voice_settings_in_background(cx);
                        cx.notify();
                    },
                )
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            )
        })
        .when(retry, |actions| {
            actions.child(live_action(
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
