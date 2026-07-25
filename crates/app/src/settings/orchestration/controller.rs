use crate::app::MultisoulApp;
use gpui::Context;
use norma::orchestration::{
    AggregateStatus, AuthorizationRegistry, ComponentState, InstallPaths, OrchestrationInstaller,
    OrchestrationProvider, ProviderIntegration,
};

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
        let work = cx
            .background_executor()
            .spawn(async move { inspect_integrations() });
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
        let work = cx
            .background_executor()
            .spawn(async move { perform_operation(provider, operation) });
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

fn inspect_integrations() -> Result<
    (
        Vec<ProviderIntegration>,
        Vec<storage::OrchestrationAuthorizationRecord>,
    ),
    String,
> {
    let paths = InstallPaths::for_current_process().map_err(|error| error.to_string())?;
    let store = storage::OrchestrationAuthorizationStore::new(
        storage::default_app_db_path().map_err(|error| error.to_string())?,
    );
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
    let store = storage::OrchestrationAuthorizationStore::new(
        storage::default_app_db_path().map_err(|error| error.to_string())?,
    );
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
mod tests {
    use super::{
        log_operation_error, missing_integration, OrchestrationOperation,
        OrchestrationSettingsState,
    };
    use norma::orchestration::OrchestrationProvider;
    use std::{fs, sync::Mutex};

    /// 后台扫描代际：安装/卸载开始前的旧扫描不得覆盖最新授权缓存。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   initial generation = 0
    ///   stale refresh      = begin_refresh() = 1
    ///   operation refresh  = begin_refresh() = 2
    ///   Total callbacks    = stale(1) + current(1) = 2
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 启动普通扫描并记录 generation=1。
    ///   2. 安装/卸载开始后推进到 generation=2。
    ///   3. generation=1 晚到 → 拒绝结果，调用方不得替换授权缓存。
    ///   4. generation=2 到达 → 接受结果，调用方可以同步替换授权缓存。
    ///
    /// 预期结果：
    ///   - 正断言：当前 generation=2 被接受。
    ///   - 负断言：过期 generation=1 被拒绝，不能重新启用已撤销令牌。
    #[test]
    fn stale_refresh_generation_cannot_replace_authorization_cache() {
        let mut state = OrchestrationSettingsState::default();
        let stale_generation = state.begin_refresh();
        let current_generation = state.begin_refresh();
        let integrations = vec![missing_integration(OrchestrationProvider::Codex)];

        assert!(
            !state.apply_refresh(stale_generation, integrations.clone()),
            "generation=1 的旧扫描不得在 generation=2 后覆盖授权缓存"
        );
        assert!(
            state.apply_refresh(current_generation, integrations),
            "当前 generation=2 扫描必须被接受并允许同步授权缓存"
        );
        assert_ne!(
            stale_generation, current_generation,
            "旧扫描与当前操作必须使用不同 generation 才能隔离竞态"
        );
    }

    /// 安装器内部失败日志：UI 已收到的 operation_error 必须同时进入 tracing。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   failure reports = 1（Codex install_or_repair + os error 2）
    ///   success reports = 1（同 Provider、同操作，但 error=None）
    ///   expected warning events = failure(1) + success(0) = 1
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 安装局部无 ANSI tracing subscriber，并把输出写入临时日志文件。
    ///   2. 上报一次 Codex install_or_repair 失败 → 应写 warning。
    ///   3. 上报一次无错误结果 → 不应伪造第二条失败 warning。
    ///   4. 读取日志并检查 Provider、操作和原始系统错误。
    ///
    /// 预期结果：
    ///   - 正断言：日志包含唯一一条 orchestration operation failed。
    ///   - 正断言：日志包含 provider=codex 与 operation=install_or_repair。
    ///   - 正断言：日志保留 No such file or directory (os error 2)。
    ///   - 负断言：成功结果不得产生第二条失败日志。
    #[test]
    fn installer_operation_error_is_written_to_tracing_log() {
        let temp = tempfile::tempdir().expect("测试应创建隔离日志目录");
        let log_path = temp.path().join("orchestration.log");
        let log_file = fs::File::create(&log_path).expect("测试应创建日志文件");
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_writer(Mutex::new(log_file))
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            log_operation_error(
                OrchestrationProvider::Codex,
                OrchestrationOperation::InstallOrRepair,
                Some("No such file or directory (os error 2)"),
            );
            log_operation_error(
                OrchestrationProvider::Codex,
                OrchestrationOperation::InstallOrRepair,
                None,
            );
        });

        let contents = fs::read_to_string(log_path).expect("测试应读取 tracing 日志");
        assert!(
            contents.contains("orchestration operation failed"),
            "安装器返回 operation_error 时必须写入可检索的失败事件"
        );
        assert!(
            contents.contains("provider=\"codex\""),
            "失败日志必须包含 provider=codex 以定位受影响集成"
        );
        assert!(
            contents.contains("operation=\"install_or_repair\""),
            "失败日志必须区分 install_or_repair 与 uninstall"
        );
        assert!(
            contents.contains("No such file or directory (os error 2)"),
            "失败日志必须保留 UI 已收到的底层系统错误"
        );
        assert_eq!(
            contents.matches("orchestration operation failed").count(),
            1,
            "failure(1) + success(0) 应只产生一条失败日志，成功结果不得误报"
        );
    }
}
