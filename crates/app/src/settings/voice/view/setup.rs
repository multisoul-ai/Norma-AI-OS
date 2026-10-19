use super::components::VoiceAction;
use super::provider::agent_provider_setup_group;
use super::tts_catalog::tts_catalog_row;
use super::tts_provider::tts_provider_settings;
use super::{with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::typography::{control_label, SettingsText, TextRole};
use crate::settings::voice::{
    VoiceRequirement, VoiceRequirementState, VoiceSettingsState, VoiceSetupDestination,
};
use gpui::{div, point, prelude::*, px, rgba, Context, Div, ScrollHandle, Stateful, Window};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

#[cfg(test)]
#[path = "../../../../tests/unit/voice_settings_redesign_tests.rs"]
mod redesign_tests;

#[path = "setup/components.rs"]
mod components;
#[path = "setup/models.rs"]
pub(super) mod models;
#[path = "setup/recognition.rs"]
mod recognition;

use self::recognition::recognition_detail;

pub(super) fn voice_setup(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    content_scroll: &ScrollHandle,
    setup_scroll_target: Option<VoiceSetupDestination>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let microphone = state.requirement_state(VoiceRequirement::Microphone);
    let asr = state.requirement_state(VoiceRequirement::SpeechRecognition);
    let provider = state.requirement_state(VoiceRequirement::AgentProvider);
    let tts = state.requirement_state(VoiceRequirement::SpokenReplies);
    let recognition = combined_requirement_state(microphone, asr);
    let page = div()
        .id("voice-setup-page")
        .debug_selector(|| "voice-setup-page".to_string())
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(setup_step(
            theme,
            1,
            "voice-setup-requirement-asr",
            ui::i18n::text("voice.speech_recognition"),
            ui::i18n::text("voice.let_norma_hear_you_using_your_preferred_recognition_source"),
            recognition,
            recognition_detail(theme, microphone, state, inputs, cx),
        ))
        .child(setup_step(
            theme,
            2,
            "voice-setup-requirement-provider",
            ui::i18n::text("voice.ai_connection"),
            ui::i18n::text(
                "voice.connect_the_openai_compatible_model_that_understands_your_request",
            ),
            provider,
            setup_detail_with_scroll_focus(
                agent_provider_setup_group(theme, state, inputs, cx),
                content_scroll,
                setup_scroll_target == Some(VoiceSetupDestination::AgentProvider),
            ),
        ))
        .child(setup_step(
            theme,
            3,
            "voice-setup-requirement-tts",
            ui::i18n::text("voice.spoken_replies"),
            ui::i18n::text("voice.choose_an_on_device_or_cloud_voice_so_norma_can_reply"),
            tts,
            spoken_replies_detail(
                theme,
                state,
                inputs,
                content_scroll,
                setup_scroll_target == Some(VoiceSetupDestination::SpokenReplies),
                window,
                cx,
            ),
        ))
        .when(state.reviewing_setup(), |page| {
            page.child(finish_setup_button(theme, cx))
        });
    div().w_full().min_w(px(0.0)).flex().flex_col().child(page)
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
    div()
        .id(("voice-setup-step", index))
        .debug_selector(move || step_selector.clone())
        .w_full()
        .child(
            div()
                .id(card_selector)
                .debug_selector(move || card_selector.to_string())
                .w_full()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .p(px(20.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
                .bg(rgba(theme.panel_high))
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
                                .child(div().settings_text(TextRole::Headline, theme).child(title))
                                .child(div().settings_text(TextRole::Helper, theme).child(hint)),
                        )
                        .child(status_badge(theme, requirement)),
                )
                .child(content),
        )
}

fn spoken_replies_detail(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    content_scroll: &ScrollHandle,
    focus_spoken_replies: bool,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div().w_full().child(
        setup_detail_with_scroll_focus(
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .child(tts_provider_settings(theme, state, inputs, cx))
                .child(super::spoken_reply::reply_preferences(theme, state, cx))
                .when(state.tts_picker_models_expanded(), |content| {
                    content.child(tts_catalog_row(theme, state, window, cx))
                }),
            content_scroll,
            focus_spoken_replies,
        )
        .id("voice-setup-spoken-replies-models")
        .debug_selector(|| "voice-setup-spoken-replies-models".to_string()),
    )
}

fn setup_detail_with_scroll_focus(content: Div, content_scroll: &ScrollHandle, focus: bool) -> Div {
    let mut detail = div().w_full().child(content);
    if focus {
        let scroll = content_scroll.clone();
        detail = detail.on_children_prepainted(move |children, window, _cx| {
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
    detail
}

fn finish_setup_button(theme: Theme, cx: &mut Context<MultisoulApp>) -> Div {
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(with_alpha(theme.accent_blue, 0x14)).into())
        .foreground(rgba(theme.accent_blue).into())
        .border(rgba(with_alpha(theme.accent_blue, 0x50)).into())
        .hover(rgba(with_alpha(theme.accent_blue, 0x25)).into())
        .active(rgba(with_alpha(theme.accent_blue, 0x38)).into());
    div().w_full().flex().justify_end().child(
        Button::new("voice-review-setup-done")
            .debug_selector(|| "voice-review-setup-done".to_string())
            .custom(variant)
            .child(control_label(ui::i18n::text("voice.done_reviewing")))
            .on_activate(cx.listener(MultisoulApp::on_voice_finish_review_setup)),
    )
}

fn status_badge(theme: Theme, state: VoiceRequirementState) -> Div {
    let (label, color) = match state {
        VoiceRequirementState::Ready => (ui::i18n::text("common.ready"), theme.accent_green),
        VoiceRequirementState::Installing { .. } => {
            (ui::i18n::text("voice.in_progress"), theme.accent_blue)
        }
        VoiceRequirementState::Failed => {
            (ui::i18n::text("guide.needs_attention"), theme.destructive)
        }
        VoiceRequirementState::Missing => (ui::i18n::text("voice.required"), theme.accent_orange),
    };
    div()
        .flex_none()
        .px(px(8.0))
        .py(px(4.0))
        .rounded(px(7.0))
        .border_1()
        .border_color(rgba(with_alpha(color, 0x78)))
        .bg(rgba(with_alpha(color, 0x18)))
        .settings_font(TextRole::Label)
        .text_color(rgba(if state == VoiceRequirementState::Ready {
            theme.text_primary
        } else {
            color
        }))
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
