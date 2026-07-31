//! Live Voice transcript caption above the Command Dock.

use crate::app::MultisoulApp;
use dialogue::DialoguePhase;
use gpui::{
    div, point, prelude::*, px, rgba, BoxShadow, Context, Div, KeyDownEvent, MouseButton, Window,
};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::icons::with_alpha;
use ui::Theme;

const COLLAPSED_PREVIEW_CHARS: usize = 72;
const CAPTION_WAVE_CYCLES_PER_SECOND: f32 = 1.2;
pub(crate) const CAPTION_SURFACE_ALPHA: u8 = 0xb8;
pub(crate) const CAPTION_SURFACE_OPACITY_MILLI: u16 =
    ((CAPTION_SURFACE_ALPHA as u32 * 1_000 + 127) / 255) as u16;

impl MultisoulApp {
    pub(crate) fn voice_caption_text(&self) -> Option<String> {
        self.voice
            .caption_text()
            .map(ToOwned::to_owned)
            .or_else(|| {
                matches!(self.dialogue.phase(), DialoguePhase::Thinking)
                    .then(|| self.dialogue.pending_transcript())
                    .flatten()
                    .filter(|text| !text.trim().is_empty())
                    .map(ToOwned::to_owned)
            })
    }

    pub(crate) fn voice_caption_is_visible(&self) -> bool {
        self.voice.caption_text().is_some()
            || (matches!(self.dialogue.phase(), DialoguePhase::Thinking)
                && self
                    .dialogue
                    .pending_transcript()
                    .is_some_and(|text| !text.trim().is_empty()))
    }

    pub(crate) fn voice_caption_animation_active(&self) -> bool {
        self.voice_caption_is_visible()
            && matches!(self.screen, crate::app::AppScreen::Canvas)
            && self.window_active
            && !self.settings_panel.open
    }

    pub(in crate::app) fn advance_voice_caption_waveform(
        &mut self,
        delta: std::time::Duration,
    ) -> bool {
        if !self.voice_caption_is_visible() {
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
                body.max_h(px(152.0))
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
                        .min_h(if expanded { px(100.0) } else { px(48.0) })
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
                        .child(caption_waveform(theme, self.voice_caption_waveform_phase))
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

fn caption_waveform(theme: Theme, phase: f32) -> Div {
    let heights = [6.0, 10.0, 15.0, 20.0, 13.0, 8.0, 17.0, 11.0, 7.0];
    let mut waveform = div()
        .flex_none()
        .w(px(34.0))
        .h(px(22.0))
        .flex()
        .items_center()
        .gap(px(2.0));
    for (index, base_height) in heights.into_iter().enumerate() {
        let pulse = ((phase * std::f32::consts::TAU + index as f32 * 0.85).sin() + 1.0) * 0.5;
        let height = base_height * (0.55 + pulse * 0.45);
        waveform = waveform.child(
            div()
                .w(px(2.0))
                .h(px(height))
                .rounded(px(1.0))
                .bg(rgba(with_alpha(theme.accent_orange, 0xdc))),
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
    let label = if expanded {
        "收起  ⌄"
    } else {
        "全文  ⌃"
    };
    let variant = ButtonCustomVariant::new(cx)
        .foreground(rgba(0xffffffc7).into())
        .hover(rgba(0xffffff18).into())
        .active(rgba(0xffffff24).into());

    Button::new("voice-caption-expand")
        .debug_selector(|| "voice-caption-expand".to_string())
        .label(label)
        .tooltip(if expanded {
            "Collapse full voice transcript"
        } else {
            "Show the full voice transcript"
        })
        .custom(variant)
        .compact()
        .min_w(px(64.0))
        .h(px(24.0))
        .rounded(px(12.0))
        .text_size(px(10.0))
        .border_color(rgba(with_alpha(theme.dock_line, 0x38)))
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
