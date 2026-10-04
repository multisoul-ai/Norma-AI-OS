use super::with_alpha;
use crate::settings::voice::state::VoiceSettingsState;
use gpui::{div, prelude::*, px, rgba, Div};
use ui::Theme;

pub(super) fn status_message(theme: Theme, state: &VoiceSettingsState) -> Vec<Div> {
    let Some((message, is_error)) = state.status() else {
        return Vec::new();
    };
    let color = if is_error {
        theme.destructive
    } else {
        theme.accent_blue
    };
    vec![div()
        .w_full()
        .p(px(12.0))
        .rounded(px(10.0))
        .bg(rgba(with_alpha(color, 0x12)))
        .text_size(px(13.0))
        .text_color(rgba(color))
        .child(message.to_string())]
}
