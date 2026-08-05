use crate::app::MultisoulApp;
use gpui::{App, Context, Window};
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
    pub fn enter_canvas_and_register_norma_route_for_e2e(
        &mut self,
        root: &std::path::Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.enter_canvas_for_test(root, cx);
        self.register_norma_workspace_route(root, window, cx);
    }

    pub fn norma_workspace_identity_for_e2e(&self) -> Option<(String, String)> {
        Some((self.workspace_id.clone()?, self.workspace_route_id.clone()?))
    }

    /// E2E 只读辅助：当前 coordinator 未完成任务总数，用于断言自派发被拒后零副作用。
    pub fn agent_coordinator_task_count_for_e2e(&self) -> usize {
        self.agent_task_coordinator
            .as_ref()
            .map(|coordinator| coordinator.tasks().len())
            .unwrap_or(0)
    }

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

pub fn install_norma_endpoint_for_e2e(
    socket_path: std::path::PathBuf,
    cx: &mut App,
) -> std::io::Result<()> {
    crate::norma::endpoint::install_at_path(socket_path, cx)
}

pub fn mark_orchestration_health_ready_for_e2e(cx: &mut App) {
    crate::settings::orchestration::health::mark_ready_for_e2e(cx);
}

pub fn publish_canonical_orchestration_health_for_e2e(cx: &mut App) {
    let integrations = crate::settings::orchestration::canonical_preview_integrations();
    crate::settings::orchestration::health::publish(integrations, Vec::new(), cx);
}

pub fn submit_norma_request_for_e2e(
    request: norma::protocol::WireRequest,
    cx: &mut App,
) -> crossbeam_channel::Receiver<norma::protocol::WireResponse> {
    crate::norma::endpoint::submit_for_e2e(request, cx)
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
