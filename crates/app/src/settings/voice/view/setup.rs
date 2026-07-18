use super::provider::agent_provider_setup_group;
use super::tts_catalog::tts_catalog_row;
use super::{action_button, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::voice::{VoiceRequirement, VoiceRequirementState, VoiceSettingsState};
use gpui::{
    div, point, prelude::*, px, relative, rgba, ClickEvent, Context, Div, FontWeight, ScrollHandle,
    Stateful, Window,
};
use ui::Theme;

#[path = "setup/components.rs"]
mod components;
#[path = "setup/models.rs"]
mod models;

use self::components::compact_setup_row;
use self::models::{alternative_models_detail, recommended_package_detail};

pub(super) fn voice_setup(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    content_scroll: &ScrollHandle,
    focus_spoken_replies: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let microphone = state.requirement_state(VoiceRequirement::Microphone);
    let asr = state.requirement_state(VoiceRequirement::SpeechRecognition);
    let provider = state.requirement_state(VoiceRequirement::AgentProvider);
    let tts = state.requirement_state(VoiceRequirement::SpokenReplies);
    let recognition = combined_requirement_state(microphone, asr);
    let completed = [microphone, asr, provider, tts]
        .into_iter()
        .filter(|requirement| *requirement == VoiceRequirementState::Ready)
        .count();

    let page = div()
        .id("voice-setup-page")
        .debug_selector(|| "voice-setup-page".to_string())
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(18.0))
        .child(setup_title_bar(theme))
        .child(setup_progress(theme, completed))
        .child(
            div()
                .id("voice-setup-stepper")
                .debug_selector(|| "voice-setup-stepper".to_string())
                .relative()
                .w_full()
                .max_w(px(760.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .absolute()
                        .left(px(21.0))
                        .top(px(42.0))
                        .bottom(px(62.0))
                        .w(px(1.0))
                        .bg(rgba(with_alpha(theme.text_tertiary, 0x58))),
                )
                .child(setup_step(
                    theme,
                    1,
                    "voice-setup-requirement-asr",
                    "Speech Recognition",
                    "Let Norma hear you with a private bilingual model.",
                    recognition,
                    recognition_detail(theme, microphone, state, cx),
                ))
                .child(setup_step(
                    theme,
                    2,
                    "voice-setup-requirement-provider",
                    "AI Connection",
                    "Connect the OpenAI-compatible model that understands your request.",
                    provider,
                    agent_provider_setup_group(theme, state, inputs, cx),
                ))
                .child(setup_step(
                    theme,
                    3,
                    "voice-setup-requirement-tts",
                    "Spoken Replies",
                    "Install the required local voice so Norma can reply out loud.",
                    tts,
                    spoken_replies_detail(theme, state, content_scroll, focus_spoken_replies, cx),
                )),
        )
        .child(finish_setup_button(theme, state.reviewing_setup(), cx));
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

fn setup_title_bar(theme: Theme) -> Div {
    div()
        .w_full()
        .h(px(42.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id("voice-setup-title")
                .debug_selector(|| "voice-setup-title".to_string())
                .text_size(px(22.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child("Voice Setup"),
        )
}

fn setup_progress(theme: Theme, completed: usize) -> Stateful<Div> {
    div()
        .id("voice-setup-progress")
        .debug_selector(|| "voice-setup-progress".to_string())
        .w(px(310.0))
        .flex()
        .flex_col()
        .gap(px(7.0))
        .px(px(14.0))
        .py(px(9.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x20)))
        .bg(rgba(with_alpha(theme.text_primary, 0x08)))
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgba(theme.text_secondary))
                .child(format!("{completed} of 4 requirements complete")),
        )
        .child(
            div()
                .w_full()
                .h(px(4.0))
                .rounded(px(2.0))
                .bg(rgba(with_alpha(theme.node_stroke, 0x48)))
                .child(
                    div()
                        .h_full()
                        .w(relative(completed as f32 / 4.0))
                        .rounded(px(2.0))
                        .bg(rgba(theme.accent_blue)),
                ),
        )
}

fn setup_step(
    theme: Theme,
    index: usize,
    card_selector: &'static str,
    title: &'static str,
    hint: &'static str,
    requirement: VoiceRequirementState,
    content: Div,
) -> Stateful<Div> {
    let step_selector = format!("voice-setup-step-{index}");
    let marker_color = if requirement == VoiceRequirementState::Ready {
        theme.accent_green
    } else {
        theme.accent_blue
    };
    div()
        .id(("voice-setup-step", index))
        .debug_selector(move || step_selector.clone())
        .relative()
        .w_full()
        .pl(px(60.0))
        .pb(px(18.0))
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(10.0))
                .w(px(42.0))
                .h(px(42.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(21.0))
                .border_1()
                .border_color(rgba(with_alpha(marker_color, 0xb8)))
                .bg(rgba(with_alpha(theme.panel, 0xff)))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(marker_color))
                .child(index.to_string()),
        )
        .child(
            div()
                .id(card_selector)
                .debug_selector(move || card_selector.to_string())
                .w_full()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .p(px(18.0))
                .rounded(px(14.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.text_primary, 0x24)))
                .bg(rgba(with_alpha(theme.text_primary, 0x08)))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_start()
                        .justify_between()
                        .gap(px(14.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .text_size(px(16.0))
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
                        .child(status_badge(theme, requirement)),
                )
                .child(content),
        )
}

