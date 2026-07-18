use super::{action_button, with_alpha};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, FontWeight, Window};
use ui::Theme;

pub(super) fn voice_ready_banner(theme: Theme, cx: &mut Context<MultisoulApp>) -> Div {
    div().w_full().child(
        div()
            .id("voice-ready-banner")
            .debug_selector(|| "voice-ready-banner".to_string())
            .w_full()
            .flex()
            .items_center()
            .gap(px(12.0))
            .p(px(14.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(rgba(with_alpha(theme.accent_green, 0x70)))
            .bg(rgba(with_alpha(theme.accent_green, 0x16)))
            .child(
                div()
                    .w(px(28.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(14.0))
                    .bg(rgba(theme.accent_green))
                    .text_color(rgba(0xffffffff))
                    .child("✓"),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgba(theme.text_primary))
                            .child("Voice is ready"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgba(theme.text_secondary))
                            .child("Press ⌘E to start a voice command."),
                    ),
            )
            .child(
                action_button(theme, "Try Voice", theme.accent_blue)
                    .id("voice-try-now")
                    .debug_selector(|| "voice-try-now".to_string())
                    .on_click(cx.listener(MultisoulApp::on_voice_try_now)),
            )
            .child(
                action_button(theme, "Review Setup", theme.accent_cyan)
                    .id("voice-review-setup")
                    .debug_selector(|| "voice-review-setup".to_string())
                    .on_click(cx.listener(MultisoulApp::on_voice_review_setup)),
            ),
    )
}

impl MultisoulApp {
    fn on_voice_try_now(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.activate_ready_voice(true, window, cx);
    }
}
