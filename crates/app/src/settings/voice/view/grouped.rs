use super::{
    components::component_action, doubao::desktop_mode_picker, interaction::interaction_rows,
};
use super::{ready::voice_status_overview, setup::recognition, with_alpha, VoiceSettingsView};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::VoiceRequirement;
use gpui::{div, prelude::*, px, rgba, Context, Div, Stateful};
use ui::Theme;

pub(super) fn grouped_settings(view: VoiceSettingsView<'_>, cx: &mut Context<MultisoulApp>) -> Div {
    let theme = view.theme;
    let state = view.state;
    let inputs = view.inputs;
    let listen = section(
        theme,
        "listen",
        "voice.command_input",
        div()
            .child(recognition::source_selector(
                theme,
                state,
                state.setup_asr_source(),
                cx,
            ))
            .when(
                state.setup_asr_source() == asr::AsrProviderId::CloudAzure,
                |rows| rows.child(recognition::azure_detail(theme, state, inputs, cx)),
            )
            .child(interaction_rows(
                theme,
                state,
                view.voice_shortcut_label,
                cx,
            ))
            .child(super::input_device::input_device_row(theme, state, cx))
            .child(recognition::microphone_detail(
                theme,
                state.requirement_state(VoiceRequirement::Microphone),
                cx,
            ))
            .when(
                state.setup_asr_source() != asr::AsrProviderId::CloudAzure,
                |rows| rows.child(super::setup::a1::a1_detail(theme, state, inputs, cx)),
            ),
    );
    let speak = section(
        theme,
        "speak",
        "voice.spoken_replies",
        div().child(super::tts_provider::tts_provider_settings(
            theme, state, inputs, cx,
        )),
    );
    let advanced = section(
        theme,
        "advanced",
        "voice.advanced_settings",
        advanced_rows(theme, state, inputs, cx),
    );
    let column = div()
        .flex_1()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(listen)
        .child(speak)
        .child(advanced);
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(20.0))
        .child(desktop_mode_picker(theme, state, cx))
        .child(voice_status_overview(
            theme,
            state,
            view.voice_shortcut_label,
            cx,
        ))
        .child(column)
        .children(super::status::status_message(theme, state))
}

// 高级设置收纳理解模型、本地引擎诊断、回复偏好与豆包/GPT-Live 的附加面板；默认折叠。
fn advanced_rows(
    theme: Theme,
    state: &crate::settings::voice::VoiceSettingsState,
    inputs: &super::VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let expanded = state.advanced_expanded();
    let toggle = detail_row(
        theme,
        "voice.recognition_models_provider_diagnostics",
        "voice.manage_the_asr_catalog_connection_details_and_troubleshooting",
        component_action(
            theme,
            "voice-command-advanced-toggle",
            if expanded {
                ui::i18n::text("voice.hide")
            } else {
                ui::i18n::text("voice.show")
            },
            cx,
            cx.listener(MultisoulApp::on_voice_settings_toggle_advanced),
        ),
    );
    let body = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("voice.command_session_hint")),
        )
        .child(detail_row(
            theme,
            "voice.command_model",
            "voice.command_model_hint",
            component_action(
                theme,
                "voice-command-model-details",
                ui::i18n::text("mobile.configure"),
                cx,
                cx.listener(MultisoulApp::on_voice_manage_provider),
            ),
        ))
        .child(value_row(
            theme,
            "voice.command_current_model",
            state.values().active_provider_profile().model().to_string(),
        ))
        .when_some(inputs.jev_panel.clone(), |rows, panel| rows.child(panel))
        .child(detail_row(
            theme,
            "voice.local_engine_details",
            "voice.command_engine_managed",
            component_action(
                theme,
                "voice-command-recognition-details",
                ui::i18n::text("voice.show"),
                cx,
                cx.listener(MultisoulApp::on_voice_manage_asr_models),
            ),
        ))
        .child(super::spoken_reply::reply_preferences(theme, state, cx));
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .child(toggle)
        .when(expanded, |rows| rows.child(body))
}

fn section(theme: Theme, id: &'static str, title: &'static str, rows: Div) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || format!("voice-command-group-{id}"))
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .settings_text(TextRole::Headline, theme)
                .child(ui::i18n::text(title)),
        )
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .p(px(16.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.text_primary, 0x18)))
                .bg(rgba(theme.panel_high))
                .child(rows),
        )
}

fn value_row(theme: Theme, title: &'static str, value: String) -> Div {
    let value = if value.trim().is_empty() {
        ui::i18n::text("voice.not_configured").to_string()
    } else {
        value
    };
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_wrap()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.text_primary, 0x10)))
        .child(
            div()
                .flex_1()
                .min_w(px(120.0))
                .settings_text(TextRole::RowTitle, theme)
                .child(ui::i18n::text(title)),
        )
        .child(
            div()
                .flex_none()
                .max_w(px(280.0))
                .debug_selector(move || format!("voice-command-value-{title}"))
                .settings_text(TextRole::Helper, theme)
                .child(value),
        )
}

fn detail_row(theme: Theme, title: &'static str, hint: &'static str, action: Stateful<Div>) -> Div {
    div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_wrap()
        .items_center()
        .gap(px(12.0))
        .py(px(10.0))
        .child(div().flex_1().min_w(px(140.0)).child(super::setting_label(
            theme,
            ui::i18n::text(title),
            ui::i18n::text(hint),
        )))
        .child(action)
}
