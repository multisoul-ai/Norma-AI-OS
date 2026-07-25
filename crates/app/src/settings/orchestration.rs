mod controller;
mod tokens;

use controller::OrchestrationOperation;
pub(crate) use controller::OrchestrationSettingsState;
use tokens::*;

use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight, Window};
use norma::orchestration::{
    AggregateStatus, ComponentState, OrchestrationProvider, ProviderIntegration,
};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

pub(super) fn orchestration_settings(
    theme: Theme,
    state: &OrchestrationSettingsState,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .child(
            div()
                .h(px(NOTICE_HEIGHT))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(10.0))
                .border_b_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
                .text_size(px(NOTICE_TEXT_SIZE))
                .text_color(rgba(theme.text_tertiary))
                .child(
                    div()
                        .w(px(18.0))
                        .h(px(18.0))
                        .relative()
                        .top(px(5.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(9.0))
                        .border_1()
                        .border_color(rgba(theme.text_tertiary))
                        .text_size(px(11.0))
                        .child("i"),
                )
                .child(
                    div()
                        .relative()
                        .top(px(5.0))
                        .child("Restart existing agent sessions after installation."),
                ),
        )
        .children(
            OrchestrationProvider::ALL
                .into_iter()
                .map(|provider| provider_section(theme, state, provider, cx)),
        )
}

fn provider_section(
    theme: Theme,
    state: &OrchestrationSettingsState,
    provider: OrchestrationProvider,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let integration = state.integration(provider);
    let expanded = state.expanded_provider == Some(provider);
    div()
        .mr(px(PROVIDER_SECTION_RIGHT_INSET))
        .flex()
        .flex_col()
        .border_b_1()
        .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        .child(provider_row(theme, state, &integration, expanded, cx))
        .when(expanded, |section| {
            section.child(component_details(theme, &integration))
        })
}

fn provider_row(
    theme: Theme,
    state: &OrchestrationSettingsState,
    integration: &ProviderIntegration,
    expanded: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let provider = integration.provider;
    let (status, status_color) = visible_status(theme, state, integration);
    let pending_confirmation = state.pending_uninstall == Some(provider);
    let description = if pending_confirmation {
        "Only Norma-managed files will be removed. Click Confirm.".to_string()
    } else if let Some((_, error)) = state
        .operation_error
        .as_ref()
        .filter(|(candidate, _)| *candidate == provider)
    {
        error.clone()
    } else {
        provider_description(provider, integration.aggregate_status).to_string()
    };
    div()
        .min_h(px(provider_row_min_height(provider, expanded)))
        .mr(px(-PROVIDER_ROW_RIGHT_OUTDENT))
        .flex()
        .items_center()
        .gap(px(PROVIDER_ROW_GAP))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(PROVIDER_TEXT_GAP))
                .child(
                    div()
                        .text_size(px(PROVIDER_TITLE_SIZE))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba(theme.text_primary))
                        .when(provider == OrchestrationProvider::Cursor, |title| {
                            title.relative().top(px(1.0))
                        })
                        .child(provider.display_name()),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(PROVIDER_BODY_SIZE))
                        .text_color(rgba(theme.text_tertiary))
                        .child(description),
                ),
        )
        .child(
            div()
                .w(px(PROVIDER_STATUS_WIDTH))
                .flex_none()
                .relative()
                .left(px(2.0))
                .font_weight(FontWeight::MEDIUM)
                .text_size(px(PROVIDER_STATUS_SIZE))
                .text_color(rgba(status_color))
                .child(status),
        )
        .child(provider_action(theme, state, integration, cx))
        .child(disclosure(theme, provider, expanded, cx))
}

