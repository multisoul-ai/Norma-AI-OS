//! Live Voice transcript caption above the Command Dock.

use crate::app::MultisoulApp;
use dialogue::DialoguePhase;
use gpui::{
    div, linear_color_stop, linear_gradient, point, prelude::*, px, rgba, BoxShadow, Context, Div,
    KeyDownEvent, MouseButton, Window,
};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::icons::with_alpha;
use ui::Theme;

const COLLAPSED_PREVIEW_CHARS: usize = 96;

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
            .relative()
            .min_w(px(0.0))
            .flex_1()
            .whitespace_normal()
            .line_height(px(20.0))
            .text_size(px(13.0))
            .text_color(rgba(0xfffffff0))
            .when(expanded, |body| {
                body.max_h(px(176.0))
                    .overflow_y_scroll()
                    .pr(px(8.0))
                    .child(visible_text.clone())
            })
            .when(!expanded, |body| {
                body.h(px(40.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .right(px(0.0))
                            .bottom(px(0.0))
                            .whitespace_normal()
                            .child(visible_text.clone()),
                    )
                    .child(
                        div()
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .bottom(px(0.0))
                            .w(px(28.0))
                            .bg(linear_gradient(
                                90.0,
                                linear_color_stop(rgba(with_alpha(theme.dock, 0xf7)), 0.0),
                                linear_color_stop(rgba(with_alpha(theme.dock, 0x00)), 1.0),
                            )),
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
                        .max_w(px(720.0))
                        .min_h(if expanded { px(126.0) } else { px(86.0) })
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(16.0))
                        .px(px(18.0))
                        .py(px(12.0))
                        .rounded(px(18.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.dock_line, 0x72)))
                        .shadow(caption_shadow())
                        .bg(rgba(with_alpha(theme.dock, 0xf7)))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(caption_waveform(theme))
                        .child(caption_divider(theme))
                        .child(
                            div()
                                .flex_none()
                                .whitespace_nowrap()
                                .text_size(px(12.0))
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

fn caption_waveform(theme: Theme) -> Div {
    let heights = [8.0, 14.0, 22.0, 30.0, 20.0, 12.0, 26.0, 18.0, 9.0];
    let mut waveform = div()
        .flex_none()
        .w(px(78.0))
        .h(px(32.0))
        .flex()
        .items_center()
        .gap(px(3.0));
    for height in heights {
        waveform = waveform.child(
            div()
                .w(px(3.0))
                .h(px(height))
                .rounded(px(1.5))
                .bg(rgba(with_alpha(theme.accent_orange, 0xdc))),
        );
    }
    waveform
}

fn caption_divider(theme: Theme) -> Div {
    div()
        .flex_none()
        .w(px(1.0))
        .h(px(42.0))
        .bg(rgba(with_alpha(theme.dock_line, 0x50)))
}

fn caption_expand_control(expanded: bool, theme: Theme, cx: &mut Context<MultisoulApp>) -> Button {
    let label = if expanded {
        "Show less  ⌄"
    } else {
        "Full transcript  ⌃"
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
        .min_w(px(112.0))
        .h(px(34.0))
        .rounded(px(17.0))
        .text_size(px(11.0))
        .border_color(rgba(with_alpha(theme.dock_line, 0x38)))
        .on_key_down(cx.listener(MultisoulApp::on_voice_caption_key_down))
        .on_click(cx.listener(MultisoulApp::on_voice_caption_toggle))
}

fn caption_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: rgba(0x0409123d).into(),
        offset: point(px(0.0), px(16.0)),
        blur_radius: px(42.0),
        spread_radius: px(-12.0),
    }]
}
