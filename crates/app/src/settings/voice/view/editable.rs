use super::{value_pill, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::state::{VoiceSettingsField, VoiceSettingsState};
use gpui::{prelude::*, px, rgba, Context, MouseButton, MouseDownEvent};
use ui::Theme;

pub(crate) fn editable_value_pill(
    theme: Theme,
    state: &VoiceSettingsState,
    field: VoiceSettingsField,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    value_pill(theme, state.row_value(field), state.is_editing(field))
        .id(("settings-voice-field", field.index()))
        .debug_selector(move || voice_field_debug_selector(field))
        .w_full()
        .max_w(px(420.0))
        .cursor_text()
        .hover(move |style| style.border_color(rgba(with_alpha(theme.accent_orange, 0x80))))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                if event.click_count >= 2 {
                    app.on_voice_settings_edit(field, event, window, cx);
                }
            }),
        )
}

fn voice_field_debug_selector(field: VoiceSettingsField) -> String {
    format!(
        "settings-voice-field-{}",
        field.stable_name().replace('_', "-")
    )
}
