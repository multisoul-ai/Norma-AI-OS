use super::provider::agent_provider_setup_group;
use super::{action_button, setting_row, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, Window};
use ui::Theme;

pub(super) fn voice_setup(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let requirements = [
        VoiceRequirement::Microphone,
        VoiceRequirement::SpeechRecognition,
        VoiceRequirement::AgentProvider,
        VoiceRequirement::SpokenReplies,
    ];
    let completed = requirements
        .iter()
        .filter(|requirement| {
            state.requirement_state(**requirement) == VoiceRequirementState::Ready
        })
        .count();
    let expanded_requirement = state.setup_expanded_requirement();
    let mut page = div()
        .id("voice-setup-page")
        .debug_selector(|| "voice-setup-page".to_string())
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(setup_header(theme, completed, state.reviewing_setup(), cx));

    for requirement in requirements {
        page = page.child(requirement_card(
            theme,
            requirement,
            state.requirement_state(requirement),
            expanded_requirement == Some(requirement),
            state,
            inputs,
            cx,
        ));
    }
    div().w_full().child(page)
}

impl MultisoulApp {
    pub(crate) fn on_voice_setup_install_recommended(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let asr_id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
            .expect("built-in recommended ASR id must parse");
        let tts_id = tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID)
            .expect("built-in recommended TTS id must parse");
        let voice_id =
            tts::TtsVoiceId::parse("default").expect("built-in recommended voice id must parse");
        self.voice_settings.select_asr_model(Some(asr_id.clone()));
        self.voice_settings
            .select_tts_model_and_voice(Some(tts_id.clone()), Some(voice_id));
        self.persist_voice_settings_in_background(cx);
        self.install_asr_model(asr_id, cx);
        self.install_tts_model(tts_id, cx);
    }

    pub(crate) fn on_voice_setup_cancel_recommended(
        &mut self,
        _: &ClickEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let asr_id = asr::AsrModelId::parse(asr::ZIPFORMER_BILINGUAL_MODEL_ID)
            .expect("built-in recommended ASR id must parse");
        let tts_id = tts::TtsModelId::parse(tts::MATCHA_ZH_EN_MODEL_ID)
            .expect("built-in recommended TTS id must parse");
        self.cancel_asr_install(asr_id, cx);
        self.cancel_tts_install(tts_id, cx);
    }
}

fn setup_header(
    theme: Theme,
    completed: usize,
    reviewing: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .pb(px(8.0))
        .child(
            div()
                .text_size(px(22.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child("Set up Voice"),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_secondary))
                .child(
                    "Complete these four requirements once, then use ⌘E anywhere on the canvas.",
                ),
        )
        .child(
            div()
                .w_full()
                .h(px(6.0))
                .rounded(px(3.0))
                .bg(rgba(with_alpha(theme.node_stroke, 0x38)))
                .child(
                    div()
                        .h_full()
                        .w(gpui::relative(completed as f32 / 4.0))
                        .rounded(px(3.0))
                        .bg(rgba(theme.accent_blue)),
                ),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgba(theme.text_secondary))
                .child(format!("{completed} of 4 required steps complete")),
        )
        .when(reviewing, |header| {
            header.child(
                action_button(theme, "Done Reviewing", theme.accent_cyan)
                    .id("voice-review-setup-done")
                    .debug_selector(|| "voice-review-setup-done".to_string())
                    .on_click(cx.listener(MultisoulApp::on_voice_finish_review_setup)),
            )
        })
}

