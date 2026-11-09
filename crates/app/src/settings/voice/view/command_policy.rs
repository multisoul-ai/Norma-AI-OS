use super::{setting_row, settings_group};
use gpui::{prelude::*, Div};
use ui::Theme;

pub(super) fn command_policy_group(theme: Theme) -> Div {
    settings_group(theme, ui::i18n::text("voice.understanding_dispatch"))
        .child(setting_row(
            theme,
            ui::i18n::text("voice.canvas_target_policy"),
            ui::i18n::text("voice.canvas_target_policy_description"),
        ))
        .child(setting_row(
            theme,
            ui::i18n::text("voice.semantic_submit_policy"),
            ui::i18n::text("voice.semantic_submit_policy_description"),
        ))
}
