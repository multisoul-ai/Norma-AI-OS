use super::{AgentDefinitionField, AgentDefinitionsState};
use crate::app::MultisoulApp;
use canvas::AgentProfile;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight};
use ui::Theme;

pub(crate) fn agents_settings(
    theme: Theme,
    state: &AgentDefinitionsState,
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
        .gap(px(20.0))
        .pt(px(8.0))
        .child(form_section(theme, state, cx))
        .child(list_section(theme, state, cx))
        .when_some(state.status(), |this, status| {
            this.child(status_block(theme, status, status.starts_with("Could not")))
        })
}

fn form_section(
    theme: Theme,
    state: &AgentDefinitionsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let draft = state.draft();

    section(
        theme,
        "Draft Agent",
        "Prepare a project agent profile before persistence wiring.",
    )
    .child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(readonly_field(
                theme,
                AgentDefinitionField::Name,
                "Name",
                draft_name_display(draft.name.as_str()),
            ))
            .child(
                action_button(theme, "Reset", theme.accent_cyan)
                    .id("settings-agent-reset")
                    .on_click(cx.listener(MultisoulApp::on_agent_settings_reset_draft)),
            ),
    )
    .child(profile_buttons(theme, draft.runtime_profile, cx))
    .child(readonly_field(
        theme,
        AgentDefinitionField::WorkflowDescription,
        "Workflow",
        workflow_display(draft.workflow_description.as_str()),
    ))
    .child(
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(
                toggle_button(theme, draft.enabled, "Enabled")
                    .id("settings-agent-toggle-enabled")
                    .on_click(cx.listener(MultisoulApp::on_agent_settings_toggle_draft_enabled)),
            )
            .child(
                action_button(theme, "Save Agent", theme.accent_orange)
                    .id("settings-agent-save")
                    .on_click(cx.listener(MultisoulApp::on_agent_settings_save)),
            ),
    )
}

fn list_section(
    theme: Theme,
    state: &AgentDefinitionsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let definitions = state.definitions();

    section(
        theme,
        "Project Agents",
        "Existing definitions can be loaded into the draft for review.",
    )
    .child(
        div()
            .id("settings-agents-list")
            .flex()
            .flex_col()
            .gap(px(10.0))
            .when(definitions.is_empty(), |this| {
                this.child(empty_list_message(theme, "No project agents saved yet."))
            })
            .children(definitions.iter().map(|definition| {
                definition_row(
                    theme,
                    definition.id,
                    definition.name.clone(),
                    definition.runtime_profile.display_title().to_string(),
                    definition.workflow_description.clone(),
                    definition.enabled,
                    cx,
                )
            })),
    )
}

fn definition_row(
    theme: Theme,
    id: i64,
    name: String,
    runtime_profile: String,
    workflow_description: String,
    enabled: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(14.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x32)))
        .bg(rgba(with_alpha(theme.node_fill, 0xd8)))
        .child(
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
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba(theme.text_primary))
                                .child(name),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(rgba(theme.text_secondary))
                                .child(runtime_profile),
                        ),
                )
                .child(status_chip(theme, enabled)),
        )
        .child(
            div()
                .text_size(px(13.0))
                .line_height(px(20.0))
                .text_color(rgba(theme.text_secondary))
                .child(workflow_display(workflow_description.as_str())),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(10.0))
                .child(
                    action_button(theme, "Edit draft", theme.accent_cyan)
                        .id(("settings-agent-edit", id as u64))
                        .on_click(cx.listener(move |app, event, window, cx| {
                            app.on_agent_settings_edit_definition(id, event, window, cx);
                        })),
                )
                .child(
                    action_button(theme, "Delete", theme.destructive)
                        .id(("settings-agent-delete", id as u64))
                        .on_click(cx.listener(move |app, event, window, cx| {
                            app.on_agent_settings_delete_definition(id, event, window, cx);
                        })),
                ),
        )
}

fn section(theme: Theme, title: &'static str, hint: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(14.0))
        .pt(px(18.0))
        .border_t_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(theme.accent_cyan))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .text_color(rgba(theme.text_tertiary))
                        .child(hint),
                ),
        )
}

