use crate::settings::typography::{control_label, SettingsText, TextRole};
mod status;
use status::{component_error_detail, component_status, delegation_notice, provider_description};
mod controller;
pub(crate) mod health;
pub(crate) mod path_convention;
mod tokens;

#[cfg(feature = "e2e-test-support")]
pub(crate) use controller::canonical_preview_integrations;
use controller::OrchestrationOperation;
pub(crate) use controller::OrchestrationSettingsState;
use tokens::*;

use crate::app::MultisoulApp;
use gpui::{div, prelude::*, px, rgba, Context, Div, Window};
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
                .settings_text(TextRole::Helper, theme)
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
                        .settings_font(TextRole::Caption)
                        .child("i"),
                )
                .child(div().relative().top(px(5.0)).child(ui::i18n::text(
                    "settings.restart_existing_agent_sessions_after_installation",
                ))),
        )
        .child(path_convention::path_convention_row(
            theme,
            state.path_convention_enabled(),
            cx,
        ))
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
        ui::i18n::text("settings.only_norma_managed_files_will_be_removed_click_confirm")
            .to_string()
    } else if let Some((_, error)) = state
        .operation_error
        .as_ref()
        .filter(|(candidate, _)| *candidate == provider)
    {
        error.to_string()
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
                        .settings_text(TextRole::RowTitle, theme)
                        .when(provider == OrchestrationProvider::Cursor, |title| {
                            title.relative().top(px(1.0))
                        })
                        .child(provider.display_name()),
                )
                .when_some(delegation_notice(provider), |column, notice| {
                    column.child(
                        div()
                            .truncate()
                            .settings_font(TextRole::Helper)
                            .text_color(rgba(warning_status_color()))
                            .child(notice),
                    )
                })
                .child(
                    div()
                        .truncate()
                        .settings_text(TextRole::Helper, theme)
                        .child(description),
                ),
        )
        .child(
            div()
                .w(px(PROVIDER_STATUS_WIDTH))
                .flex_none()
                .relative()
                .left(px(2.0))
                .settings_font(TextRole::Label)
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
                Some((_, OrchestrationOperation::Uninstall)) => {
                    ui::i18n::text("settings.uninstalling")
                }
                _ => ui::i18n::text("settings.installing"),
            },
            theme.text_tertiary,
            false,
        )
    } else if pending_confirmation {
        (ui::i18n::text("common.confirm"), theme.destructive, true)
    } else {
        match integration.aggregate_status {
            AggregateStatus::Ready => (
                ui::i18n::text("common.uninstall"),
                theme.text_secondary,
                true,
            ),
            AggregateStatus::NotInstalled => {
                (ui::i18n::text("common.install"), theme.accent_blue, true)
            }
            AggregateStatus::Unsupported => (
                ui::i18n::text("settings.unavailable"),
                theme.text_tertiary,
                false,
            ),
            AggregateStatus::NeedsRepair | AggregateStatus::InstallFailed => {
                (ui::i18n::text("settings.repair"), theme.accent_blue, true)
            }
            AggregateStatus::Installing | AggregateStatus::Uninstalling => (
                ui::i18n::text("settings.installing"),
                theme.text_tertiary,
                false,
            ),
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
        .child(control_label(label))
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
                .settings_font(TextRole::Control)
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
        ui::i18n::format(
            "settings.collapse_component_details",
            &[("arg0", (provider.display_name()).to_string())],
        )
    } else {
        ui::i18n::format(
            "settings.expand_component_details",
            &[("arg0", (provider.display_name()).to_string())],
        )
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
            .child(control_label(label))
            .tooltip(tooltip)
            .custom(variant)
            .w(px(18.0))
            .h(px(32.0))
            .p_0()
            .border_0()
            .settings_font(TextRole::Control)
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
                ui::i18n::text("settings.authorization"),
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
                    .settings_font(TextRole::Helper)
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
        .settings_text(TextRole::Helper, theme)
        .child(div().w(px(COMPONENT_LABEL_WIDTH)).flex_none().child(title))
        .child(
            div()
                .settings_font(TextRole::Label)
                .text_color(rgba(color))
                .child(label),
        )
}

pub(crate) fn visible_status(
    theme: Theme,
    state: &OrchestrationSettingsState,
    integration: &ProviderIntegration,
) -> (&'static str, u32) {
    if state
        .operation_error
        .as_ref()
        .is_some_and(|(provider, _)| *provider == integration.provider)
    {
        return (ui::i18n::text("settings.install_failed"), theme.destructive);
    }
    if let Some((provider, operation)) = state.busy {
        if provider == integration.provider {
            return match operation {
                OrchestrationOperation::InstallOrRepair => {
                    (ui::i18n::text("settings.installing"), theme.text_tertiary)
                }
                OrchestrationOperation::Uninstall => {
                    (ui::i18n::text("settings.uninstalling"), theme.text_tertiary)
                }
            };
        }
    }
    match integration.aggregate_status {
        AggregateStatus::Ready => (ui::i18n::text("common.ready"), theme.accent_green),
        AggregateStatus::NotInstalled => (
            ui::i18n::text("settings.not_installed"),
            theme.text_secondary,
        ),
        AggregateStatus::NeedsRepair => (
            ui::i18n::text("settings.needs_repair"),
            warning_status_color(),
        ),
        AggregateStatus::InstallFailed => {
            (ui::i18n::text("settings.install_failed"), theme.destructive)
        }
        AggregateStatus::Unsupported => (
            ui::i18n::text("settings.not_supported"),
            theme.text_secondary,
        ),
        AggregateStatus::Installing => (ui::i18n::text("settings.installing"), theme.text_tertiary),
        AggregateStatus::Uninstalling => {
            (ui::i18n::text("settings.uninstalling"), theme.text_tertiary)
        }
    }
}

fn provider_index(provider: OrchestrationProvider) -> usize {
    match provider {
        OrchestrationProvider::Codex => 0,
        OrchestrationProvider::Claude => 1,
        OrchestrationProvider::Cursor => 2,
        OrchestrationProvider::OpenCode => 3,
        OrchestrationProvider::InfCode => 4,
        OrchestrationProvider::Grok => 5,
        OrchestrationProvider::Kimi => 6,
        OrchestrationProvider::ZCode => 7,
        OrchestrationProvider::Pi => 8,
        OrchestrationProvider::Hermes => 9,
    }
}

fn action_selector(provider: OrchestrationProvider) -> &'static str {
    match provider {
        OrchestrationProvider::Codex => "settings-orchestration-codex-action",
        OrchestrationProvider::Claude => "settings-orchestration-claude-action",
        OrchestrationProvider::Cursor => "settings-orchestration-cursor-action",
        OrchestrationProvider::OpenCode => "settings-orchestration-opencode-action",
        OrchestrationProvider::InfCode => "settings-orchestration-infcode-action",
        OrchestrationProvider::Grok => "settings-orchestration-grok-action",
        OrchestrationProvider::Kimi => "settings-orchestration-kimi-action",
        OrchestrationProvider::ZCode => "settings-orchestration-zcode-action",
        OrchestrationProvider::Pi => "settings-orchestration-pi-action",
        OrchestrationProvider::Hermes => "settings-orchestration-hermes-action",
    }
}

