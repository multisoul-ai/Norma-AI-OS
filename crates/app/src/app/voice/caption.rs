//! Live Voice transcript caption above the Command Dock.

use crate::app::MultisoulApp;
use dialogue::DialoguePhase;
use gpui::{
    div, point, prelude::*, px, rgba, BoxShadow, Context, Div, FontWeight, KeyDownEvent,
    MouseButton, Stateful, Window,
};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::component::{Icon, IconName, Sizable, Size};
use ui::icons::with_alpha;
use ui::Theme;

const COLLAPSED_PREVIEW_CHARS: usize = 72;
const CAPTION_WAVE_CYCLES_PER_SECOND: f32 = 1.2;
const EXPANDED_CAPTION_BODY_MAX_HEIGHT: f32 = 104.0;
const EXPANDED_CAPTION_MIN_HEIGHT: f32 = 72.0;
pub(crate) const CAPTION_SURFACE_ALPHA: u8 = 0xb8;
pub(crate) const CAPTION_SURFACE_OPACITY_MILLI: u16 =
    ((CAPTION_SURFACE_ALPHA as u32 * 1_000 + 127) / 255) as u16;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum VoiceWaveformState {
    Listening,
    Thinking,
    Replying,
}

impl MultisoulApp {
    pub(crate) fn voice_caption_text(&self) -> Option<String> {
        self.voice
            .caption_text()
            .map(ToOwned::to_owned)
            .or_else(|| {
                matches!(
                    self.dialogue.phase(),
                    DialoguePhase::Thinking | DialoguePhase::Speaking
                )
                .then(|| self.dialogue.pending_transcript())
                .flatten()
                .filter(|text| !text.trim().is_empty())
                .map(ToOwned::to_owned)
            })
    }

    pub(crate) fn voice_caption_is_visible(&self) -> bool {
        self.voice.caption_text().is_some()
            || (matches!(
                self.dialogue.phase(),
                DialoguePhase::Thinking | DialoguePhase::Speaking
            ) && self
                .dialogue
                .pending_transcript()
                .is_some_and(|text| !text.trim().is_empty()))
    }

    pub(crate) fn voice_caption_animation_active(&self) -> bool {
        self.voice_waveform_state().is_some()
            && !self.motion.reduce_motion
            && matches!(self.screen, crate::app::AppScreen::Canvas)
            && self.window_active
            && !self.settings_panel.open
    }

    pub(crate) fn voice_waveform_state(&self) -> Option<VoiceWaveformState> {
        if self.voice_settings.values().desktop_mode()
            == crate::settings::voice::DesktopVoiceMode::Doubao
        {
            return match self.doubao.status {
                crate::app::doubao::DoubaoStatus::Connected => Some(VoiceWaveformState::Listening),
                crate::app::doubao::DoubaoStatus::Thinking => Some(VoiceWaveformState::Thinking),
                crate::app::doubao::DoubaoStatus::Speaking => Some(VoiceWaveformState::Replying),
                _ => None,
            };
        }
        if self.effective_voice_visual() == super::control::VoiceVisual::Error {
            return None;
        }
        match self.dialogue.phase() {
            DialoguePhase::Speaking => Some(VoiceWaveformState::Replying),
            DialoguePhase::Thinking => Some(VoiceWaveformState::Thinking),
            DialoguePhase::Listening => Some(VoiceWaveformState::Listening),
            DialoguePhase::Idle => match self.voice.visual() {
                super::control::VoiceVisual::Recording => Some(VoiceWaveformState::Listening),
                super::control::VoiceVisual::Processing => Some(VoiceWaveformState::Thinking),
                _ => None,
            },
        }
    }

    pub(in crate::app) fn advance_voice_caption_waveform(
        &mut self,
        delta: std::time::Duration,
    ) -> bool {
        if self.voice_waveform_state().is_none() || self.motion.reduce_motion {
            let changed = self.voice_caption_waveform_phase != 0.0;
            self.voice_caption_waveform_phase = 0.0;
            return changed;
        }

        let next = (self.voice_caption_waveform_phase
            + delta.as_secs_f32() * CAPTION_WAVE_CYCLES_PER_SECOND)
            .fract();
        let changed = (next - self.voice_caption_waveform_phase).abs() > f32::EPSILON;
        self.voice_caption_waveform_phase = next;
        changed
    }

    pub(crate) fn voice_caption_waveform_phase_milli(&self) -> u16 {
        (self.voice_caption_waveform_phase * 1_000.0).round() as u16
    }

    pub(crate) fn voice_caption_preview(&self) -> Option<String> {
        self.voice_caption_text()
            .map(|text| trailing_chars(&text, COLLAPSED_PREVIEW_CHARS))
    }