fn readonly_field(
    theme: Theme,
    field: AgentDefinitionField,
    label: &'static str,
    value: String,
) -> Div {
    let (id, min_height) = match field {
        AgentDefinitionField::Name => ("settings-agent-name-input", 40.0),
        AgentDefinitionField::WorkflowDescription => ("settings-agent-workflow-input", 92.0),
    };

    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(13.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(theme.text_secondary))
                .child(label),
        )
        .child(
            div()
                .id(id)
                .w_full()
                .max_w(px(540.0))
                .min_h(px(min_height))
                .p(px(12.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x40)))
                .bg(rgba(with_alpha(theme.panel_high, 0xb8)))
                .text_color(rgba(theme.text_primary))
                .line_height(px(20.0))
                .child(value),
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
            div()
                .flex()
                .flex_row()
                .gap(px(8.0))
                .child(
                    action_button(
                        theme,
                        "Claude",
                        profile_accent(theme, selected == AgentProfile::Claude),
                    )
                    .id("settings-agent-profile-claude")
                    .on_click(cx.listener(move |app, event, window, cx| {
                        app.on_agent_settings_select_profile(
                            AgentProfile::Claude,
                            event,
                            window,
                            cx,
                        );
                    })),
                )
                .child(
                    action_button(
                        theme,
                        "Codex",
                        profile_accent(theme, selected == AgentProfile::Codex),
                    )
                    .id("settings-agent-profile-codex")
                    .on_click(cx.listener(move |app, event, window, cx| {
                        app.on_agent_settings_select_profile(
                            AgentProfile::Codex,
                            event,
                            window,
                            cx,
                        );
                    })),
                )
                .child(
                    action_button(
                        theme,
                        "OpenCode",
                        profile_accent(theme, selected == AgentProfile::OpenCode),
                    )
                    .id("settings-agent-profile-opencode")
                    .on_click(cx.listener(move |app, event, window, cx| {
                        app.on_agent_settings_select_profile(
                            AgentProfile::OpenCode,
                            event,
                            window,
                            cx,
                        );
                    })),
                ),
        )
}

fn profile_accent(theme: Theme, selected: bool) -> u32 {
    if selected {
        theme.accent_orange
    } else {
        theme.edge_highlight
    }
}

fn toggle_button(theme: Theme, enabled: bool, label: &'static str) -> Div {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(
            if enabled {
                theme.accent_cyan
            } else {
                theme.edge_highlight
            },
            0x70,
        )))
        .bg(rgba(with_alpha(
            if enabled {
                theme.accent_cyan
            } else {
                theme.panel_high
            },
            if enabled { 0x18 } else { 0xb8 },
        )))
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(theme.text_primary))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(theme.panel_high, 0xd2))))
        .child(format!("{label}: {}", if enabled { "On" } else { "Off" }))
}

fn action_button(_theme: Theme, label: &'static str, accent: u32) -> Div {
    div()
        .px(px(12.0))
        .py(px(8.0))
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(accent, 0x70)))
        .bg(rgba(with_alpha(accent, 0x18)))
        .text_size(px(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(accent))
        .cursor_pointer()
        .hover(move |style| style.bg(rgba(with_alpha(accent, 0x28))))
        .child(label)
}

fn status_chip(theme: Theme, enabled: bool) -> Div {
    div()
        .px(px(10.0))
        .py(px(5.0))
        .rounded(px(999.0))
        .bg(rgba(with_alpha(
            if enabled {
                theme.accent_cyan
            } else {
                theme.destructive
            },
            0x18,
        )))
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgba(if enabled {
            theme.accent_cyan
        } else {
            theme.destructive
        }))
        .child(if enabled { "Enabled" } else { "Disabled" })
}

fn status_block(theme: Theme, message: &str, is_error: bool) -> Div {
    div()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(
            if is_error {
                theme.destructive
            } else {
                theme.accent_cyan
            },
            0x40,
        )))
        .bg(rgba(with_alpha(
            if is_error {
                theme.destructive
            } else {
                theme.panel_high
            },
            0x16,
        )))
        .px(px(14.0))
        .py(px(12.0))
        .text_size(px(13.0))
        .line_height(px(20.0))
        .text_color(rgba(if is_error {
            theme.destructive
        } else {
            theme.text_secondary
        }))
        .child(message.to_string())
}

fn empty_list_message(theme: Theme, message: &'static str) -> Div {
    div()
        .rounded(px(8.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .bg(rgba(with_alpha(theme.panel_high, 0x60)))
        .px(px(14.0))
        .py(px(12.0))
        .text_size(px(13.0))
        .text_color(rgba(theme.text_tertiary))
        .child(message)
}

fn draft_name_display(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "Set in Task 4 persistence/editor wiring.".to_string()
    } else {
        trimmed.to_string()
    }
}

fn workflow_display(workflow: &str) -> String {
    let trimmed = workflow.trim();
    if trimmed.is_empty() {
        "Workflow description will appear here once the draft is populated.".to_string()
    } else {
        trimmed.to_string()
    }
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}
