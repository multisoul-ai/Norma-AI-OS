use super::controls::{guide_button, is_unmodified_activation, GuideButtonKind};
use super::style::{with_alpha, FeatureGuidePalette};
use crate::app::MultisoulApp;
use crate::settings::orchestration::{health, visible_status, OrchestrationSettingsState};
use gpui::{div, prelude::*, px, rgba, Context, Div, FontWeight};
use norma::orchestration::{
    AggregateStatus, ComponentState, OrchestrationProvider, ProviderIntegration,
};
use ui::component::button::{Button, ButtonCustomVariant, ButtonVariants};
use ui::Theme;

pub(super) const GUIDED_PROVIDERS: [OrchestrationProvider; 4] = [
    OrchestrationProvider::Codex,
    OrchestrationProvider::Claude,
    OrchestrationProvider::Cursor,
    OrchestrationProvider::OpenCode,
];

pub(super) fn guide_badge_label(
    checking: bool,
    statuses: [AggregateStatus; GUIDED_PROVIDERS.len()],
    authorized_attention: bool,
) -> Option<&'static str> {
    if checking {
        return Some("Checking");
    }
    if authorized_attention {
        return Some("Needs attention");
    }
    if statuses
        .iter()
        .all(|status| *status != AggregateStatus::Ready)
    {
        return Some("Set up");
    }
    None
}

pub(super) fn guide_navigation_badge(
    state: &OrchestrationSettingsState,
    theme: Theme,
    palette: FeatureGuidePalette,
    cx: &Context<MultisoulApp>,
) -> Option<Div> {
    let statuses = GUIDED_PROVIDERS.map(|provider| state.integration(provider).aggregate_status);
    let checking = health::is_checking(cx);
    let authorized_attention = GUIDED_PROVIDERS.into_iter().any(|provider| {
        let integration = state.integration(provider);
        matches!(
            integration.authorization_component,
            ComponentState::Ready { .. }
        ) && matches!(
            visible_status(theme, state, &integration).0,
            "Needs repair" | "Install failed"
        )
    });
    let label = guide_badge_label(checking, statuses, authorized_attention)?;
    let color = if label == "Needs attention" || label == "Set up" {
        theme.accent_orange
    } else {
        palette.metadata_text
    };
    let selector = format!(
        "feature-guide-orchestration-badge-{}",
        label.to_ascii_lowercase().replace(' ', "-")
    );
    Some(
        div()
            .debug_selector(move || selector.clone())
            .flex_none()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(6.0))
            .bg(rgba(with_alpha(color, 0x18)))
            .text_size(px(9.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgba(color))
            .border_1()
            .border_color(rgba(with_alpha(theme.node_stroke, 0x58)))
            .child(label),
    )
}

pub(super) fn render_orchestration_setup(
    app: &MultisoulApp,
    compact: bool,
    theme: Theme,
    palette: FeatureGuidePalette,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let expanded = app.feature_guide.orchestration_details_expanded();
    div()
        .flex_1()
        .min_h(px(0.0))
        .flex()
        .flex_col()
        .gap(px(if compact { 8.0 } else { 11.0 }))
        .child(
            div()
                .id("feature-guide-orchestration-providers")
                .debug_selector(|| "feature-guide-orchestration-providers".to_string())
                .flex_none()
                .flex()
                .flex_col()
                .rounded(px(12.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.node_stroke, 0x78)))
                .overflow_hidden()
                .children(
                    GUIDED_PROVIDERS
                        .into_iter()
                        .enumerate()
                        .map(|(index, provider)| {
                            provider_row(
                                app.orchestration_settings.integration(provider),
                                index + 1 < GUIDED_PROVIDERS.len(),
                                compact,
                                theme,
                                &app.orchestration_settings,
                            )
                        }),
                ),
        )
        .child(
            div()
                .debug_selector(|| "feature-guide-orchestration-restart-note".to_string())
                .flex_none()
                .flex()
                .items_center()
                .gap(px(9.0))
                .px(px(12.0))
                .py(px(if compact { 8.0 } else { 10.0 }))
                .rounded(px(10.0))
                .border_1()
                .border_color(rgba(with_alpha(theme.accent_blue, 0x3c)))
                .bg(rgba(with_alpha(theme.accent_blue, 0x0c)))
                .text_size(px(11.0))
                .text_color(rgba(palette.supporting_text))
                .child(
                    div()
                        .w(px(18.0))
                        .h(px(18.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(9.0))
                        .border_1()
                        .border_color(rgba(with_alpha(theme.accent_blue, 0x90)))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba(palette.accent_text))
                        .child("i"),
                )
                .child("Restart existing agent sessions after installation."),
        )
        .when(expanded, |content| {
            content.child(
                div()
                    .debug_selector(|| {
                        "feature-guide-orchestration-components-explanation".to_string()
                    })
                    .flex_none()
                    .px(px(12.0))
                    .py(px(9.0))
                    .rounded(px(10.0))
                    .bg(rgba(with_alpha(theme.graphite, 0x0b)))
                    .text_size(px(10.5))
                    .text_color(rgba(palette.metadata_text))
                    .child("The Skill teaches delegation, Norma CLI routes work, Agent Hook reports lifecycle events, and authorization keeps access scoped to this device."),
            )
        })
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(10.0))
                .when(compact, |actions| actions.flex_col().items_start())
                .child(guide_button(
                    GuideButtonKind::OrchestrationSettings,
                    "Open Orchestration Settings",
                    theme,
                    palette,
                    cx,
                    |app, cx| app.open_feature_guide_orchestration_settings(cx),
                ))
                .child(details_button(expanded, theme, palette, cx)),
        )
}