    pub(crate) fn render_voice_caption(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let full_text = self.voice_caption_text()?;
        let expanded = self.voice.caption_expanded();
        let visible_text = if expanded {
            full_text
        } else {
            trailing_chars(&full_text, COLLAPSED_PREVIEW_CHARS)
        };
        let theme = self.theme;
        let body = div()
            .id("voice-caption-text")
            .debug_selector(|| "voice-caption-text".to_string())
            .min_w(px(0.0))
            .flex_1()
            .line_height(px(16.0))
            .text_size(px(11.0))
            .text_color(rgba(0xfffffff0))
            .when(expanded, |body| {
                body.max_h(px(EXPANDED_CAPTION_BODY_MAX_HEIGHT))
                    .overflow_y_scroll()
                    .pr(px(8.0))
                    .whitespace_normal()
                    .child(visible_text.clone())
            })
            .when(!expanded, |body| {
                body.h(px(18.0)).overflow_hidden().child(
                    div()
                        .id("voice-caption-text-content")
                        .debug_selector(|| "voice-caption-text-content".to_string())
                        .w_full()
                        .min_w(px(0.0))
                        .whitespace_nowrap()
                        .truncate()
                        .child(visible_text.clone()),
                )
            });

        Some(
            div()
                .absolute()
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(88.0))
                .px(px(24.0))
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("workspace-voice-caption")
                        .debug_selector(|| "workspace-voice-caption".to_string())
                        .w_full()
                        .max_w(px(560.0))
                        .min_h(if expanded {
                            px(EXPANDED_CAPTION_MIN_HEIGHT)
                        } else {
                            px(48.0)
                        })
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .py(px(7.0))
                        .rounded(px(14.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.dock_line, 0x58)))
                        .shadow(caption_shadow())
                        .bg(rgba(with_alpha(theme.dock, CAPTION_SURFACE_ALPHA)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(voice_waveform(
                            theme,
                            self.voice_caption_waveform_phase,
                            self.voice_waveform_state(),
                            self.motion.reduce_motion,
                            "voice-caption-waveform",
                        ))
                        .child(caption_divider(theme))
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_size(px(10.0))
                                .text_color(rgba(0xffffffc7))
                                .child(self.effective_voice_dock_label()),
                        )
                        .child(caption_divider(theme))
                        .child(body)
                        .child(caption_expand_control(expanded, theme, cx)),
                ),
        )
    }

    fn on_voice_caption_toggle(
        &mut self,
        _: &gpui::ClickEvent,
        _: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.voice.toggle_caption_expanded();
        cx.notify();
    }

    fn on_voice_caption_key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(event.keystroke.key.as_str(), "enter" | "space")
            && !event.keystroke.modifiers.modified()
        {
            cx.stop_propagation();
            self.voice.toggle_caption_expanded();
            cx.notify();
        }
    }
}

fn trailing_chars(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    text.chars().skip(count.saturating_sub(max_chars)).collect()
}

pub(crate) fn voice_waveform(
    theme: Theme,
    phase: f32,
    state: Option<VoiceWaveformState>,
    reduce_motion: bool,
    selector: &'static str,
) -> Stateful<Div> {
    let color = match state {
        Some(VoiceWaveformState::Listening) => theme.accent_green,
        Some(VoiceWaveformState::Thinking) => theme.accent_violet,
        Some(VoiceWaveformState::Replying) => theme.accent_blue,
        None => theme.text_secondary,
    };
    let peaks = [0.60, 0.95, 0.72, 1.0, 0.55];
    let mut waveform = div()
        .id(selector)
        .debug_selector(move || selector.to_string())
        .flex_none()
        .w(px(27.0))
        .h(px(20.0))
        .flex()
        .items_center()
        .gap(px(3.0));
    for (index, peak) in peaks.into_iter().enumerate() {
        let height = if reduce_motion || state.is_none() {
            4.0 + peak * 10.0
        } else {
            let pulse = ((phase * std::f32::consts::TAU + index as f32 * 1.6).sin() + 1.0) * 0.5;
            let movement = if state == Some(VoiceWaveformState::Thinking) {
                0.25 + pulse * 0.25
            } else {
                0.2 + pulse * 0.8
            };
            4.0 + peak * 14.0 * movement
        };
        waveform = waveform.child(
            div()
                .w(px(3.0))
                .h(px(height))
                .rounded(px(1.5))
                .bg(rgba(with_alpha(color, 0xe0))),
        );
    }
    waveform
}

fn caption_divider(theme: Theme) -> Div {
    div()
        .flex_none()
        .w(px(1.0))
        .h(px(22.0))
        .bg(rgba(with_alpha(theme.dock_line, 0x50)))
}

fn caption_expand_control(expanded: bool, theme: Theme, cx: &mut Context<MultisoulApp>) -> Button {
    let label = if expanded { "收起" } else { "全文" };
    let chevron = if expanded {
        IconName::ChevronDown
    } else {
        IconName::ChevronUp
    };
    let foreground = rgba(0xffffffb8);
    let variant = ButtonCustomVariant::new(cx)
        .foreground(foreground.into())
        .border(rgba(with_alpha(theme.dock_line, 0x2c)).into())
        .hover(rgba(0xffffff18).into())
        .active(rgba(0xffffff24).into());

    Button::new("voice-caption-expand")
        .debug_selector(|| "voice-caption-expand".to_string())
        .tooltip(if expanded {
            "Collapse full voice transcript"
        } else {
            "Show the full voice transcript"
        })
        .custom(variant)
        .with_size(Size::XSmall)
        .compact()
        .min_w(px(58.0))
        .h(px(26.0))
        .rounded(px(13.0))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .font_family(".SystemUIFont")
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(11.0))
                .line_height(px(14.0))
                .text_color(foreground)
                .child(
                    div()
                        .id("voice-caption-expand-label")
                        .debug_selector(|| "voice-caption-expand-label".to_string())
                        .flex_none()
                        .child(label),
                )
                .child(
                    div()
                        .id("voice-caption-expand-chevron")
                        .debug_selector(|| "voice-caption-expand-chevron".to_string())
                        .flex_none()
                        .w(px(10.0))
                        .h(px(10.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            Icon::new(chevron)
                                .with_size(px(10.0))
                                .text_color(foreground),
                        ),
                ),
        )
        .on_key_down(cx.listener(MultisoulApp::on_voice_caption_key_down))
        .on_click(cx.listener(MultisoulApp::on_voice_caption_toggle))
}

fn caption_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(0x0409122b).into(),
        offset: point(px(0.0), px(10.0)),
        blur_radius: px(30.0),
        spread_radius: px(-10.0),
    }]
}
