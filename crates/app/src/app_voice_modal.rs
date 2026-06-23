//! Voice confirmation modal rendering and actions.

use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, BoxShadow, ClickEvent, Context, Div, FontWeight, Window};

impl MultisoulApp {
    pub(crate) fn render_voice_confirmation_modal(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<Div> {
        let (transcript, reason) = self.voice.confirmation()?;
        let transcript = transcript.to_string();
        let reason = reason.to_string();
        let theme = self.theme;
        Some(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(with_alpha(theme.graphite, 0x8a)))
                .child(
                    div()
                        .w(px(420.0))
                        .p(px(18.0))
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.edge_highlight, 0x42)))
                        .shadow(vec![BoxShadow {
                            color: rgba(theme.shadow_color).into(),
                            offset: gpui::point(px(0.0), px(18.0)),
                            blur_radius: px(48.0),
                            spread_radius: px(-12.0),
                        }])
                        .bg(rgba(with_alpha(theme.panel, 0xf0)))
                        .text_color(rgba(theme.text_primary))
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_size(px(14.0))
                                .child("Confirm voice command"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(transcript),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.accent_orange))
                                .child(reason),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .justify_end()
                                .gap_2()
                                .child(
                                    modal_button(theme, "Cancel", false)
                                        .id("voice-confirm-cancel")
                                        .on_click(cx.listener(Self::on_voice_cancel)),
                                )
                                .child(
                                    modal_button(theme, "Confirm", true)
                                        .id("voice-confirm-confirm")
                                        .on_click(cx.listener(Self::on_voice_confirm)),
                                ),
                        ),
                ),
        )
    }

    fn on_voice_confirm(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let commands = std::mem::take(&mut self.pending_voice_commands);
        if commands.is_empty() {
            self.voice.set_error("No pending voice command to confirm.");
            cx.notify();
            return;
        }
        match self.execute_compiled_voice_plan(commands, window) {
            Ok(message) => {
                self.voice.set_executed(message);
                self.save_voice_history("".into(), "{}".into(), 0.0, "medium", "confirmed", None);
            }
            Err(error) => {
                self.voice
                    .set_error(format!("Voice command failed: {error}"));
                self.save_voice_history(
                    "".into(),
                    "{}".into(),
                    0.0,
                    "medium",
                    "runtime_error",
                    Some(error),
                );
            }
        }
        cx.notify();
    }

    fn on_voice_cancel(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.pending_voice_commands.clear();
        let transcript = self
            .voice
            .confirmation()
            .map(|(transcript, _)| transcript.to_string())
            .unwrap_or_default();
        self.voice
            .set_transcript_only(transcript, "voice command cancelled");
        self.save_voice_history("".into(), "{}".into(), 0.0, "medium", "cancelled", None);
        cx.notify();
    }
}

fn modal_button(theme: ui::Theme, label: &'static str, primary: bool) -> Div {
    let fill = if primary {
        with_alpha(theme.accent_orange, 0x44)
    } else {
        with_alpha(theme.edge_highlight, 0x10)
    };
    let stroke = if primary {
        with_alpha(theme.accent_orange, 0x88)
    } else {
        with_alpha(theme.edge_highlight, 0x30)
    };
    div()
        .h(px(30.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(stroke))
        .bg(rgba(fill))
        .text_size(px(12.0))
        .child(label)
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