fn provider_row(
    integration: ProviderIntegration,
    has_divider: bool,
    compact: bool,
    theme: Theme,
    state: &OrchestrationSettingsState,
) -> Div {
    let provider = integration.provider;
    let (status, status_color) = visible_status(theme, state, &integration);
    let provider_selector = format!("feature-guide-orchestration-provider-{}", provider.slug());
    let status_selector = format!(
        "feature-guide-orchestration-status-{}-{}",
        provider.slug(),
        status.to_ascii_lowercase().replace(' ', "-")
    );
    div()
        .debug_selector(move || provider_selector.clone())
        .h(px(if compact { 38.0 } else { 43.0 }))
        .flex_none()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .px(px(13.0))
        .when(has_divider, |row| {
            row.border_b_1()
                .border_color(rgba(with_alpha(theme.node_stroke, 0x58)))
        })
        .child(
            div()
                .min_w(px(0.0))
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .w(px(24.0))
                        .h(px(24.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.0))
                        .bg(rgba(with_alpha(theme.accent_blue, 0x12)))
                        .text_size(px(10.0))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgba(palette_provider_color(theme, provider)))
                        .child(provider_mark(provider)),
                )
                .child(
                    div()
                        .truncate()
                        .text_size(px(12.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba(theme.text_primary))
                        .child(provider.display_name()),
                ),
        )
        .child(
            div()
                .debug_selector(move || status_selector.clone())
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgba(status_color))
                .child(
                    div()
                        .w(px(6.0))
                        .h(px(6.0))
                        .rounded(px(3.0))
                        .bg(rgba(status_color)),
                )
                .child(status),
        )
}

fn details_button(
    expanded: bool,
    theme: Theme,
    palette: FeatureGuidePalette,
    cx: &mut Context<MultisoulApp>,
) -> Div {
    let label = if expanded {
        "Hide component details"
    } else {
        "Why are these components needed?"
    };
    let interaction = details_button_interaction(theme, palette);
    let variant = ButtonCustomVariant::new(cx)
        .color(rgba(interaction.background).into())
        .foreground(rgba(interaction.foreground).into())
        .border(rgba(0x00000000).into())
        .hover(rgba(interaction.hover).into())
        .active(rgba(interaction.active).into());
    let state_selector = if expanded {
        "feature-guide-orchestration-details-expanded"
    } else {
        "feature-guide-orchestration-details-collapsed"
    };
    div()
        .debug_selector(|| "feature-guide-orchestration-details".to_string())
        .child(
            Button::new("feature-guide-orchestration-details-button")
                .debug_selector(move || state_selector.to_string())
                .child(div().text_color(rgba(interaction.foreground)).child(label))
                .tooltip(label)
                .tab_index(interaction.tab_index)
                .custom(variant)
                .h(px(34.0))
                .px(px(10.0))
                .rounded(px(9.0))
                .on_click(cx.listener(|app, _, _, cx| {
                    cx.stop_propagation();
                    app.toggle_feature_guide_orchestration_details(cx);
                }))
                .on_key_down(cx.listener(|app, event, _, cx| {
                    if is_unmodified_activation(event) {
                        cx.stop_propagation();
                        app.toggle_feature_guide_orchestration_details(cx);
                    }
                })),
        )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct DetailsButtonInteraction {
    pub(super) background: u32,
    pub(super) foreground: u32,
    pub(super) hover: u32,
    pub(super) active: u32,
    pub(super) tab_index: isize,
}

pub(super) fn details_button_interaction(
    theme: Theme,
    palette: FeatureGuidePalette,
) -> DetailsButtonInteraction {
    DetailsButtonInteraction {
        background: 0x00000000,
        foreground: palette.accent_text,
        hover: with_alpha(theme.accent_blue, 0x10),
        active: with_alpha(theme.accent_blue, 0x20),
        tab_index: 7,
    }
}

fn provider_mark(provider: OrchestrationProvider) -> &'static str {
    match provider {
        OrchestrationProvider::Codex => "<>",
        OrchestrationProvider::Claude => "AI",
        OrchestrationProvider::Cursor => ">",
        OrchestrationProvider::OpenCode => "{}",
        OrchestrationProvider::Grok => "G",
    }
}

fn palette_provider_color(theme: Theme, provider: OrchestrationProvider) -> u32 {
    match provider {
        OrchestrationProvider::Claude => theme.accent_orange,
        OrchestrationProvider::OpenCode => theme.accent_blue,
        OrchestrationProvider::Codex | OrchestrationProvider::Cursor => theme.text_primary,
        OrchestrationProvider::Grok => theme.text_secondary,
    }
}
