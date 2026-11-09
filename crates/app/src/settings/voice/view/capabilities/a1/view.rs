use super::*;
use crate::settings::{
    typography::{SettingsText, TextRole},
    voice::view::components::VoiceAction,
};
use gpui::{div, prelude::*, px, Context, Render, Window};
use ui::component::{button::Button, Disableable};

impl Render for A1Panel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let installed = matches!(self.state, State::Installed);
        let primary_selector = if installed {
            "voice-a1-installed"
        } else {
            "voice-a1-download-install"
        };
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
                                (
                                    "file",
                                    if matches!(file.as_str(), "engine.json" | "engine.tar.bz2") {
                                        ui::i18n::text("voice.a1_engine_download_label").to_string()
                                    } else {
                                        file.clone()
                                    },
                                ),
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
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        Button::new("voice-a1-download-install")
                            .debug_selector(move || primary_selector.into())
                            .label(ui::i18n::text(self.state.install_label_key()))
                            .disabled(self.busy || !self.state.can_install())
                            .on_activate(cx.listener(|view, _, _, cx| {
                                if view.state.can_install() {
                                    view.install(cx);
                                }
                            })),
                    )
                    .when(installed, |row| {
                        row.child(
                            Button::new("voice-a1-reinstall")
                                .outline()
                                .debug_selector(|| "voice-a1-reinstall".into())
                                .label(ui::i18n::text("voice.a1_reinstall"))
                                .on_activate(cx.listener(|view, _, _, cx| view.install(cx))),
                        )
                    })
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
    }
}
