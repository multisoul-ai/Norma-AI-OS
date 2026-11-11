use super::{components::component_action, setup::recognition, with_alpha, VoiceSettingsInputs};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use crate::settings::voice::{AdvancedGroup, AdvancedStatus, AdvancedSummary, VoiceSettingsState};
use gpui::{div, prelude::*, px, rgba, ClickEvent, Context, Div, ElementId, SharedString};
use ui::Theme;

/// 「更多设置」手风琴：每组一行（标题 + 一句摘要），同一时刻只展开一组。
pub(super) fn advanced_groups(
    theme: Theme,
    state: &VoiceSettingsState,
    inputs: &VoiceSettingsInputs,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let source = state.setup_asr_source();
    let recognition_body = div()
        .w_full()
        .flex()
        .flex_col()
        .when(source == asr::AsrProviderId::CloudAzure, |rows| {
            rows.child(recognition::azure_detail(theme, state, inputs, cx))
        })
        .when(source != asr::AsrProviderId::CloudAzure, |rows| {
            rows.child(super::setup::a1::a1_status_row(theme, state, cx))
        })
        .when(source == asr::AsrProviderId::LocalA1, |rows| {
            rows.child(super::setup::a1::a1_detail(theme, state, inputs, cx))
        })
        .when(source == asr::AsrProviderId::LocalSherpa, |rows| {
            rows.child(recognition::local_detail(theme, state, cx))
        });
    let model_body =
        div()
            .w_full()
            .flex()
            .flex_col()
            .child(super::provider::agent_provider_setup_group(
                theme, state, inputs, cx,
            ));
    let jev_body = div().w_full().children(inputs.jev_panel.clone());
    let reply_body = super::spoken_reply::reply_preferences(theme, state, cx);
    let diagnostics_body = div().w_full().child(component_action(
        theme,
        "voice-command-recognition-details",
        ui::i18n::text("voice.show"),
        cx,
        cx.listener(MultisoulApp::on_voice_manage_asr_models),
    ));
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(group(
            theme,
            state,
            AdvancedGroup::Model,
            ui::i18n::text("voice.command_model"),
            model_body,
            cx,
        ))
        .child(group(
            theme,
            state,
            AdvancedGroup::Recognition,
            ui::i18n::text("voice.group_recognition"),
            recognition_body,
            cx,
        ))
        .child(group(
            theme,
            state,
            AdvancedGroup::Jev,
            ui::i18n::text("voice.group_jev"),
            jev_body,
            cx,
        ))
        .child(group(
            theme,
            state,
            AdvancedGroup::Reply,
            ui::i18n::text("voice.group_reply"),
            reply_body,
            cx,
        ))
        .child(group(
            theme,
            state,
            AdvancedGroup::Diagnostics,
            ui::i18n::text("voice.local_engine_details"),
            diagnostics_body,
            cx,
        ))
}

fn group(
    theme: Theme,
    state: &VoiceSettingsState,
    which: AdvancedGroup,
    title: impl Into<SharedString>,
    body: Div,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let open = state.advanced_group() == Some(which);
    let summary = summary_text(which, &state.advanced_group_summary(which));
    let status = state.advanced_group_status(which);
    let name = format!("voice-advanced-group-{which:?}");
    let id: ElementId = SharedString::from(name.clone()).into();
    let header = div()
        .id(id.clone())
        .debug_selector(move || name.clone())
        .w_full()
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(10.0))
        .rounded(px(12.0))
        .cursor_pointer()
        .border_1()
        .border_color(rgba(0x00000000))
        .hover(|row| row.bg(rgba(with_alpha(theme.text_primary, 0x10))))
        .active(|row| row.bg(rgba(with_alpha(theme.text_primary, 0x1c))))
        .tab_index(0)
        .focus(move |row| row.border_color(rgba(theme.accent_blue)))
        .on_click(cx.listener(move |app, _: &ClickEvent, _, cx| {
            app.voice_settings.toggle_advanced_group(which);
            cx.notify();
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
                        .child(title.into()),
                )
                .child(div().settings_text(TextRole::Helper, theme).child(summary)),
        )
        .children(status.map(|status| status_tag(theme, which, status)))
        .child(
            div()
                .settings_text(TextRole::Helper, theme)
                .text_color(rgba(theme.text_secondary))
                .child(if open { "⌃" } else { "⌄" }),
        );
    div()
        .w_full()
        .flex()
        .flex_col()
        .rounded(px(12.0))
        .when(open, |g| g.bg(rgba(with_alpha(theme.text_primary, 0x08))))
        .child(header)
        .when(open, |g| {
            g.child(div().px(px(12.0)).pb(px(12.0)).child(body))
        })
}

fn summary_text(which: AdvancedGroup, summary: &AdvancedSummary) -> String {
    match summary {
        AdvancedSummary::Model(Some(model)) => {
            ui::i18n::format("voice.group_model_summary", &[("model", model.clone())])
        }
        AdvancedSummary::Model(None) => ui::i18n::text("voice.not_configured").to_string(),
        AdvancedSummary::Source(source) => ui::i18n::format(
            "voice.group_recognition_summary_source",
            &[(
                "source",
                ui::i18n::text(source_name_key(*source)).to_string(),
            )],
        ),
        AdvancedSummary::Reply { announcements } => ui::i18n::format(
            "voice.group_reply_summary_state",
            &[("announcements", on_off(*announcements))],
        ),
        AdvancedSummary::Default => ui::i18n::text(if which == AdvancedGroup::Jev {
            "voice.group_jev_summary"
        } else {
            "voice.command_engine_managed"
        })
        .to_string(),
    }
}

fn on_off(enabled: bool) -> String {
    ui::i18n::text(if enabled {
        "voice.group_on"
    } else {
        "voice.group_off"
    })
    .to_string()
}

fn source_name_key(source: asr::AsrProviderId) -> &'static str {
    match source {
        asr::AsrProviderId::LocalSherpa => "voice.group_source_sherpa",
        asr::AsrProviderId::LocalA1 => "voice.a1_name",
        asr::AsrProviderId::CloudAzure => "voice.group_source_azure",
    }
}

fn status_tag(theme: Theme, which: AdvancedGroup, status: AdvancedStatus) -> Div {
    let (key, color) = match status {
        AdvancedStatus::Pending => ("voice.group_status_pending", theme.accent_warm),
        AdvancedStatus::NeedsConnection => ("voice.group_status_connect", theme.accent_warm),
        AdvancedStatus::Ready => ("voice.group_status_ready", theme.accent_green),
        AdvancedStatus::Unconfigured => ("voice.group_status_unset", theme.text_secondary),
    };
    div()
        .debug_selector(move || format!("voice-advanced-status-{which:?}"))
        .px(px(8.0))
        .py(px(2.0))
        .rounded(px(999.0))
        .bg(rgba(with_alpha(color, 0x22)))
        .settings_text(TextRole::Helper, theme)
        .text_color(rgba(color))
        .child(ui::i18n::text(key))
}
