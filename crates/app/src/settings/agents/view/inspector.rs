//! 右侧编辑抽屉：身份头部、可编辑字段、运行时 Chip、启用开关、画布状态与固定操作栏。

use super::super::components::{
    avatar, icon_button, primary_button, runtime_chip, tag, text_button, with_alpha,
};
use super::super::text_input::editable_field;
use super::super::{AgentDefinitionField, AgentDefinitionsState};
use crate::app::MultisoulApp;
use crate::settings::typography::{SettingsText, TextRole};
use canvas::AgentProfile;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, KeyDownEvent, Window};
use ui::component::{switch::Switch, IconName, Sizable, Size};
use ui::Theme;

pub(super) const DRAWER_WIDTH: f32 = 380.0;

pub(super) fn agent_drawer(
    theme: Theme,
    state: &AgentDefinitionsState,
    selected_agent_on_canvas: bool,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let draft = state.draft();
    let creating = state.is_creating_new();

    div()
        .id("settings-agent-inspector")
        .debug_selector(|| "settings-agent-inspector".to_string())
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(px(DRAWER_WIDTH))
        .max_w_full()
        .occlude()
        .shadow_lg()
        .flex()
        .flex_col()
        .overflow_hidden()
        .rounded_r(px(24.0))
        .border_l_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x28)))
        .bg(rgba(with_alpha(theme.panel_high, 0xff)))
        .child(drawer_header(theme, state, selected_agent_on_canvas, cx))
        .child(
            div()
                .id("settings-agent-inspector-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .px(px(20.0))
                .pb(px(12.0))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(editable_field(
                    theme,
                    state,
                    AgentDefinitionField::Name,
                    ui::i18n::text("settings.name"),
                    window,
                    cx,
                ))
                .child(profile_chips(theme, draft.runtime_profile, cx))
                .child(editable_field(
                    theme,
                    state,
                    AgentDefinitionField::WorkflowDescription,
                    ui::i18n::text("settings.workflow_instructions"),
                    window,
                    cx,
                ))
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::format(
                            "settings.used_when_receives_a_task",
                            &[("arg0", (draft.name).to_string())],
                        )),
                )
                .child(enabled_row(theme, draft.enabled, cx))
                .when_some(draft.id, |this, id| {
                    this.child(canvas_status(
                        theme,
                        id,
                        draft.enabled,
                        selected_agent_on_canvas,
                        cx,
                    ))
                }),
        )
        .child(drawer_footer(
            theme,
            state.is_dirty(),
            creating,
            draft.id,
            cx,
        ))
        .into_any_element()
}

fn drawer_header(
    theme: Theme,
    state: &AgentDefinitionsState,
    selected_agent_on_canvas: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let draft = state.draft();
    let identity = if state.is_creating_new() {
        div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .settings_text(TextRole::Title, theme)
                    .child(ui::i18n::text("settings.new_agent")),
            )
            .child(
                div()
                    .settings_text(TextRole::Helper, theme)
                    .child(ui::i18n::text(
                        "settings.agents_can_be_configured_once_and_added_to_canvas_when_needed",
                    )),
            )
    } else {
        div()
            .min_w(px(0.0))
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                div()
                    .truncate()
                    .settings_text(TextRole::Title, theme)
                    .child(draft.name.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(6.0))
                    .child(tag(theme, draft.runtime_profile.display_title(), None))
                    .when(selected_agent_on_canvas, |this| {
                        this.child(
                            tag(
                                theme,
                                ui::i18n::text("settings.on_canvas_2"),
                                Some(theme.accent_cyan),
                            )
                            .id("settings-agent-on-canvas-indicator")
                            .debug_selector(|| "settings-agent-on-canvas-indicator".to_string()),
                        )
                    }),
            )
    };

    div()
        .w_full()
        .flex_none()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(14.0))
        .px(px(20.0))
        .pt(px(20.0))
        .pb(px(12.0))
        .child(if state.is_creating_new() {
            div()
                .w(px(40.0))
                .h(px(40.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(rgba(with_alpha(theme.text_primary, 0x14)))
                .child(
                    ui::component::Icon::new(IconName::Plus).text_color(rgba(theme.text_secondary)),
                )
        } else {
            avatar(&draft.name, draft.runtime_profile, 40.0)
        })
        .child(identity)
        .child(icon_button(
            theme,
            "settings-agent-drawer-close",
            IconName::Close,
            cx.listener(MultisoulApp::on_agent_settings_close_drawer),
        ))
}

fn profile_chips(theme: Theme, selected: AgentProfile, cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .settings_text(TextRole::Label, theme)
                .child(ui::i18n::text("settings.runtime_profile")),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(6.0))
                .children(PROFILES.into_iter().map(|(id, label, profile)| {
                    runtime_chip(
                        theme,
                        id,
                        label,
                        profile,
                        selected == profile,
                        cx.listener(move |app, event, window, cx| {
                            app.on_agent_settings_select_profile(profile, event, window, cx);
                        }),
                    )
                })),
        )
}

const PROFILES: [(&str, &str, AgentProfile); 10] = [
    (
        "settings-agent-profile-claude",
        "Claude",
        AgentProfile::Claude,
    ),
    ("settings-agent-profile-codex", "Codex", AgentProfile::Codex),
    (
        "settings-agent-profile-cursor",
        "Cursor",
        AgentProfile::Cursor,
    ),
    ("settings-agent-profile-grok", "Grok", AgentProfile::Grok),
    (
        "settings-agent-profile-opencode",
        "OpenCode",
        AgentProfile::OpenCode,
    ),
    (
        "settings-agent-profile-infcode",
        "InfCode",
        AgentProfile::InfCode,
    ),
    ("settings-agent-profile-kimi", "Kimi", AgentProfile::Kimi),
    ("settings-agent-profile-zcode", "ZCode", AgentProfile::ZCode),
    ("settings-agent-profile-pi", "Pi", AgentProfile::Pi),
    (
        "settings-agent-profile-hermes",
        "Hermes",
        AgentProfile::Hermes,
    ),
];

