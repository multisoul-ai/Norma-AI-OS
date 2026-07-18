use super::super::model_catalog::model_catalog_row;
use super::super::tts_catalog::tts_catalog_row;
use super::super::{action_button, with_alpha};
use super::components::compact_setup_row;
use crate::app::MultisoulApp;
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState, VoiceSettingsState};
use gpui::{div, prelude::*, px, relative, rgba, Context, Div, Stateful};
use ui::Theme;

pub(super) fn recommended_package_detail(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let installing = matches!(
        state.requirement_state(VoiceRequirement::SpeechRecognition),
        VoiceRequirementState::Installing { .. }
    ) || matches!(
        state.requirement_state(VoiceRequirement::SpokenReplies),
        VoiceRequirementState::Installing { .. }
    );
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(compact_setup_row(
            theme,
            "Chinese + English · Recommended · 644 MB",
            "Zipformer recognition + Matcha spoken voice",
            action_button(
                theme,
                if installing {
                    "Cancel Download"
                } else {
                    "Install"
                },
                theme.accent_blue,
            )
            .id("voice-setup-install-recommended")
            .debug_selector(|| "voice-setup-install-recommended".to_string())
            .when(!installing, |button| {
                button.on_click(cx.listener(MultisoulApp::on_voice_setup_install_recommended))
            })
            .when(installing, |button| {
                button.on_click(cx.listener(MultisoulApp::on_voice_setup_cancel_recommended))
            }),
        ))
        .when_some(
            state.selected_model_install_progress(),
            |section, (completed, total)| section.child(download_progress(theme, completed, total)),
        )
}

fn download_progress(theme: Theme, completed: u64, total: u64) -> Stateful<Div> {
    let completed = completed.min(total);
    let completed_mb = completed / 1_000_000;
    let total_mb = total / 1_000_000;
    let percent = if total == 0 {
        0
    } else {
        completed.saturating_mul(100) / total
    };
    let ratio = if total == 0 {
        0.0
    } else {
        completed as f32 / total as f32
    };
    div()
        .id("voice-setup-download-progress")
        .debug_selector(|| "voice-setup-download-progress".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .px(px(2.0))
        .child(
            div()
                .debug_selector(move || {
                    format!("voice-setup-download-values-{percent}-{completed_mb}-{total_mb}")
                })
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .text_size(px(11.0))
                .text_color(rgba(theme.text_secondary))
                .child(format!("Downloading · {percent}%"))
                .child(format!("{completed_mb} MB / {total_mb} MB")),
        )
        .child(
            div()
                .w_full()
                .h(px(5.0))
                .rounded(px(2.5))
                .bg(rgba(with_alpha(theme.node_stroke, 0x48)))
                .child(
                    div()
                        .h_full()
                        .w(relative(ratio))
                        .rounded(px(2.5))
                        .bg(rgba(theme.accent_blue)),
                ),
        )
}

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
            "Prefer another local model?",
            "Choose any installed ASR, TTS model or voice without leaving Voice Setup",
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
                    .child(model_catalog_row(theme, state, cx))
                    .child(tts_catalog_row(theme, state, cx)),
            )
        })
}
