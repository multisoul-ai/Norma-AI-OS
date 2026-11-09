use super::super::with_alpha;
use super::super::VoiceSettingsInputs;
use super::picker::active_voice_summary;
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Context, Stateful};
use ui::Theme;

pub(in crate::settings::voice::view) fn spoken_reply_row(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<gpui::Div> {
    let (provider, voice, detail) = active_voice_summary(state);
    let hover = with_alpha(theme.text_primary, 0x10);
    let pressed = with_alpha(theme.text_primary, 0x1c);
    div()
        .id("settings-tts-change-voice")
        .debug_selector(|| "settings-tts-change-voice".to_string())
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(10.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .border_1()
        .border_color(rgba(0x00000000))
        .hover(move |row| row.bg(rgba(hover)))
        .active(move |row| row.bg(rgba(pressed)))
        .tab_index(0)
        .track_focus(&inputs.tts_row_focus)
        .focus(move |row| row.border_color(rgba(theme.accent_blue)))
        .on_click(cx.listener(MultisoulApp::on_voice_open_tts_picker))
        .on_key_down(cx.listener(|app, event: &gpui::KeyDownEvent, window, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                app.on_voice_open_tts_picker(&gpui::ClickEvent::default(), window, cx);
                cx.stop_propagation();
            }
        }))
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
                        .child(ui::i18n::text("voice.spoken_replies")),
                )
                .child(div().settings_text(TextRole::Helper, theme).child(detail)),
        )
        .child(
            div()
                .flex_none()
                .max_w(px(220.0))
                .flex()
                .flex_col()
                .items_end()
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .text_color(rgba(theme.accent_blue))
                        .child(voice),
                )
                .child(
                    div()
                        .settings_text(TextRole::Caption, theme)
                        .child(provider),
                ),
        )
}
