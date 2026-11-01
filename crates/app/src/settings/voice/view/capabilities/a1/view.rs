use super::*;
use crate::settings::{
    typography::{SettingsText, TextRole},
    voice::view::components::VoiceAction,
};
use gpui::{div, prelude::*, px, Render};
use ui::component::{button::Button, Disableable};

impl Render for A1Panel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let (text, error, selector) = match &self.state {
            State::Idle => (
                ui::i18n::text("voice.a1_install_idle").to_string(),
                false,
                "voice-a1-install-idle",
            ),
            State::Saved => (
                ui::i18n::text("voice.a1_saved_configuration").to_string(),
                false,
                "voice-a1-install-saved",
            ),
            State::Installed => (
                ui::i18n::text("voice.a1_install_complete").to_string(),
                false,
                "voice-a1-install-complete",
            ),
            State::Failed(error) => (messages::failure(error), true, "voice-a1-install-failed"),
            State::Running(progress) => {
                let text = if self.cancelling {
                    ui::i18n::text("voice.a1_install_cancelling").to_string()
                } else {
                    match progress {
                        A1InstallProgress::Runtime => {
                            ui::i18n::text("voice.a1_install_runtime").to_string()
                        }
                        A1InstallProgress::Verify => {
                            ui::i18n::text("voice.a1_install_verify").to_string()
                        }
                        A1InstallProgress::Download {
                            file,
                            completed,
                            total,
                        } => ui::i18n::format(
                            "voice.a1_install_download",
                            &[
                                ("file", file.clone()),
                                (
                                    "completed",
                                    format!("{:.1}", *completed as f64 / 1_000_000.0),
                                ),
                                ("total", format!("{:.1}", *total as f64 / 1_000_000.0)),
                            ],
                        ),
                    }
                };
                (text, false, "voice-a1-install-running")
            }
        };
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .py(px(12.0))
            .child(
                div()
                    .settings_text(TextRole::RowTitle, theme)
                    .child(ui::i18n::text("voice.a1_name")),
            )
            .child(super::super::helper(theme, "voice.a1_install_hint"))
            .child(super::super::field(
                theme,
                "voice.a1_python",
                &self.input,
                "voice-a1-python",
                self.busy,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        Button::new("voice-a1-choose-python")
                            .outline()
                            .label(ui::i18n::text("voice.a1_choose_python"))
                            .disabled(self.busy)
                            .on_activate(
                                cx.listener(|view, _, window, cx| view.choose_python(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("voice-a1-download-install")
                            .debug_selector(|| "voice-a1-download-install".into())
                            .label(ui::i18n::text(if matches!(self.state, State::Failed(_)) {
                                "voice.a1_install_retry"
                            } else {
                                "voice.a1_install_action"
                            }))
                            .disabled(self.busy || self.input.read(cx).value().trim().is_empty())
                            .on_activate(cx.listener(|view, _, _, cx| view.install(cx))),
                    )
                    .when(self.busy, |row| {
                        row.child(
                            Button::new("voice-a1-cancel-install")
                                .outline()
                                .label(ui::i18n::text("voice.a1_install_cancel"))
                                .disabled(self.cancelling)
                                .on_activate(cx.listener(|view, _, _, cx| {
                                    view.cancelled.store(true, Ordering::Release);
                                    view.cancelling = true;
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .child(super::super::status(theme, text, error, selector))
            .child(
                div()
                    .settings_text(TextRole::Helper, theme)
                    .child(ui::i18n::format(
                        "voice.a1_install_location",
                        &[("path", self.root.join("a1").display().to_string())],
                    )),
            )
    }
}
