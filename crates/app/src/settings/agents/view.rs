use super::components::{action_button, empty_list_message, status_block, status_chip, with_alpha};
use super::text_input::editable_field;
use super::{AgentDefinitionField, AgentDefinitionsState};
use crate::app::MultisoulApp;
use canvas::AgentProfile;
use gpui::{
    div, prelude::*, px, rgba, AnyElement, Context, Div, FontWeight, MouseButton, MouseDownEvent,
    Window,
};
use storage::AgentDefinition;
use ui::Theme;

pub(crate) fn agents_settings(
    theme: Theme,
    state: &AgentDefinitionsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    if !state.has_project_store() {
        return div()
            .pt(px(8.0))
            .child(div().id("settings-agent-empty-project").child(status_block(
                theme,
                "Open a project to configure project Agents.",
                false,
            )));
    }

    div()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .pt(px(8.0))
        .child(toolbar(theme, cx))
        .child(agent_cards(theme, state, window, cx))
        .when_some(state.status(), |this, status| {
            this.child(status_block(theme, status, status.starts_with("Could not")))
        })
}

fn toolbar(theme: Theme, cx: &mut Context<MultisoulApp>) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.text_primary))
                        .child("Agent Cards"),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child("Double-click a card to edit it in place."),
                ),
        )
        .child(
            action_button(theme, "+ New", theme.accent_orange)
                .id("settings-agent-new")
                .debug_selector(|| "settings-agent-new".to_string())
                .on_click(cx.listener(MultisoulApp::on_agent_settings_new_definition)),
        )
}

fn agent_cards(
    theme: Theme,
    state: &AgentDefinitionsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let definitions = state.definitions();

    div()
        .id("settings-agents-list")
        .debug_selector(|| "settings-agents-list".to_string())
        .flex()
        .flex_col()
        .gap(px(12.0))
        .when(definitions.is_empty() && !state.is_creating_new(), |this| {
            this.child(empty_list_message(theme, "No project agents saved yet."))
        })
        .children(definitions.iter().map(|definition| {
            if state.editing_card_id() == Some(definition.id) {
                editing_agent_card(theme, state, Some(definition.id), window, cx)
            } else {
                readonly_agent_card(
                    theme,
                    definition,
                    state.selected_definition_id() == Some(definition.id),
                    cx,
                )
            }
        }))
        .when(state.is_creating_new(), |this| {
            this.child(editing_agent_card(theme, state, None, window, cx))
        })
        .into_any_element()
}

fn readonly_agent_card(
    theme: Theme,
    definition: &AgentDefinition,
    selected: bool,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let id = definition.id;
    let border = if selected {
        with_alpha(theme.accent_orange, 0x80)
    } else {
        with_alpha(theme.edge_highlight, 0x30)
    };
    let bg = if selected {
        with_alpha(theme.accent_orange, 0x14)
    } else {
        with_alpha(theme.node_fill, 0xcc)
    };

    div()
        .id(("settings-agent-card", id as u64))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .p(px(14.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(border))
        .bg(rgba(bg))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xb0))))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                if event.click_count >= 2 {
                    app.on_agent_settings_begin_card_edit(id, event, window, cx);
                } else {
                    app.on_agent_settings_select_definition(id, event, window, cx);
                }
            }),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_start()
                .justify_between()
                .gap(px(14.0))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(5.0))
                        .child(
                            div()
                                .text_size(px(16.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba(theme.text_primary))
                                .child(definition.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(definition.runtime_profile.display_title().to_string()),
                        ),
                )
                .child(status_chip(theme, definition.enabled)),
        )
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(rgba(theme.text_secondary))
                .child(workflow_summary(&definition.workflow_description)),
        )
        .child(
            div().flex().flex_row().gap(px(10.0)).child(
                action_button(theme, "Create card", theme.accent_orange)
                    .id(("settings-agent-create-card", id as u64))
                    .on_click(cx.listener(move |app, event, window, cx| {
                        app.on_agent_settings_create_card(id, event, window, cx);
                    })),
            ),
        )
        .into_any_element()
}

fn editing_agent_card(
    theme: Theme,
    state: &AgentDefinitionsState,
    id: Option<i64>,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let draft = state.draft();
    let delete_label = if id.is_some_and(|id| state.is_delete_confirmation_pending(id)) {
        "Confirm Delete"
    } else {
        "Delete"
    };

    div()
        .id(("settings-agent-card-editing", id.unwrap_or(0) as u64))
        .flex()
        .flex_col()
        .gap(px(14.0))
        .p(px(16.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.accent_orange, 0x8c)))
        .bg(rgba(with_alpha(theme.node_fill, 0xe0)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(14.0))
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.accent_orange))
                        .child("Editing Agent"),
                )
                .child(
                    status_chip(theme, draft.enabled)
                        .id("settings-agent-toggle-enabled")
                        .on_click(
                            cx.listener(MultisoulApp::on_agent_settings_toggle_draft_enabled),
                        ),
                ),
        )
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
            "Workflow",
            window,
            cx,
        ))
        .child(
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(px(10.0))
                .child(
                    action_button(theme, "Save", theme.accent_orange)
                        .id("settings-agent-save")
                        .debug_selector(|| "settings-agent-save".to_string())
                        .on_click(cx.listener(MultisoulApp::on_agent_settings_save)),
                )
                .child(
                    action_button(theme, "Cancel", theme.accent_cyan)
                        .id("settings-agent-cancel-edit")
                        .on_click(cx.listener(MultisoulApp::on_agent_settings_cancel_card_edit)),
                )
                .when_some(id, |this, id| {
                    this.child(
                        action_button(theme, delete_label, theme.destructive)
                            .id(("settings-agent-delete-confirm", id as u64))
                            .on_click(cx.listener(move |app, event, window, cx| {
                                app.on_agent_settings_request_delete_definition(
                                    id, event, window, cx,
                                );
                            })),
                    )
                }),
        )
        .into_any_element()
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
            div()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap(px(8.0))
                .child(profile_button(
                    theme,
                    "Claude",
                    AgentProfile::Claude,
                    selected,
                    cx,
                ))
                .child(profile_button(
                    theme,
                    "Codex",
                    AgentProfile::Codex,
                    selected,
                    cx,
                ))
                .child(profile_button(
                    theme,
                    "Cursor",
                    AgentProfile::Cursor,
                    selected,
                    cx,
                ))
                .child(profile_button(
                    theme,
                    "Grok",
                    AgentProfile::Grok,
                    selected,
                    cx,
                ))
                .child(profile_button(
                    theme,
                    "OpenCode",
                    AgentProfile::OpenCode,
                    selected,
                    cx,
                )),
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

    action_button(theme, label, profile_accent(theme, selected == profile))
        .id(button_id)
        .debug_selector(move || button_id.to_string())
        .on_click(cx.listener(move |app, event, window, cx| {
            app.on_agent_settings_select_profile(profile, event, window, cx);
        }))
}

fn profile_accent(theme: Theme, selected: bool) -> u32 {
    if selected {
        theme.accent_orange
    } else {
        theme.edge_highlight
    }
}

fn workflow_summary(workflow: &str) -> String {
    let trimmed = workflow.trim();
    if trimmed.is_empty() {
        "No workflow description".to_string()
    } else if trimmed.chars().count() > 180 {
        format!("{}...", trimmed.chars().take(177).collect::<String>())
    } else {
        trimmed.to_string()
    }
}