fn enabled_row(theme: Theme, enabled: bool, cx: &mut Context<MultisoulApp>) -> Div {
    let app = cx.weak_entity();
    let app_for_keyboard = app.clone();
    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(if enabled {
                            ui::i18n::text("settings.enabled")
                        } else {
                            ui::i18n::text("settings.disabled")
                        }),
                )
                .child(
                    div()
                        .settings_text(TextRole::Helper, theme)
                        .child(ui::i18n::text(
                            "settings.include_this_agent_in_project_routing",
                        )),
                ),
        )
        .child(
            div()
                .id("settings-agent-toggle-enabled-control")
                .debug_selector(|| "settings-agent-toggle-enabled".to_string())
                .rounded(px(6.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x00)))
                .tab_index(0)
                .focus(move |style| style.border_color(rgba(theme.accent_orange)))
                .on_key_down(move |event: &KeyDownEvent, _window, app_cx| {
                    if !matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        return;
                    }
                    let _ = app_for_keyboard.update(app_cx, |app, cx| {
                        app.agent_definitions.toggle_draft_enabled();
                        app.agent_definitions.clear_status();
                        cx.notify();
                    });
                    app_cx.stop_propagation();
                })
                .child(
                    Switch::new("settings-agent-toggle-enabled-switch")
                        .checked(enabled)
                        .with_size(Size::Medium)
                        .on_click(move |_checked, _window, app_cx| {
                            let _ = app.update(app_cx, |app, cx| {
                                app.agent_definitions.toggle_draft_enabled();
                                app.agent_definitions.clear_status();
                                cx.notify();
                            });
                        }),
                ),
        )
}

fn canvas_status(
    theme: Theme,
    id: i64,
    enabled: bool,
    on_canvas: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let action = if on_canvas {
        text_button(
            theme,
            "settings-agent-show-on-canvas",
            ui::i18n::text("settings.show_on_canvas"),
            theme.accent_orange,
            cx.listener(move |app, event, window, cx| {
                app.on_agent_settings_show_on_canvas(id, event, window, cx);
            }),
        )
    } else if enabled {
        primary_button(
            theme,
            "settings-agent-add-to-canvas",
            ui::i18n::text("settings.add_to_canvas"),
            true,
            cx.listener(move |app, event, window, cx| {
                app.on_agent_settings_create_card(id, event, window, cx);
            }),
        )
    } else {
        div()
            .id("settings-agent-add-to-canvas-disabled")
            .debug_selector(|| "settings-agent-add-to-canvas-disabled".to_string())
            .px(px(10.0))
            .py(px(5.0))
            .rounded_full()
            .bg(rgba(with_alpha(theme.text_primary, 0x10)))
            .settings_font(TextRole::Control)
            .text_color(rgba(with_alpha(theme.text_secondary, 0x90)))
            .child(ui::i18n::text("settings.add_to_canvas"))
            .into_any_element()
    };
    let caption = if on_canvas {
        ui::i18n::text("settings.1_active_card")
    } else if enabled {
        ui::i18n::text("settings.not_added")
    } else {
        ui::i18n::text("settings.enable_before_adding_to_canvas")
    };

    div()
        .id("settings-agent-on-canvas")
        .debug_selector(|| "settings-agent-on-canvas".to_string())
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(14.0))
        .py(px(12.0))
        .rounded(px(14.0))
        .bg(rgba(with_alpha(theme.text_primary, 0x0a)))
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .settings_text(TextRole::RowTitle, theme)
                        .child(ui::i18n::text("settings.on_canvas")),
                )
                .child(div().settings_text(TextRole::Helper, theme).child(caption)),
        )
        .child(action)
        .into_any_element()
}

fn drawer_footer(
    theme: Theme,
    dirty: bool,
    creating: bool,
    id: Option<i64>,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex_none()
        .min_h(px(52.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(6.0))
        .px(px(14.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x28)))
        .when_some(id, |this, id| {
            this.child(text_button(
                theme,
                "settings-agent-delete",
                ui::i18n::text("settings.delete_agent_2"),
                theme.destructive,
                cx.listener(move |app, event, window, cx| {
                    app.on_agent_settings_request_delete_definition(id, event, window, cx);
                }),
            ))
        })
        .child(div().flex_1())
        .when(dirty || creating, |this| {
            this.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .w(px(7.0))
                            .h(px(7.0))
                            .rounded_full()
                            .bg(rgba(theme.accent_orange)),
                    )
                    .child(
                        div()
                            .mr(px(4.0))
                            .settings_text(TextRole::Helper, theme)
                            .child(ui::i18n::text("cards.unsaved_changes")),
                    ),
            )
            .child(text_button(
                theme,
                "settings-agent-discard",
                if creating {
                    ui::i18n::text("common.cancel")
                } else {
                    ui::i18n::text("settings.discard")
                },
                theme.accent_orange,
                cx.listener(MultisoulApp::on_agent_settings_cancel_card_edit),
            ))
            .child(primary_button(
                theme,
                "settings-agent-save",
                if creating {
                    ui::i18n::text("settings.create_agent")
                } else {
                    ui::i18n::text("settings.save_changes")
                },
                false,
                cx.listener(MultisoulApp::on_agent_settings_save),
            ))
        })
}
