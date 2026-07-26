use crate::app::MultisoulApp;
use gpui::Context;
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, ComponentState, InstallPaths, OrchestrationInstaller,
    OrchestrationProvider, ProviderIntegration,
};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum OrchestrationOperation {
    InstallOrRepair,
    Uninstall,
}

impl OrchestrationOperation {
    const fn slug(self) -> &'static str {
        match self {
            Self::InstallOrRepair => "install_or_repair",
            Self::Uninstall => "uninstall",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OrchestrationSettingsState {
    pub(super) integrations: Vec<ProviderIntegration>,
    pub(super) expanded_provider: Option<OrchestrationProvider>,
    pub(super) busy: Option<(OrchestrationProvider, OrchestrationOperation)>,
    pub(super) pending_uninstall: Option<OrchestrationProvider>,
    pub(super) operation_error: Option<(OrchestrationProvider, String)>,
    refresh_generation: u64,
}

impl Default for OrchestrationSettingsState {
    fn default() -> Self {
        Self {
            integrations: OrchestrationProvider::ALL
                .into_iter()
                .map(missing_integration)
                .collect(),
            expanded_provider: None,
            busy: None,
            pending_uninstall: None,
            operation_error: None,
            refresh_generation: 0,
        }
    }
}

impl OrchestrationSettingsState {
    pub(crate) fn integration(&self, provider: OrchestrationProvider) -> ProviderIntegration {
        self.integrations
            .iter()
            .find(|integration| integration.provider == provider)
            .cloned()
            .unwrap_or_else(|| missing_integration(provider))
    }

    fn begin_refresh(&mut self) -> u64 {
        self.refresh_generation = self.refresh_generation.saturating_add(1);
        self.refresh_generation
    }

    fn apply_refresh(&mut self, generation: u64, integrations: Vec<ProviderIntegration>) -> bool {
        if generation != self.refresh_generation {
            return false;
        }
        self.integrations = integrations;
        true
    }

    fn begin_operation(
        &mut self,
        provider: OrchestrationProvider,
        operation: OrchestrationOperation,
    ) -> u64 {
        self.busy = Some((provider, operation));
        self.pending_uninstall = None;
        self.operation_error = None;
        self.begin_refresh()
    }

    fn finish_operation(
        &mut self,
        generation: u64,
        provider: OrchestrationProvider,
        result: Result<(Vec<ProviderIntegration>, Option<String>), String>,
    ) {
        if generation != self.refresh_generation {
            return;
        }
        self.busy = None;
        match result {
            Ok((integrations, operation_error)) => {
                self.integrations = integrations;
                self.operation_error = operation_error.map(|error| (provider, error));
            }
            Err(error) => self.operation_error = Some((provider, error)),
        }
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn use_canonical_preview(&mut self) {
        let ready = |provider| {
            let component = ComponentState::Ready {
                version: env!("CARGO_PKG_VERSION").to_string(),
            };
            ProviderIntegration::from_components(
                provider,
                component.clone(),
                component.clone(),
                component.clone(),
                component,
            )
        };
        let ready_component = ComponentState::Ready {
            version: env!("CARGO_PKG_VERSION").to_string(),
        };
        self.integrations = vec![
            ready(OrchestrationProvider::Codex),
            missing_integration(OrchestrationProvider::Claude),
            ProviderIntegration::from_components(
                OrchestrationProvider::Cursor,
                ready_component.clone(),
                ready_component.clone(),
                ComponentState::Missing,
                ready_component,
            ),
            ready(OrchestrationProvider::OpenCode),
            ProviderIntegration::unsupported(OrchestrationProvider::Grok),
        ];
        self.expanded_provider = Some(OrchestrationProvider::Cursor);
        self.busy = None;
        self.pending_uninstall = None;
        self.operation_error = None;
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn expanded_provider_for_e2e(&self) -> Option<OrchestrationProvider> {
        self.expanded_provider
    }

    #[cfg(feature = "e2e-test-support")]
    pub(crate) fn pending_uninstall_for_e2e(&self) -> Option<OrchestrationProvider> {
        self.pending_uninstall
    }
}

impl MultisoulApp {
    pub(crate) fn refresh_orchestration_integrations(&mut self, cx: &mut Context<Self>) {
        let generation = self.orchestration_settings.begin_refresh();
        let database_path = authorization_database_path(&self.database_paths);
        let work = cx
            .background_executor()
            .spawn(async move { inspect_integrations(database_path) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok((integrations, records)) => {
                        if app
                            .orchestration_settings
                            .apply_refresh(generation, integrations)
                        {
                            app.orchestration_authorizations =
                                storage::OrchestrationAuthorizationCache::new(records);
                        }
                    }
                    Err(error) => tracing::warn!(%error, "orchestration inspection failed"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn activate_orchestration_provider(
        &mut self,
        provider: OrchestrationProvider,
        cx: &mut Context<Self>,
    ) {
        let integration = self.orchestration_settings.integration(provider);
        if integration.aggregate_status == AggregateStatus::Unsupported
            || self.orchestration_settings.busy.is_some()
        {
            return;
        }
        if integration.aggregate_status == AggregateStatus::Ready
            && self.orchestration_settings.pending_uninstall != Some(provider)
        {
            self.orchestration_settings.pending_uninstall = Some(provider);
            cx.notify();
            return;
        }
        let operation = if integration.aggregate_status == AggregateStatus::Ready {
            OrchestrationOperation::Uninstall
        } else {
            OrchestrationOperation::InstallOrRepair
        };
        let generation = self
            .orchestration_settings
            .begin_operation(provider, operation);
        let database_path = authorization_database_path(&self.database_paths);
        let work = cx
            .background_executor()
            .spawn(async move { perform_operation(database_path, provider, operation) });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            let _ = this.update(cx, |app, cx| {
                match &result {
                    Ok((_, records, operation_error)) => {
                        app.orchestration_authorizations =
                            storage::OrchestrationAuthorizationCache::new(records.clone());
                        log_operation_error(provider, operation, operation_error.as_deref());
                    }
                    Err(error) => {
                        log_operation_error(provider, operation, Some(error.as_str()));
                    }
                }
                app.orchestration_settings.finish_operation(
                    generation,
                    provider,
                    result.map(|(integrations, _, error)| (integrations, error)),
                );
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}

fn authorization_database_path(paths: &storage::DatabasePaths) -> PathBuf {
    paths.app_db_path().to_path_buf()
}

fn inspect_integrations(
    database_path: PathBuf,
) -> Result<
    (
        Vec<ProviderIntegration>,
        Vec<storage::OrchestrationAuthorizationRecord>,
    ),
    String,
> {
    let paths = InstallPaths::for_current_process().map_err(|error| error.to_string())?;
    let store = storage::OrchestrationAuthorizationStore::new(database_path);
    let registry = StorageAuthorizationRegistry::new(store);
    let installer = OrchestrationInstaller::new(paths, env!("CARGO_PKG_VERSION"));
    let integrations = OrchestrationProvider::ALL
        .into_iter()
        .map(|provider| installer.inspect(provider, &registry))
        .collect();
    let records = registry.list().map_err(|error| error.to_string())?;
    Ok((integrations, records))
}

fn perform_operation(
    database_path: PathBuf,
    provider: OrchestrationProvider,
    operation: OrchestrationOperation,
) -> Result<
    (
        Vec<ProviderIntegration>,
        Vec<storage::OrchestrationAuthorizationRecord>,
        Option<String>,
    ),
    String,
> {
    let paths = InstallPaths::for_current_process().map_err(|error| error.to_string())?;
    let store = storage::OrchestrationAuthorizationStore::new(database_path);
    let mut registry = StorageAuthorizationRegistry::new(store);
    let installer = OrchestrationInstaller::new(paths, env!("CARGO_PKG_VERSION"));
    let operation_error = match operation {
        OrchestrationOperation::InstallOrRepair => installer
            .install_or_repair(provider, &mut registry)
            .err()
            .map(|error| error.to_string()),
        OrchestrationOperation::Uninstall => installer
            .uninstall(provider, &mut registry)
            .err()
            .map(|error| error.to_string()),
    };
    let integrations = OrchestrationProvider::ALL
        .into_iter()
        .map(|candidate| installer.inspect(candidate, &registry))
        .collect();
    let records = registry.list().map_err(|error| error.to_string())?;
    Ok((integrations, records, operation_error))
}

fn log_operation_error(
    provider: OrchestrationProvider,
    operation: OrchestrationOperation,
    error: Option<&str>,
) {
    let Some(error) = error else {
        return;
    };
    tracing::warn!(
        provider = provider.slug(),
        operation = operation.slug(),
        %error,
        "orchestration operation failed"
    );
}

struct StorageAuthorizationRegistry {
    store: storage::OrchestrationAuthorizationStore,
}

impl StorageAuthorizationRegistry {
    fn new(store: storage::OrchestrationAuthorizationStore) -> Self {
        Self { store }
    }

    fn list(&self) -> storage::StorageResult<Vec<storage::OrchestrationAuthorizationRecord>> {
        self.store.list()
    }
}

impl AuthorizationRegistry for StorageAuthorizationRegistry {
    type Error = storage::StorageError;

    fn authorize(
        &mut self,
        provider: OrchestrationProvider,
        token: &str,
        installed_version: &str,
    ) -> Result<(), Self::Error> {
        self.store
            .authorize(provider.slug(), token, installed_version)
    }

    fn revoke(&mut self, provider: OrchestrationProvider) -> Result<(), Self::Error> {
        self.store.revoke(provider.slug())
    }

    fn is_authorized(
        &self,
        provider: OrchestrationProvider,
        token: &str,
    ) -> Result<bool, Self::Error> {
        self.store.is_authorized(provider.slug(), token)
    }
}

fn missing_integration(provider: OrchestrationProvider) -> ProviderIntegration {
    if provider == OrchestrationProvider::Grok {
        ProviderIntegration::unsupported(provider)
    } else {
        ProviderIntegration::from_components(
            provider,
            ComponentState::Missing,
            ComponentState::Missing,
            ComponentState::Missing,
            ComponentState::Missing,
        )
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/orchestration_controller_tests.rs"]
mod tests;