fn disclosure_selector(provider: OrchestrationProvider) -> &'static str {
    match provider {
        OrchestrationProvider::Codex => "settings-orchestration-codex-disclosure",
        OrchestrationProvider::Claude => "settings-orchestration-claude-disclosure",
        OrchestrationProvider::Cursor => "settings-orchestration-cursor-disclosure",
        OrchestrationProvider::OpenCode => "settings-orchestration-opencode-disclosure",
        OrchestrationProvider::InfCode => "settings-orchestration-infcode-disclosure",
        OrchestrationProvider::Grok => "settings-orchestration-grok-disclosure",
        OrchestrationProvider::Kimi => "settings-orchestration-kimi-disclosure",
        OrchestrationProvider::ZCode => "settings-orchestration-zcode-disclosure",
        OrchestrationProvider::Pi => "settings-orchestration-pi-disclosure",
        OrchestrationProvider::Hermes => "settings-orchestration-hermes-disclosure",
    }
}

fn with_alpha(color: u32, alpha: u8) -> u32 {
    (color & 0xffffff00) | u32::from(alpha)
}

#[cfg(test)]
#[path = "../../tests/unit/orchestration_settings_layout_tests.rs"]
mod orchestration_settings_layout_tests;

#[cfg(test)]
#[path = "../../tests/unit/orchestration_new_provider_tests.rs"]
mod orchestration_new_provider_tests;
