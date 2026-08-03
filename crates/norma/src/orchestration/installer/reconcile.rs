use super::{AuthorizationRegistry, InstallerError, OrchestrationInstaller};
use crate::agent_hook::install_hook_config;
use crate::orchestration::model::{
    AggregateStatus, ComponentState, OrchestrationProvider, ProviderIntegration,
};

#[derive(Debug)]
pub struct OrchestrationReconcileFailure {
    pub provider: OrchestrationProvider,
    pub error: InstallerError,
}

#[derive(Debug)]
pub struct OrchestrationReconcileReport {
    pub integrations: Vec<ProviderIntegration>,
    pub failures: Vec<OrchestrationReconcileFailure>,
}

impl OrchestrationInstaller {
    pub fn reconcile_installed<R: AuthorizationRegistry>(
        &self,
        registry: &mut R,
    ) -> OrchestrationReconcileReport {
        let installed_providers = OrchestrationProvider::ALL
            .into_iter()
            .filter(|provider| {
                let integration = self.inspect(*provider, registry);
                integration.aggregate_status != AggregateStatus::Ready
                    && matches!(
                        integration.authorization_component,
                        ComponentState::Ready { .. }
                    )
            })
            .collect::<Vec<_>>();
        let mut failures = Vec::new();

        if let Some(provider) = installed_providers.first().copied() {
            self.reconcile_shared_binaries(provider, &mut failures);
        }
        for provider in installed_providers {
            self.reconcile_provider_components(provider, &mut failures);
        }

        let integrations = OrchestrationProvider::ALL
            .into_iter()
            .map(|provider| self.inspect(provider, registry))
            .collect();
        OrchestrationReconcileReport {
            integrations,
            failures,
        }
    }

    fn reconcile_shared_binaries(
        &self,
        provider: OrchestrationProvider,
        failures: &mut Vec<OrchestrationReconcileFailure>,
    ) {
        if !matches!(
            self.inspect_binary(
                self.paths.managed_cli_path().as_path(),
                self.paths.bundled_cli.as_path(),
            ),
            ComponentState::Ready { .. }
        ) {
            record_failure(
                failures,
                provider,
                self.install_binary(
                    self.paths.bundled_cli.as_path(),
                    self.paths.managed_cli_path().as_path(),
                ),
            );
        }
        if !matches!(
            self.inspect_binary(
                self.paths.managed_hook_path().as_path(),
                self.paths.bundled_hook.as_path(),
            ),
            ComponentState::Ready { .. }
        ) {
            record_failure(
                failures,
                provider,
                self.install_binary(
                    self.paths.bundled_hook.as_path(),
                    self.paths.managed_hook_path().as_path(),
                ),
            );
        }
    }

    fn reconcile_provider_components(
        &self,
        provider: OrchestrationProvider,
        failures: &mut Vec<OrchestrationReconcileFailure>,
    ) {
        record_failure(failures, provider, self.install_skills_if_owned(provider));

        if matches!(
            self.inspect_hook(provider),
            ComponentState::Ready { .. } | ComponentState::Conflict { .. }
        ) {
            return;
        }
        let result = install_hook_config(
            provider
                .hook_target()
                .expect("supported orchestration provider has a hook target"),
            self.paths.managed_hook_path().as_path(),
            &self.paths.hook_paths,
        )
        .map_err(InstallerError::from);
        record_failure(failures, provider, result);
    }
}

fn record_failure(
    failures: &mut Vec<OrchestrationReconcileFailure>,
    provider: OrchestrationProvider,
    result: Result<(), InstallerError>,
) {
    if let Err(error) = result {
        failures.push(OrchestrationReconcileFailure { provider, error });
    }
}