fn requirement_card(
    theme: Theme,
    requirement: VoiceRequirement,
    state_value: VoiceRequirementState,
    expanded: bool,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let ready = state_value == VoiceRequirementState::Ready;
    let (title, hint) = requirement_copy(requirement);
    let mut card = div()
        .id(("voice-setup-requirement", requirement_index(requirement)))
        .debug_selector(move || requirement_selector(requirement).to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(if expanded {
            with_alpha(theme.accent_blue, 0x88)
        } else {
            with_alpha(theme.node_stroke, 0xb0)
        }))
        .bg(rgba(with_alpha(theme.panel_high, 0x8c)))
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .text_size(px(14.0))
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
                .child(status_badge(theme, state_value)),
        );

    if !expanded && !ready {
        card = card
            .cursor_pointer()
            .on_click(cx.listener(move |app, _, _, cx| {
                app.voice_settings.expand_setup_requirement(requirement);
                cx.notify();
            }));
    }

    if expanded || ready {
        card = match requirement {
            VoiceRequirement::Microphone => card.child(microphone_detail(theme, state_value, cx)),
            VoiceRequirement::SpeechRecognition => {
                card.child(recommended_package_detail(theme, state, cx))
            }
            VoiceRequirement::AgentProvider => {
                card.child(agent_provider_setup_group(theme, state, inputs, cx))
            }
            VoiceRequirement::SpokenReplies => card.child(
                setting_row(
                    theme,
                    "Local spoken replies",
                    "Matcha Chinese-English and Default Bilingual Voice are required.",
                )
                .child(div().text_size(px(12.0)).child(if ready {
                    "Default Bilingual Voice selected"
                } else {
                    "Install the recommended voice package above"
                })),
            ),
        };
    }
    div().w_full().child(card)
}

fn microphone_detail(
    theme: Theme,
    state: VoiceRequirementState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    setting_row(
        theme,
        "Microphone permission",
        "Audio stays on this Mac for local recognition.",
    )
    .child(
        action_button(
            theme,
            if state == VoiceRequirementState::Ready {
                "Authorized"
            } else if state == VoiceRequirementState::Failed {
                "Open Microphone Settings"
            } else {
                "Grant Access"
            },
            if state == VoiceRequirementState::Ready {
                theme.accent_green
            } else {
                theme.accent_blue
            },
        )
        .id("voice-setup-microphone-action")
        .debug_selector(|| "voice-setup-microphone-action".to_string())
        .when(state != VoiceRequirementState::Ready, |button| {
            button.on_click(cx.listener(MultisoulApp::on_voice_setup_request_microphone))
        }),
    )
}

fn recommended_package_detail(
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
    setting_row(
        theme,
        "Recommended Voice Package · about 644 MB",
        "Zipformer Bilingual + Matcha Chinese-English + Default Bilingual Voice",
    )
    .child(
        action_button(
            theme,
            if installing {
                "Cancel Download"
            } else {
                "Install Recommended"
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
    )
}

fn status_badge(theme: Theme, state: VoiceRequirementState) -> Div {
    let (label, color) = match state {
        VoiceRequirementState::Ready => ("Ready", theme.accent_green),
        VoiceRequirementState::Installing { .. } => ("In progress", theme.accent_blue),
        VoiceRequirementState::Failed => ("Needs attention", theme.destructive),
        VoiceRequirementState::Missing => ("Required", theme.accent_orange),
    };
    div()
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(with_alpha(color, 0x78)))
        .bg(rgba(with_alpha(color, 0x18)))
        .text_size(px(11.0))
        .text_color(rgba(color))
        .child(label)
}

fn requirement_copy(requirement: VoiceRequirement) -> (&'static str, &'static str) {
    match requirement {
        VoiceRequirement::Microphone => ("1  Microphone", "Allow Norma to hear a voice command."),
        VoiceRequirement::SpeechRecognition => (
            "2  Speech Recognition",
            "Install the recommended bilingual local model.",
        ),
        VoiceRequirement::AgentProvider => (
            "3  AI Connection",
            "Connect an OpenAI-compatible provider and select a model.",
        ),
        VoiceRequirement::SpokenReplies => (
            "4  Spoken Replies",
            "Install the required local TTS model and voice.",
        ),
    }
}

fn requirement_index(requirement: VoiceRequirement) -> usize {
    match requirement {
        VoiceRequirement::Microphone => 0,
        VoiceRequirement::SpeechRecognition => 1,
        VoiceRequirement::AgentProvider => 2,
        VoiceRequirement::SpokenReplies => 3,
    }
}

fn requirement_selector(requirement: VoiceRequirement) -> &'static str {
    match requirement {
        VoiceRequirement::Microphone => "voice-setup-requirement-microphone",
        VoiceRequirement::SpeechRecognition => "voice-setup-requirement-asr",
        VoiceRequirement::AgentProvider => "voice-setup-requirement-provider",
        VoiceRequirement::SpokenReplies => "voice-setup-requirement-tts",
    }
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