fn recognition_detail(
    theme: Theme,
    microphone: VoiceRequirementState,
    state: &VoiceSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(microphone_detail(theme, microphone, cx))
        .child(recommended_package_detail(theme, state, cx))
        .child(alternative_models_detail(theme, state, cx))
}

fn microphone_detail(
    theme: Theme,
    state: VoiceRequirementState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    compact_setup_row(
        theme,
        "Microphone permission",
        "Required for local recognition",
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

fn spoken_replies_detail(
    theme: Theme,
    state: &VoiceSettingsState,
    content_scroll: &ScrollHandle,
    focus_spoken_replies: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let mut models = div().w_full().child(tts_catalog_row(theme, state, cx));
    if focus_spoken_replies {
        let scroll = content_scroll.clone();
        models = models.on_children_prepainted(move |children, window, _cx| {
            let Some(target) = children.first() else {
                return;
            };
            let viewport = scroll.bounds();
            let current = scroll.offset();
            let unscrolled_target_top = target.top() - current.y;
            let desired_y = (viewport.top() + px(16.0) - unscrolled_target_top)
                .clamp(-scroll.max_offset().height, px(0.0));
            if f32::from(current.y - desired_y).abs() <= 0.5 {
                return;
            }
            scroll.set_offset(point(current.x, desired_y));
            window.request_animation_frame();
        });
    }
    div().w_full().child(
        models
            .id("voice-setup-spoken-replies-models")
            .debug_selector(|| "voice-setup-spoken-replies-models".to_string()),
    )
}

fn finish_setup_button(
    theme: Theme,
    reviewing: bool,
    cx: &mut Context<MultisoulApp>,
) -> Stateful<Div> {
    let selector = if reviewing {
        "voice-review-setup-done"
    } else {
        "voice-setup-finish"
    };
    div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .w_full()
        .max_w(px(700.0))
        .flex()
        .items_center()
        .justify_center()
        .px(px(16.0))
        .py(px(12.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(with_alpha(
            if reviewing {
                theme.accent_blue
            } else {
                theme.text_primary
            },
            0x72,
        )))
        .bg(rgba(with_alpha(
            if reviewing {
                theme.accent_blue
            } else {
                theme.text_primary
            },
            if reviewing { 0x24 } else { 0x0c },
        )))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(if reviewing {
            theme.accent_blue
        } else {
            theme.text_tertiary
        }))
        .when(reviewing, |button| {
            button
                .cursor_pointer()
                .on_click(cx.listener(MultisoulApp::on_voice_finish_review_setup))
        })
        .child(if reviewing {
            "Done Reviewing"
        } else {
            "Finish Setup"
        })
}

fn status_badge(theme: Theme, state: VoiceRequirementState) -> Div {
    let (label, color) = match state {
        VoiceRequirementState::Ready => ("Ready", theme.accent_green),
        VoiceRequirementState::Installing { .. } => ("In progress", theme.accent_blue),
        VoiceRequirementState::Failed => ("Needs attention", theme.destructive),
        VoiceRequirementState::Missing => ("Required", theme.accent_orange),
    };
    div()
        .flex_none()
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

fn combined_requirement_state(
    first: VoiceRequirementState,
    second: VoiceRequirementState,
) -> VoiceRequirementState {
    if first == VoiceRequirementState::Failed || second == VoiceRequirementState::Failed {
        VoiceRequirementState::Failed
    } else if let VoiceRequirementState::Installing { .. } = first {
        first
    } else if let VoiceRequirementState::Installing { .. } = second {
        second
    } else if first == VoiceRequirementState::Ready && second == VoiceRequirementState::Ready {
        VoiceRequirementState::Ready
    } else {
        VoiceRequirementState::Missing
    }
}
