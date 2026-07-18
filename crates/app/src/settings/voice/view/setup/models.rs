use super::super::action_button;
use super::super::model_catalog::model_catalog_row;
use super::components::compact_setup_row;
use crate::app::MultisoulApp;
use crate::settings::voice::VoiceSettingsState;
use gpui::{div, prelude::*, px, Context, Div};
use ui::Theme;

pub(super) fn alternative_models_detail(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let expanded = state.setup_models_expanded();
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(compact_setup_row(
            theme,
            "Prefer another recognition model?",
            "Choose any installed ASR model without leaving Voice Setup",
            action_button(
                theme,
                if expanded {
                    "Hide Models"
                } else {
                    "Choose Other Models"
                },
                theme.accent_blue,
            )
            .id("voice-setup-alternative-models-toggle")
            .debug_selector(|| "voice-setup-alternative-models-toggle".to_string())
            .on_click(cx.listener(MultisoulApp::on_voice_setup_toggle_models)),
        ))
        .when(expanded, |section| {
            section.child(
                div()
                    .id("voice-setup-alternative-models")
                    .debug_selector(|| "voice-setup-alternative-models".to_string())
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(model_catalog_row(theme, state, cx)),
            )
        })
}
