use crate::app::MultisoulApp;
use gpui::Context;
use norma::orchestration::{AggregateStatus, ComponentState, OrchestrationProvider};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationProviderE2eSnapshot {
    pub provider: &'static str,
    pub status: &'static str,
    pub action: &'static str,
    pub skill: &'static str,
    pub cli: &'static str,
    pub hook: &'static str,
    pub authorization: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationSettingsE2eSnapshot {
    pub providers: Vec<OrchestrationProviderE2eSnapshot>,
    pub expanded_provider: Option<&'static str>,
    pub pending_uninstall: Option<&'static str>,
}

impl MultisoulApp {
    pub fn use_canonical_orchestration_preview_for_e2e(&mut self, cx: &mut Context<Self>) {
        self.orchestration_settings.use_canonical_preview();
        cx.notify();
    }

    pub fn orchestration_settings_e2e_snapshot(&self) -> OrchestrationSettingsE2eSnapshot {
        let pending = self.orchestration_settings.pending_uninstall_for_e2e();
        OrchestrationSettingsE2eSnapshot {
            providers: OrchestrationProvider::ALL
                .into_iter()
                .map(|provider| {
                    let integration = self.orchestration_settings.integration(provider);
                    OrchestrationProviderE2eSnapshot {
                        provider: provider.display_name(),
                        status: aggregate_status(integration.aggregate_status),
                        action: if pending == Some(provider) {
                            "Confirm"
                        } else {
                            aggregate_action(integration.aggregate_status)
                        },
                        skill: component_status(&integration.skill_component),
                        cli: component_status(&integration.cli_component),
                        hook: component_status(&integration.hook_component),
                        authorization: component_status(&integration.authorization_component),
                    }
                })
                .collect(),
            expanded_provider: self
                .orchestration_settings
                .expanded_provider_for_e2e()
                .map(OrchestrationProvider::display_name),
            pending_uninstall: pending.map(OrchestrationProvider::display_name),
        }
    }
}

fn aggregate_status(status: AggregateStatus) -> &'static str {
    match status {
        AggregateStatus::NotInstalled => "Not installed",
        AggregateStatus::Installing => "Installing",
        AggregateStatus::Ready => "Ready",
        AggregateStatus::NeedsRepair => "Needs repair",
        AggregateStatus::InstallFailed => "Install failed",
        AggregateStatus::Uninstalling => "Uninstalling",
        AggregateStatus::Unsupported => "Not supported",
    }
}

fn aggregate_action(status: AggregateStatus) -> &'static str {
    match status {
        AggregateStatus::NotInstalled => "Install",
        AggregateStatus::Ready => "Uninstall",
        AggregateStatus::NeedsRepair | AggregateStatus::InstallFailed => "Repair",
        AggregateStatus::Unsupported => "Unavailable",
        AggregateStatus::Installing | AggregateStatus::Uninstalling => "Installing",
    }
}

fn component_status(status: &ComponentState) -> &'static str {
    match status {
        ComponentState::Ready { .. } => "Ready",
        ComponentState::Missing => "Needs repair",
        ComponentState::Installing => "Installing",
        ComponentState::Outdated { .. } => "Outdated",
        ComponentState::Conflict { .. } => "Conflict",
        ComponentState::Failed { .. } => "Failed",
        ComponentState::Unsupported { .. } => "Unavailable",
    }
}
