use super::super::{action_button, setting_row, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::assets::TOOLBAR_PREVIEW_ASSET;
use crate::settings::voice::state::{VoiceSettingsField, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, svg, Context, Div, SharedString};
use ui::Theme;

pub(super) fn api_key_row(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let configured = state
        .values()
        .active_provider_profile()
        .credential_configured();
    let revealed = inputs.api_key_revealed(cx);
    setting_row(
        theme,
        "API Key",
        "Stored in macOS Keychain; hidden by default and never written to logs",
    )
    .child(
        div()
            .w_full()
            .flex()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap(px(10.0))
            .child(inputs.input(VoiceSettingsField::ApiKey))
            .when(configured, |row| {
                row.child(visibility_button(theme, revealed, cx))
            })
            .child(
                action_button(theme, "Remove", theme.destructive)
                    .id("settings-agent-provider-api-key-clear")
                    .debug_selector(|| "settings-agent-provider-api-key-clear".to_string())
                    .on_click(cx.listener(MultisoulApp::on_voice_settings_clear_api_key)),
            ),
    )
}

fn visibility_button(
    theme: Theme,
    revealed: bool,
    cx: &mut Context<MultisoulApp>,
) -> gpui::Stateful<Div> {
    let color = if revealed {
        theme.accent_orange
    } else {
        theme.text_secondary
    };
    div()
        .id("settings-agent-provider-api-key-visibility")
        .debug_selector(|| "settings-agent-provider-api-key-visibility".to_string())
        .w(px(38.0))
        .h(px(38.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(color, if revealed { 0x90 } else { 0x38 })))
        .bg(rgba(with_alpha(color, if revealed { 0x18 } else { 0x08 })))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(color, 0x20))))
        .on_click(cx.listener(MultisoulApp::on_voice_settings_toggle_api_key_visibility))
        .child(
            svg()
                .path(SharedString::from(TOOLBAR_PREVIEW_ASSET))
                .w(px(16.0))
                .h(px(16.0))
                .text_color(rgba(color)),
        )
}
