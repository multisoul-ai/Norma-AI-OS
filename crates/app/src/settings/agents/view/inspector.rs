use super::super::components::{action_button, disabled_action_button, with_alpha};
use super::super::text_input::editable_field;
use super::super::{AgentDefinitionField, AgentDefinitionsState};
use crate::app::MultisoulApp;
use canvas::AgentProfile;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, FontWeight, KeyDownEvent, Window};
use ui::component::{switch::Switch, Sizable, Size};
use ui::Theme;

pub(super) fn agent_inspector(
    theme: Theme,
    state: &AgentDefinitionsState,
    selected_agent_on_canvas: bool,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    if !state.is_creating_new() && state.draft().id.is_none() {
        return empty_inspector(theme);
    }

    let draft = state.draft();
    let dirty = state.is_dirty();
    let creating = state.is_creating_new();

    div()
        .id("settings-agent-inspector")
        .debug_selector(|| "settings-agent-inspector".to_string())
        .w_full()
        .h_full()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .child(inspector_header(theme, state, selected_agent_on_canvas, cx))
        .child(
            div()
                .id("settings-agent-inspector-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .py(px(18.0))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .child(editable_field(
                    theme,
                    state,
                    AgentDefinitionField::Name,
                    "Name",
                    window,
                    cx,
                ))
                .child(profile_buttons(theme, draft.runtime_profile, cx))
                .child(editable_field(
                    theme,
                    state,
                    AgentDefinitionField::WorkflowDescription,
                    "Workflow Instructions",
                    window,
                    cx,
                ))
                .when_some(draft.id, |this, id| {
                    this.child(canvas_status(theme, id, selected_agent_on_canvas, cx))
                })
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(format!("Used when {} receives a task.", draft.name)),
                ),
        )
        .child(inspector_footer(theme, state, dirty, creating, cx))
        .into_any_element()
}

fn canvas_status(
    theme: Theme,
    id: i64,
    on_canvas: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    div()
        .id("settings-agent-on-canvas")
        .debug_selector(|| "settings-agent-on-canvas".to_string())
        .w_full()
        .min_h(px(46.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(12.0))
        .py(px(9.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
        .bg(rgba(with_alpha(theme.panel_high, 0x58)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgba(theme.text_secondary))
                        .child("On Canvas"),
                )
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(if on_canvas {
                            "1 active card"
                        } else {
                            "Not added"
                        }),
                ),
        )
        .when(on_canvas, |this| {
            this.child(action_button(
                theme,
                "settings-agent-show-on-canvas",
                "Show on Canvas",
                theme.text_secondary,
                cx.listener(move |app, event, window, cx| {
                    app.on_agent_settings_show_on_canvas(id, event, window, cx);
                }),
            ))
        })
        .into_any_element()
}

fn inspector_header(
    theme: Theme,
    state: &AgentDefinitionsState,
    selected_agent_on_canvas: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let draft = state.draft();
    let app = cx.weak_entity();
    let app_for_keyboard = app.clone();
    let identity = if state.is_creating_new() {
        div()
            .text_size(px(18.0))
            .font_weight(FontWeight::BOLD)
            .text_color(rgba(theme.text_primary))
            .child("New Agent")
    } else {
        agent_identity(theme, &draft.name, draft.runtime_profile)
    };

    div()
        .w_full()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .pb(px(16.0))
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x28)))
        .child(identity)
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .justify_end()
                .gap(px(10.0))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(7.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(if draft.enabled { "Enabled" } else { "Disabled" }),
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
                                        .checked(draft.enabled)
                                        .with_size(Size::Medium)
                                        .tooltip("Include this Agent in project routing")
                                        .on_click(move |_checked, _window, app_cx| {
                                            let _ = app.update(app_cx, |app, cx| {
                                                app.agent_definitions.toggle_draft_enabled();
                                                app.agent_definitions.clear_status();
                                                cx.notify();
                                            });
                                        }),
                                ),
                        ),
                )
                .when_some(draft.id, |this, id| {
                    if selected_agent_on_canvas {
                        this.child(canvas_presence_indicator(theme))
                    } else if draft.enabled {
                        this.child(action_button(
                            theme,
                            "settings-agent-add-to-canvas",
                            "Add to Canvas",
                            theme.accent_orange,
                            cx.listener(move |app, event, window, cx| {
                                app.on_agent_settings_create_card(id, event, window, cx);
                            }),
                        ))
                    } else {
                        this.child(
                            div()
                                .flex()
                                .flex_col()
                                .items_end()
                                .gap(px(3.0))
                                .child(disabled_action_button(
                                    theme,
                                    "settings-agent-add-to-canvas-disabled",
                                    "Add to Canvas",
                                ))
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(rgba(theme.text_tertiary))
                                        .child("Enable before adding to Canvas"),
                                ),
                        )
                    }
                }),
        )
}

