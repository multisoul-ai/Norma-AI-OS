use super::{components::component_action, with_alpha};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{
    DesktopVoiceMode, VoiceEntryState, VoiceRequirement, VoiceRequirementState, VoiceSettingsState,
};
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful};
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
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::format(
                "voice.press_voice_shortcut_label_to_talk_changes_below_are_saved_automatically",
                &[("voice_shortcut_label", (voice_shortcut_label).to_string())],
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
                ui::i18n::text("voice.ready_to_connect")
            } else {
                ui::i18n::text("voice.ready_to_talk")
            }
            .to_string(),
            if doubao {
                super::interaction::doubao_shortcut_hint(state, voice_shortcut_label)
            } else {
                ui::i18n::format(
                    "voice.press_voice_shortcut_label_to",
                    &[
                        ("voice_shortcut_label", (voice_shortcut_label).to_string()),
                        (
                            "arg0",
                            (if doubao {
                                ui::i18n::text("voice.start_a_conversation")
                            } else {
                                ui::i18n::text("voice.give_norma_a_voice_command")
                            })
                            .to_string(),
                        ),
                    ],
                )
            },
        )
    } else if let VoiceEntryState::SettingUp { percent } = state.readiness() {
        (
            ui::i18n::format(
                "voice.preparing_voice_percent",
                &[("percent", (percent).to_string())],
            ),
            ui::i18n::text("voice.downloads_continue_in_the_background_you_can_keep_working")
                .to_string(),
        )
    } else {
        (
            if failed {
                ui::i18n::text("voice.voice_needs_attention")
            } else {
                ui::i18n::text("voice.finish_setting_up_voice")
            }
            .to_string(),
            ui::i18n::format(
                "voice.completed_of_requirements_ready_configure_the_sections_below_to_get_started",
                &[
                    ("completed", (completed).to_string()),
                    ("arg0", (requirements.len()).to_string()),
                ],
            ),
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
                .child(div().settings_text(TextRole::RowTitle, theme).child(title))
                .child(div().settings_text(TextRole::Helper, theme).child(hint)),
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
            ui::i18n::text("voice.grant_access"),
            cx,
            cx.listener(MultisoulApp::on_voice_setup_request_microphone),
        ),
        VoiceRequirement::SpeechRecognition => component_action(
            theme,
            "voice-status-next",
            ui::i18n::text("voice.configure_recognition"),
            cx,
            cx.listener(MultisoulApp::on_voice_manage_asr_models),
        ),
        VoiceRequirement::AgentProvider => component_action(
            theme,
            "voice-status-next",
            ui::i18n::text("voice.connect_ai"),
            cx,
            cx.listener(MultisoulApp::on_voice_manage_provider),
        ),
        VoiceRequirement::SpokenReplies => component_action(
            theme,
            "voice-status-next",
            ui::i18n::text("voice.choose_voice"),
            cx,
            cx.listener(MultisoulApp::on_voice_manage_tts_models),
        ),
    })
}

fn ready_summary(theme: Theme, cx: &mut Context<MultisoulApp>) -> Stateful<Div> {
    let change_asr = component_action(
        theme,
        "voice-ready-change-asr",
        ui::i18n::text("mobile.configure"),
        cx,
        cx.listener(MultisoulApp::on_voice_manage_asr_models),
    );
    let change_tts = component_action(
        theme,
        "voice-ready-change-tts",
        ui::i18n::text("voice.change_voice"),
        cx,
        cx.listener(MultisoulApp::on_voice_manage_tts_models),
    );
    let change_provider = component_action(
        theme,
        "voice-ready-change-provider",
        ui::i18n::text("mobile.configure"),
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
            ui::i18n::text("voice.speech_recognition"),
            ui::i18n::text("voice.ready"),
            theme.accent_green,
            Some(change_asr),
        ))
        .child(summary_row(
            theme,
            "voice-ready-summary-provider",
            ui::i18n::text("voice.ai_connection"),
            ui::i18n::text("voice.connected"),
            theme.accent_green,
            Some(change_provider),
        ))
        .child(summary_row(
            theme,
            "voice-ready-summary-replies",
            ui::i18n::text("voice.spoken_replies"),
            ui::i18n::text("voice.ready"),
            theme.accent_green,
            Some(change_tts),
        ))
        .child(
            component_action(
                theme,
                "voice-review-setup",
                ui::i18n::text("voice.review_setup"),
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
        .child(div().settings_text(TextRole::Body, theme).child(label))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .settings_font(TextRole::Label)
                        .text_color(rgba(status_color))
                        .child(status),
                )
                .when_some(action, |row, action| row.child(action)),
        )
}
