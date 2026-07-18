use super::{action_button, with_alpha};
use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, Stateful};
use ui::Theme;

pub(super) fn voice_ready_overview(theme: Theme, cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .child(ready_banner(theme))
        .child(ready_summary(theme, cx))
}

fn ready_banner(theme: Theme) -> Stateful<Div> {
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
                        .child("All required models are configured. Press ⌘E to talk."),
                ),
        )
}

fn ready_summary(theme: Theme, cx: &mut Context<MultisoulApp>) -> Stateful<Div> {
    let change_asr = action_button(theme, "Change", theme.accent_blue)
        .id("voice-ready-change-asr")
        .debug_selector(|| "voice-ready-change-asr".to_string())
        .on_click(cx.listener(MultisoulApp::on_voice_manage_asr_models));
    let change_tts = action_button(theme, "Change", theme.accent_blue)
        .id("voice-ready-change-tts")
        .debug_selector(|| "voice-ready-change-tts".to_string())
        .on_click(cx.listener(MultisoulApp::on_voice_manage_tts_models));
    div()
        .id("voice-ready-summary")
        .debug_selector(|| "voice-ready-summary".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .p(px(14.0))
        .rounded(px(14.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x24)))
        .bg(rgba(with_alpha(theme.text_primary, 0x08)))
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
            None,
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
            action_button(theme, "Review Setup", theme.accent_blue)
                .id("voice-review-setup")
                .debug_selector(|| "voice-review-setup".to_string())
                .w_full()
                .flex()
                .items_center()
                .justify_center()
                .mt(px(10.0))
                .on_click(cx.listener(MultisoulApp::on_voice_review_setup)),
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
