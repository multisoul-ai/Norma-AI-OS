use super::*;
use crate::settings::{
    typography::{SettingsText, TextRole},
    voice::view::components::VoiceAction,
};
use gpui::{div, prelude::*, px, Render};
use ui::component::{button::Button, Disableable};

fn failure(error: &JevConfigurationError) -> String {
    let key = match error {
        JevConfigurationError::InvalidKey => "voice.jev_invalid_key",
        JevConfigurationError::SecureStorage => "voice.jev_secure_storage_error",
        JevConfigurationError::SettingsStorage => "voice.jev_settings_storage_error",
        JevConfigurationError::Network => "voice.jev_network_error",
        JevConfigurationError::Timeout => "voice.jev_timeout",
        JevConfigurationError::InvalidResponse => "voice.jev_invalid_response",
        JevConfigurationError::Http(status) => {
            return ui::i18n::format("voice.jev_http_error", &[("status", status.to_string())])
        }
    };
    ui::i18n::text(key).to_string()
}

impl Render for JevPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        let production = self
            .settings
            .as_ref()
            .is_some_and(JevSettings::production_enabled);
        debug_assert!(
            !production,
            "The configuration UI cannot enable the Jev production gate"
        );
        let (text, error, selector) = match &self.state {
            State::Loading => (
                ui::i18n::text("voice.jev_loading").to_string(),
                false,
                "voice-jev-loading",
            ),
            State::Unconfigured => (
                ui::i18n::text("voice.not_configured").to_string(),
                false,
                "voice-jev-unconfigured",
            ),
            State::Draft => (
                ui::i18n::text("voice.jev_draft").to_string(),
                false,
                "voice-jev-draft",
            ),
            State::Saved => (
                ui::i18n::text("voice.jev_saved").to_string(),
                false,
                "voice-jev-saved",
            ),
            State::Failed(error) => (failure(error), true, "voice-jev-failed"),
            State::Connected(info, saved) => (
                format!(
                    "{}{}",
                    ui::i18n::format(
                        "voice.jev_connected",
                        &[
                            ("model", info.model.clone()),
                            ("elapsed", info.elapsed_ms.to_string())
                        ]
                    ),
                    if *saved {
                        String::new()
                    } else {
                        format!(" · {}", ui::i18n::text("voice.jev_draft"))
                    }
                ),
                false,
                "voice-jev-connected",
            ),
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
                    .child(ui::i18n::text("voice.jev_title")),
            )
            .child(super::super::helper(theme, "voice.jev_configuration_hint"))
            .child(
                div()
                    .settings_text(TextRole::Helper, theme)
                    .child(ui::i18n::format(
                        "voice.jev_endpoint",
                        &[("endpoint", agent::jev_choice::JEV_ENDPOINT.into())],
                    )),
            )
            .child(
                div()
                    .settings_text(TextRole::Helper, theme)
                    .child(ui::i18n::format(
                        "voice.jev_request_model",
                        &[("model", agent::jev_choice::JEV_MODEL.into())],
                    )),
            )
            .child(super::super::field(
                theme,
                "voice.jev_api_key",
                &self.input,
                "voice-jev-api-key",
                self.busy,
                cx,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .child(
                        Button::new("voice-jev-save")
                            .debug_selector(|| "voice-jev-save".into())
                            .label(ui::i18n::text("voice.jev_save"))
                            .disabled(
                                self.busy
                                    || self.settings.is_none()
                                    || self.input.read(cx).value().trim().is_empty(),
                            )
                            .on_activate(cx.listener(|view, _, window, cx| view.save(window, cx))),
                    )
                    .child(
                        Button::new("voice-jev-verify")
                            .debug_selector(|| "voice-jev-verify".into())
                            .outline()
                            .label(ui::i18n::text(if self.busy {
                                "voice.jev_busy"
                            } else {
                                "voice.jev_verify"
                            }))
                            .disabled(self.busy || self.key(cx).is_none())
                            .on_activate(cx.listener(|view, _, _, cx| view.verify(cx))),
                    )
                    .child(
                        Button::new("voice-jev-clear")
                            .outline()
                            .label(ui::i18n::text("voice.jev_clear"))
                            .disabled(
                                self.busy
                                    || self.settings.is_none()
                                    || (self.saved_key.is_none()
                                        && self.input.read(cx).value().is_empty()),
                            )
                            .on_activate(cx.listener(|view, _, window, cx| view.clear(window, cx))),
                    )
                    .when(self.environment_key.is_some(), |row| {
                        row.child(
                            Button::new("voice-jev-import-env")
                                .outline()
                                .label(ui::i18n::text("voice.jev_import_environment"))
                                .disabled(self.busy)
                                .on_activate(cx.listener(|view, _, window, cx| {
                                    if view.busy {
                                        return;
                                    }
                                    if let Some(key) = view.environment_key.clone() {
                                        view.input.update(cx, |input, cx| {
                                            input.set_value(key, window, cx)
                                        });
                                    }
                                })),
                        )
                    }),
            )
            .child(super::super::status(theme, text, error, selector))
            .child(
                super::super::helper(theme, "voice.jev_production_disabled")
                    .debug_selector(|| "voice-jev-production-disabled".into()),
            )
    }
}
