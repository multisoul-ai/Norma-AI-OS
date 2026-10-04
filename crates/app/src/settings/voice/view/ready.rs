use super::{components::component_action, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::voice::{
    DesktopVoiceMode, VoiceEntryState, VoiceRequirement, VoiceRequirementState, VoiceSettingsState,
};
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, Stateful};
use ui::Theme;

pub(super) fn voice_ready_overview(
    theme: Theme,
    voice_shortcut_label: &str,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_secondary))
                .child(format!(
                    "Press {voice_shortcut_label} to talk. Changes below are saved automatically."
                )),
        )
        .child(ready_summary(theme, cx))
}

pub(super) fn voice_status_overview(
    theme: Theme,
    state: &VoiceSettingsState,
    voice_shortcut_label: &str,
    cx: &Context<MultisoulApp>,
) -> Stateful<Div> {
    let ready = state.readiness() == VoiceEntryState::Ready;
    let doubao = state.values().desktop_mode() == DesktopVoiceMode::Doubao;
    let requirements = if doubao {
        vec![
            state.requirement_state(VoiceRequirement::Microphone),
            if state.values().doubao_credential_ref().is_some() {
                VoiceRequirementState::Ready
            } else {
                VoiceRequirementState::Missing
            },
        ]
    } else {
        [
            VoiceRequirement::Microphone,
            VoiceRequirement::SpeechRecognition,
            VoiceRequirement::AgentProvider,
            VoiceRequirement::SpokenReplies,
        ]
        .map(|requirement| state.requirement_state(requirement))
        .to_vec()
    };
    let completed = requirements
        .iter()
        .filter(|requirement| **requirement == VoiceRequirementState::Ready)
        .count();
    let failed = requirements.contains(&VoiceRequirementState::Failed);
    let color = if ready {
        theme.accent_green
    } else if failed {
        theme.destructive
    } else {
        theme.accent_blue
    };
    let (title, hint) = if ready {
        (
            if doubao {
                "Ready to connect"
            } else {
                "Ready to talk"
            }
            .to_string(),
            format!(
                "Press {voice_shortcut_label} to {arg0}.",
                arg0 = if doubao {
                    "start a conversation"
                } else {
                    "give Norma a voice command"
                }
            ),
        )
    } else if let VoiceEntryState::SettingUp { percent } = state.readiness() {
        (
            format!("Preparing voice · {percent}%"),
            "Downloads continue in the background. You can keep working.".to_string(),
        )
    } else {
        (
            if failed {
                "Voice needs attention"
            } else {
                "Finish setting up voice"
            }
            .to_string(),
            format!("{completed} of {arg0} requirements ready. Configure the sections below to get started.", arg0 = requirements.len()),
        )
    };
    div()
        .id("voice-status-overview")
        .debug_selector(|| "voice-status-overview".to_string())
        .w_full()
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(12.0))
        .p(px(16.0))
        .rounded(px(12.0))
        .bg(rgba(with_alpha(color, 0x12)))
        .child(
            div()
                .w(px(28.0))
                .h(px(28.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(14.0))
                .bg(rgba(with_alpha(color, 0x25)))
                .text_color(rgba(color))
                .child(if ready {
                    "✓"
                } else if failed {
                    "!"
                } else {
                    "i"
                }),
        )
        .child(
            div()
                .min_w(px(180.0))
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_secondary))
                        .child(hint),
                ),
        )
        .children(next_setup_action(theme, state, cx))
}

fn next_setup_action(
    theme: Theme,
    state: &VoiceSettingsState,
    cx: &Context<MultisoulApp>,
) -> Option<Stateful<Div>> {
    if state.values().desktop_mode() == DesktopVoiceMode::Doubao {
        return None;
    }
    let first_incomplete = match state.readiness() {
        VoiceEntryState::SetUp { first_incomplete }
        | VoiceEntryState::Unavailable { first_incomplete } => first_incomplete,
        VoiceEntryState::Ready | VoiceEntryState::SettingUp { .. } => return None,
    };
    Some(match first_incomplete {
        VoiceRequirement::Microphone => component_action(
            theme,
            "voice-status-next",
            "Grant Access",
            cx,
            cx.listener(MultisoulApp::on_voice_setup_request_microphone),
        ),
        VoiceRequirement::SpeechRecognition => component_action(
            theme,
            "voice-status-next",
            "Configure Recognition",
            cx,
            cx.listener(MultisoulApp::on_voice_manage_asr_models),
        ),
        VoiceRequirement::AgentProvider => component_action(
            theme,
            "voice-status-next",
            "Connect AI",
            cx,
            cx.listener(MultisoulApp::on_voice_manage_provider),
        ),
        VoiceRequirement::SpokenReplies => component_action(
            theme,
            "voice-status-next",
            "Choose Voice",
            cx,
            cx.listener(MultisoulApp::on_voice_manage_tts_models),
        ),
    })
}

fn ready_summary(theme: Theme, cx: &mut Context<MultisoulApp>) -> Stateful<Div> {
    let change_asr = component_action(
        theme,
        "voice-ready-change-asr",
        "Configure",
        cx,
        cx.listener(MultisoulApp::on_voice_manage_asr_models),
    );
    let change_tts = component_action(
        theme,
        "voice-ready-change-tts",
        "Change Voice",
        cx,
        cx.listener(MultisoulApp::on_voice_manage_tts_models),
    );
    let change_provider = component_action(
        theme,
        "voice-ready-change-provider",
        "Configure",
        cx,
        cx.listener(MultisoulApp::on_voice_manage_provider),
    );
    div()
        .id("voice-ready-summary")
        .debug_selector(|| "voice-ready-summary".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .p(px(20.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
        .bg(rgba(theme.panel_high))
        .child(summary_row(
            theme,
            "voice-ready-summary-recognition",
            "Speech Recognition",
            "●  Ready",
            theme.accent_green,
            Some(change_asr),
        ))
        .child(summary_row(
            theme,
            "voice-ready-summary-provider",
            "AI Connection",
            "●  Connected",
            theme.accent_green,
            Some(change_provider),
        ))
        .child(summary_row(
            theme,
            "voice-ready-summary-replies",
            "Spoken Replies",
            "●  Ready",
            theme.accent_green,
            Some(change_tts),
        ))
        .child(
            component_action(
                theme,
                "voice-review-setup",
                "Review Setup",
                cx,
                cx.listener(MultisoulApp::on_voice_review_setup),
            )
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .mt(px(10.0)),
        )
}

fn summary_row(
    theme: Theme,
    selector: &'static str,
    label: &'static str,
    status: &'static str,
    status_color: u32,
    action: Option<Stateful<Div>>,
) -> Stateful<Div> {
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.0))
        .px(px(4.0))
        .py(px(10.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_primary))
                .child(label),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(status_color))
                        .child(status),
                )
                .when_some(action, |row, action| row.child(action)),
        )
}