fn provider_action(
    theme: Theme,
    state: &OrchestrationSettingsState,
    integration: &ProviderIntegration,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let provider = integration.provider;
    let busy = state
        .busy
        .is_some_and(|(candidate, _)| candidate == provider);
    let pending_confirmation = state.pending_uninstall == Some(provider);
    let (label, tone, enabled) = if busy {
        (
            match state.busy {
                Some((_, OrchestrationOperation::Uninstall)) => "Uninstalling",
                _ => "Installing",
            },
            theme.text_tertiary,
            false,
        )
    } else if pending_confirmation {
        ("Confirm", theme.destructive, true)
    } else {
        match integration.aggregate_status {
            AggregateStatus::Ready => ("Uninstall", theme.text_secondary, true),
            AggregateStatus::NotInstalled => ("Install", theme.accent_blue, true),
            AggregateStatus::Unsupported => ("Unavailable", theme.text_tertiary, false),
            AggregateStatus::NeedsRepair | AggregateStatus::InstallFailed => {
                ("Repair", theme.accent_blue, true)
            }
            AggregateStatus::Installing | AggregateStatus::Uninstalling => {
                ("Installing", theme.text_tertiary, false)
            }
        }
    };
    let selector = action_selector(provider);
    if !enabled {
        return disabled_action(theme, provider, selector, label);
    }
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(with_alpha(tone, 0x0c)).into())
        .foreground(rgba(tone).into())
        .border(rgba(with_alpha(tone, 0x78)).into())
        .hover(rgba(with_alpha(tone, 0x1c)).into())
        .active(rgba(with_alpha(tone, 0x2c)).into());
    let button = Button::new(("settings-orchestration-action", provider_index(provider)))
        .debug_selector(move || selector.to_string())
        .label(label)
        .custom(variant)
        .w_full()
        .h(px(40.0))
        .on_click(cx.listener(move |app, _, _window: &mut Window, cx| {
            app.activate_orchestration_provider(provider, cx);
        }));
    div()
        .w(px(PROVIDER_ACTION_WIDTH))
        .flex_none()
        .flex()
        .justify_end()
        .child(button)
}

fn disabled_action(
    theme: Theme,
    provider: OrchestrationProvider,
    selector: &'static str,
    label: &'static str,
) -> Div {
    div()
        .w(px(PROVIDER_ACTION_WIDTH))
        .h(px(40.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .id(("settings-orchestration-action", provider_index(provider)))
                .debug_selector(move || selector.to_string())
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.text_tertiary, 0x42)))
                .text_size(px(PROVIDER_STATUS_SIZE))
                .text_color(rgba(theme.text_tertiary))
                .child(label),
        )
}

fn disclosure(
    theme: Theme,
    provider: OrchestrationProvider,
    expanded: bool,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let selector = disclosure_selector(provider);
    let label = if expanded { "⌃" } else { "⌄" };
    let tooltip = if expanded {
        format!("Collapse {} component details", provider.display_name())
    } else {
        format!("Expand {} component details", provider.display_name())
    };
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(0x00000000).into())
        .foreground(rgba(theme.text_secondary).into())
        .border(rgba(0x00000000).into())
        .hover(rgba(with_alpha(theme.text_secondary, 0x10)).into())
        .active(rgba(with_alpha(theme.text_secondary, 0x20)).into());
    div()
        .w(px(18.0))
        .h(px(32.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(
            Button::new((
                "settings-orchestration-disclosure",
                provider_index(provider),
            ))
            .debug_selector(move || selector.to_string())
            .label(label)
            .tooltip(tooltip)
            .custom(variant)
            .w(px(18.0))
            .h(px(32.0))
            .p_0()
            .border_0()
            .text_size(px(15.0))
            .on_click(cx.listener(move |app, _, _window: &mut Window, cx| {
                app.orchestration_settings.expanded_provider =
                    if app.orchestration_settings.expanded_provider == Some(provider) {
                        None
                    } else {
                        Some(provider)
                    };
                cx.notify();
            })),
        )
}

fn component_details(theme: Theme, integration: &ProviderIntegration) -> Div {
    let detail = component_error_detail(integration);
    div()
        .w_full()
        .mb(px(COMPONENT_DETAILS_BOTTOM_MARGIN))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgba(with_alpha(theme.node_stroke, 0x78)))
        .bg(rgba(with_alpha(theme.panel_high, 0x44)))
        .children([
            component_row(theme, "Skill", &integration.skill_component, false),
            component_row(theme, "Norma CLI", &integration.cli_component, true),
            component_row(theme, "Agent Hook", &integration.hook_component, true),
            component_row(
                theme,
                "Authorization",
                &integration.authorization_component,
                true,
            ),
        ])
        .when_some(detail, |details, message| {
            details.child(
                div()
                    .border_t_1()
                    .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
                    .px(px(18.0))
                    .py(px(12.0))
                    .text_size(px(12.0))
                    .text_color(rgba(theme.destructive))
                    .child(message),
            )
        })
}

fn component_row(
    theme: Theme,
    title: &'static str,
    component: &ComponentState,
    border: bool,
) -> Div {
    let (label, color) = component_status(theme, component);
    div()
        .h(px(COMPONENT_ROW_HEIGHT))
        .flex()
        .items_center()
        .pl(px(20.0))
        .pr(px(18.0))
        .when(border, |row| {
            row.border_t_1()
                .border_color(rgba(with_alpha(theme.edge_highlight, 0x24)))
        })
        .text_size(px(COMPONENT_TEXT_SIZE))
        .text_color(rgba(theme.text_secondary))
        .child(div().w(px(COMPONENT_LABEL_WIDTH)).flex_none().child(title))
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(color))
                .child(label),
        )
}

