use super::{
    components::component_action, doubao::desktop_mode_picker, interaction::interaction_rows,
};
use super::{ready::voice_status_overview, setup::recognition, with_alpha, VoiceSettingsView};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::VoiceRequirement;
use gpui::{div, prelude::*, px, rgba, Context, Div, KeyDownEvent, Stateful, Window};
use ui::Theme;

pub(super) fn grouped_settings(
    view: VoiceSettingsView<'_>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
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
                |rows| rows.child(super::setup::a1::a1_status_row(theme, state, cx)),
            ),
    );
    let speak = section(
        theme,
        "speak",
        "voice.output",
        if state.tts_picker_open() {
            super::tts_provider::tts_provider_settings(theme, state, inputs, window, cx)
        } else {
            div().child(super::tts_provider::spoken_reply_row(
                theme, state, inputs, cx,
            ))
        },
    );
    let advanced = div()
        .id("advanced")
        .debug_selector(|| "voice-command-group-advanced".to_string())
        .w_full()
        .min_w(px(0.0))
        .child(advanced_rows(theme, state, inputs, cx));
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
    let toggle = div()
        .id("voice-command-advanced-toggle")
        .debug_selector(|| "voice-command-advanced-toggle".to_string())
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(12.0))
        .rounded(px(12.0))
        .cursor_pointer()
        .border_1()
        .border_color(rgba(0x00000000))
        .hover(|row| row.bg(rgba(with_alpha(theme.text_primary, 0x10))))
        .active(|row| row.bg(rgba(with_alpha(theme.text_primary, 0x1c))))
        .tab_index(0)
        .focus(move |row| row.border_color(rgba(theme.accent_blue)))
        .on_click(cx.listener(MultisoulApp::on_voice_settings_toggle_advanced))
        .on_key_down(cx.listener(|app, event: &KeyDownEvent, _, cx| {
            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                app.voice_settings.toggle_advanced_expanded();
                cx.notify();
                cx.stop_propagation();
            }
        }))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(ui::i18n::text("voice.more_settings")),
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::text("voice.advanced_settings_summary")),
                ),
        )
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .text_color(rgba(theme.text_secondary))
                .child(if expanded { "⌃" } else { "⌄" }),
        );
    let source = state.setup_asr_source();
    let body = div()
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .when(source == asr::AsrProviderId::CloudAzure, |rows| {
            rows.child(recognition::azure_detail(theme, state, inputs, cx))
        })
        .when(source != asr::AsrProviderId::CloudAzure, |rows| {
            rows.child(super::setup::a1::a1_detail(theme, state, inputs, cx))
        })
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("voice.command_session_hint")),
        )
        .child(
            div()
                .px(px(12.0))
                .settings_text(TextRole::RowTitle, theme)
                .child(ui::i18n::text("voice.command_model")),
        )
        .child(
            div()
                .px(px(12.0))
                .settings_text(TextRole::Helper, theme)
                .child(ui::i18n::text("voice.command_model_hint")),
        )
        .child(super::provider::agent_provider_setup_group(
            theme, state, inputs, cx,
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
        .gap(px(4.0))
        .child(
            div()
                .px(px(12.0))
                .pt(px(8.0))
                .settings_text(TextRole::Label, theme)
                .text_color(rgba(theme.accent_blue))
                .child(ui::i18n::text(title)),
        )
        .child(div().w_full().min_w(px(0.0)).child(rows))
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
