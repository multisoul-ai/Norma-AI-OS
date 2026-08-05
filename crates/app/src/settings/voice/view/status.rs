use crate::settings::voice::state::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Div};
use ui::Theme;

pub(super) fn status_message(theme: Theme, state: &VoiceSettingsState) -> Vec<Div> {
    let Some((message, is_error)) = state.status() else {
        return Vec::new();
    };
    vec![div()
        .text_size(px(13.0))
        .text_color(rgba(if is_error {
            theme.destructive
        } else {
            theme.accent_cyan
        }))
        .child(message.to_string())]
}