fn provider_description(provider: OrchestrationProvider, status: AggregateStatus) -> &'static str {
    match (provider, status) {
        (OrchestrationProvider::Claude, AggregateStatus::NotInstalled) => {
            "Install Skill, CLI, hooks, and authorization"
        }
        (OrchestrationProvider::Cursor, AggregateStatus::NeedsRepair)
        | (_, AggregateStatus::InstallFailed) => "One component needs attention",
        (OrchestrationProvider::Grok, _) => "Orchestration is not supported in the first release",
        _ => "Delegate tasks through Norma OS",
    }
}

fn visible_status(
    theme: Theme,
    state: &OrchestrationSettingsState,
    integration: &ProviderIntegration,
) -> (&'static str, u32) {
    if state
        .operation_error
        .as_ref()
        .is_some_and(|(provider, _)| *provider == integration.provider)
    {
        return ("Install failed", theme.destructive);
    }
    if let Some((provider, operation)) = state.busy {
        if provider == integration.provider {
            return match operation {
                OrchestrationOperation::InstallOrRepair => ("Installing", theme.text_tertiary),
                OrchestrationOperation::Uninstall => ("Uninstalling", theme.text_tertiary),
            };
        }
    }
    match integration.aggregate_status {
        AggregateStatus::Ready => ("Ready", theme.accent_green),
        AggregateStatus::NotInstalled => ("Not installed", theme.text_secondary),
        AggregateStatus::NeedsRepair => ("Needs repair", warning_status_color()),
        AggregateStatus::InstallFailed => ("Install failed", theme.destructive),
        AggregateStatus::Unsupported => ("Not supported", theme.text_secondary),
        AggregateStatus::Installing => ("Installing", theme.text_tertiary),
        AggregateStatus::Uninstalling => ("Uninstalling", theme.text_tertiary),
    }
}

fn component_status(theme: Theme, component: &ComponentState) -> (&'static str, u32) {
    match component {
        ComponentState::Ready { .. } => ("Ready", theme.accent_green),
        ComponentState::Missing => ("Needs repair", warning_status_color()),
        ComponentState::Outdated { .. } => ("Outdated", warning_status_color()),
        ComponentState::Conflict { .. } => ("Conflict", theme.destructive),
        ComponentState::Failed { .. } => ("Failed", theme.destructive),
        ComponentState::Installing => ("Installing", theme.text_tertiary),
        ComponentState::Unsupported { .. } => ("Unavailable", theme.text_tertiary),
    }
}

fn component_error_detail(integration: &ProviderIntegration) -> Option<String> {
    [
        ("Skill", &integration.skill_component),
        ("Norma CLI", &integration.cli_component),
        ("Agent Hook", &integration.hook_component),
        ("Authorization", &integration.authorization_component),
    ]
    .into_iter()
    .find_map(|(label, component)| match component {
        ComponentState::Conflict { path, reason } => Some(format!(
            "{label}: {reason} Resolve the conflict at {} and choose Repair.",
            path.display()
        )),
        ComponentState::Failed { message, .. } => {
            Some(format!("{label}: {message} Choose Repair to try again."))
        }
        _ => None,
    })
}

fn provider_index(provider: OrchestrationProvider) -> usize {
    match provider {
        OrchestrationProvider::Codex => 0,
        OrchestrationProvider::Claude => 1,
        OrchestrationProvider::Cursor => 2,
        OrchestrationProvider::OpenCode => 3,
        OrchestrationProvider::Grok => 4,
    }
}

fn action_selector(provider: OrchestrationProvider) -> &'static str {
    match provider {
        OrchestrationProvider::Codex => "settings-orchestration-codex-action",
        OrchestrationProvider::Claude => "settings-orchestration-claude-action",
        OrchestrationProvider::Cursor => "settings-orchestration-cursor-action",
        OrchestrationProvider::OpenCode => "settings-orchestration-opencode-action",
        OrchestrationProvider::Grok => "settings-orchestration-grok-action",
    }
}

fn disclosure_selector(provider: OrchestrationProvider) -> &'static str {
    match provider {
        OrchestrationProvider::Codex => "settings-orchestration-codex-disclosure",
        OrchestrationProvider::Claude => "settings-orchestration-claude-disclosure",
        OrchestrationProvider::Cursor => "settings-orchestration-cursor-disclosure",
        OrchestrationProvider::OpenCode => "settings-orchestration-opencode-disclosure",
        OrchestrationProvider::Grok => "settings-orchestration-grok-disclosure",
    }
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../tests/unit/orchestration_settings_layout_tests.rs"]
mod orchestration_settings_layout_tests;