fn canvas_presence_indicator(theme: Theme) -> AnyElement {
    div()
        .id("settings-agent-on-canvas-indicator")
        .debug_selector(|| "settings-agent-on-canvas-indicator".to_string())
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.accent_cyan, 0x62)))
        .bg(rgba(with_alpha(theme.accent_cyan, 0x16)))
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(theme.accent_cyan))
        .child("✓ On Canvas")
        .into_any_element()
}

fn inspector_footer(
    theme: Theme,
    state: &AgentDefinitionsState,
    dirty: bool,
    creating: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let id = state.draft().id;
    div()
        .w_full()
        .min_h(px(58.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .pt(px(14.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x28)))
        .when_some(id, |this, id| {
            this.child(action_button(
                theme,
                "settings-agent-delete",
                "Delete Agent…",
                theme.destructive,
                cx.listener(move |app, event, window, cx| {
                    app.on_agent_settings_request_delete_definition(id, event, window, cx);
                }),
            ))
        })
        .when(id.is_none(), |this| this.child(div()))
        .when(dirty || creating, |this| {
            this.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .mr(px(4.0))
                            .text_size(px(12.0))
                            .text_color(rgba(theme.text_tertiary))
                            .child("Unsaved changes"),
                    )
                    .child(action_button(
                        theme,
                        "settings-agent-discard",
                        if creating { "Cancel" } else { "Discard" },
                        theme.text_secondary,
                        cx.listener(MultisoulApp::on_agent_settings_cancel_card_edit),
                    ))
                    .child(action_button(
                        theme,
                        "settings-agent-save",
                        if creating {
                            "Create Agent"
                        } else {
                            "Save Changes"
                        },
                        theme.accent_orange,
                        cx.listener(MultisoulApp::on_agent_settings_save),
                    )),
            )
        })
}

fn agent_identity(theme: Theme, name: &str, profile: AgentProfile) -> Div {
    let initial = name.chars().next().unwrap_or('A').to_string();
    div()
        .min_w(px(0.0))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(10.0))
        .child(
            div()
                .w(px(38.0))
                .h(px(38.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(19.0))
                .bg(rgba(with_alpha(theme.accent_orange, 0x20)))
                .text_size(px(15.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgba(theme.accent_orange))
                .child(initial),
        )
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(3.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(name.to_string()),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgba(with_alpha(theme.text_primary, 0xc0)))
                        .child(profile.display_title().to_string()),
                ),
        )
}

fn profile_buttons(theme: Theme, selected: AgentProfile, cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(theme.text_secondary))
                .child("Runtime Profile"),
        )
        .child(
            div().flex().flex_row().flex_wrap().gap(px(8.0)).children(
                [
                    ("Claude", AgentProfile::Claude),
                    ("Codex", AgentProfile::Codex),
                    ("Cursor", AgentProfile::Cursor),
                    ("Grok", AgentProfile::Grok),
                    ("OpenCode", AgentProfile::OpenCode),
                ]
                .into_iter()
                .map(|(label, profile)| profile_button(theme, label, profile, selected, cx)),
            ),
        )
}

fn profile_button(
    theme: Theme,
    label: &'static str,
    profile: AgentProfile,
    selected: AgentProfile,
    cx: &mut Context<MultisoulApp>,
) -> impl IntoElement {
    let button_id = match profile {
        AgentProfile::Claude => "settings-agent-profile-claude",
        AgentProfile::Codex => "settings-agent-profile-codex",
        AgentProfile::Cursor => "settings-agent-profile-cursor",
        AgentProfile::Grok => "settings-agent-profile-grok",
        AgentProfile::OpenCode => "settings-agent-profile-opencode",
    };
    let accent = if selected == profile {
        theme.accent_orange
    } else {
        theme.text_secondary
    };

    action_button(
        theme,
        button_id,
        label,
        accent,
        cx.listener(move |app, event, window, cx| {
            app.on_agent_settings_select_profile(profile, event, window, cx);
        }),
    )
}

fn empty_inspector(theme: Theme) -> AnyElement {
    div()
        .w_full()
        .h_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .text_color(rgba(theme.text_tertiary))
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(FontWeight::MEDIUM)
                .child("Create your first project Agent"),
        )
        .child(
            div()
                .text_size(px(12.0))
                .child("Agents can be configured once and added to Canvas when needed."),
        )
        .into_any_element()
}
