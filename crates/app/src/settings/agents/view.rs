mod list;

use self::list::agent_list;
use super::components::{action_button, status_block, status_chip, with_alpha};
use super::text_input::editable_field;
use super::{AgentDefinitionField, AgentDefinitionsState};
use crate::app::MultisoulApp;
use canvas::AgentProfile;
use gpui::{div, prelude::*, px, rgba, AnyElement, Context, Div, FontWeight, ScrollHandle, Window};
use storage::AgentDefinition;
use ui::Theme;

pub(crate) fn agents_settings(
    theme: Theme,
    state: &AgentDefinitionsState,
    list_scroll: &ScrollHandle,
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
        .flex_1()
        .min_h(px(0.0))
        .gap(px(12.0))
        .pt(px(8.0))
        .child(master_detail(theme, state, list_scroll, window, cx))
        .when_some(state.status(), |this, status| {
            this.child(status_block(theme, status, status.starts_with("Could not")))
        })
}

fn master_detail(
    theme: Theme,
    state: &AgentDefinitionsState,
    list_scroll: &ScrollHandle,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    div()
        .id("settings-agents-master-detail")
        .debug_selector(|| "settings-agents-master-detail".to_string())
        .w_full()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_row()
        .overflow_hidden()
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x30)))
        .bg(rgba(with_alpha(theme.node_fill, 0xb8)))
        .child(agent_list(theme, state, list_scroll, window, cx))
        .child(agent_detail(theme, state, window, cx))
        .into_any_element()
}

fn agent_detail(
    theme: Theme,
    state: &AgentDefinitionsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let selected = state.selected_definition_id().and_then(|id| {
        state
            .definitions()
            .iter()
            .find(|definition| definition.id == id)
    });
    let content = if state.is_creating_new() || state.editing_card_id().is_some() {
        editing_agent_detail(theme, state, window, cx)
    } else if let Some(definition) = selected {
        readonly_agent_detail(theme, definition, cx)
    } else {
        empty_detail(theme)
    };

    div()
        .id("settings-agent-detail")
        .debug_selector(|| "settings-agent-detail".to_string())
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .h_full()
        .overflow_hidden()
        .rounded_r(px(10.0))
        .p(px(20.0))
        .child(content)
        .into_any_element()
}

fn readonly_agent_detail(
    theme: Theme,
    definition: &AgentDefinition,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let id = definition.id;

    div()
        .id("settings-agent-detail-readonly")
        .debug_selector(|| "settings-agent-detail-readonly".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(24.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(agent_identity(
                    theme,
                    &definition.name,
                    definition.runtime_profile,
                ))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .child(status_chip(theme, definition.enabled))
                        .child(
                            action_button(theme, "Edit", theme.text_secondary)
                                .id("settings-agent-edit")
                                .debug_selector(|| "settings-agent-edit".to_string())
                                .on_click(cx.listener(move |app, event, window, cx| {
                                    app.on_agent_settings_begin_detail_edit(id, event, window, cx);
                                })),
                        )
                        .child(
                            action_button(theme, "Add to Canvas", theme.accent_orange)
                                .id("settings-agent-add-to-canvas")
                                .debug_selector(|| "settings-agent-add-to-canvas".to_string())
                                .on_click(cx.listener(move |app, event, window, cx| {
                                    app.on_agent_settings_create_card(id, event, window, cx);
                                })),
                        ),
                ),
        )
        .child(
            div()
                .w_full()
                .rounded(px(8.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x34)))
                .bg(rgba(with_alpha(theme.panel_high, 0x54)))
                .child(summary_row(
                    theme,
                    "Runtime Profile",
                    definition.runtime_profile.display_title(),
                    false,
                ))
                .child(summary_row(
                    theme,
                    "Workflow",
                    &workflow_summary(&definition.workflow_description),
                    true,
                )),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgba(theme.text_tertiary))
                .child("Select Edit to change this agent."),
        )
        .into_any_element()
}

fn agent_identity(theme: Theme, name: &str, profile: AgentProfile) -> Div {
    div()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(
            div()
                .text_size(px(18.0))
                .font_weight(FontWeight::BOLD)
                .text_color(rgba(theme.text_primary))
                .child(name.to_string()),
        )
        .child(
            div()
                .text_size(px(13.0))
                .text_color(rgba(theme.text_tertiary))
                .child(format!("{} Agent", profile.display_title())),
        )
}

fn summary_row(theme: Theme, label: &'static str, value: &str, separated: bool) -> Div {
    div()
        .min_h(px(58.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .px(px(14.0))
        .py(px(10.0))
        .when(separated, |this| {
            this.border_t_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x28)))
        })
        .child(
            div()
                .flex_none()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(theme.text_secondary))
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_size(px(13.0))
                .text_color(rgba(theme.text_tertiary))
                .child(value.to_string()),
        )
}

fn editing_agent_detail(
    theme: Theme,
    state: &AgentDefinitionsState,
    window: &mut Window,
    cx: &mut Context<MultisoulApp>,
) -> AnyElement {
    let draft = state.draft();
    let id = draft.id;
    let delete_label = if id.is_some_and(|id| state.is_delete_confirmation_pending(id)) {
        "Confirm Delete"
    } else {
        "Delete"
    };

    div()
        .id("settings-agent-detail-editing")
        .debug_selector(|| "settings-agent-detail-editing".to_string())
        .w_full()
        .flex()
        .flex_col()
        .gap(px(16.0))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(agent_identity(theme, &draft.name, draft.runtime_profile))
                .child(
                    status_chip(theme, draft.enabled)
                        .cursor_pointer()
                        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xd2))))
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
                .justify_between()
                .gap(px(10.0))
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
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            action_button(theme, "Cancel", theme.text_secondary)
                                .id("settings-agent-cancel-edit")
                                .debug_selector(|| "settings-agent-cancel-edit".to_string())
                                .on_click(
                                    cx.listener(MultisoulApp::on_agent_settings_cancel_card_edit),
                                ),
                        )
                        .child(
                            action_button(theme, "Save", theme.accent_orange)
                                .id("settings-agent-save")
                                .debug_selector(|| "settings-agent-save".to_string())
                                .on_click(cx.listener(MultisoulApp::on_agent_settings_save)),
                        ),
                ),
        )
        .into_any_element()
}

fn empty_detail(theme: Theme) -> AnyElement {
    div()
        .w_full()
        .min_h(px(300.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(13.0))
        .text_color(rgba(theme.text_tertiary))
        .child("Create an Agent to configure its runtime and workflow.")
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
        "No workflow instructions".to_string()
    } else if trimmed.chars().count() > 180 {
        format!("{}...", trimmed.chars().take(177).collect::<String>())
    } else {
        trimmed.to_string()
    }
}
