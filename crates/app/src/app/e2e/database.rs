use crate::app::MultisoulApp;
use crate::settings::agent_hooks::AgentHookServer;
use gpui::Context;
use runtime::Runtime;

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn retain_mobile_server_for_e2e(
        &self,
        server: std::sync::Arc<crate::MobileServer>,
        cx: &mut gpui::App,
    ) {
        self.mobile
            .update(cx, |mobile, _| mobile.use_server_for_e2e(server));
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn project_windows_closed_for_e2e(cx: &mut gpui::App) -> bool {
        crate::workspace::sessions::windows_closed(cx)
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn stop_background_projects_for_e2e(cx: &mut gpui::App) {
        crate::workspace::sessions::shutdown(cx);
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn loaded_project_for_e2e(
        &self,
        workspace: &str,
        cx: &gpui::App,
    ) -> Option<gpui::Entity<MultisoulApp>> {
        crate::workspace::sessions::find(self.database_paths.app_db_path(), workspace, cx)
    }
    pub fn mobile_push_settings_for_e2e(
        &self,
        cx: &gpui::App,
    ) -> (bool, Option<String>, Option<String>) {
        let host = self.mobile.read(cx);
        (
            host.enabled(),
            host.push.as_ref().and_then(|s| s.endpoint.clone()),
            host.error.clone(),
        )
    }
    pub fn new_with_pty_factory(cx: &mut Context<Self>, factory: runtime::PtyFactory) -> Self {
        let (database_paths, database_guard) = crate::database::isolated_test_database()
            .unwrap_or_else(|error| {
                panic!("failed to create isolated test database paths: {error}")
            });
        let mut app = Self::build_with_pty_factory_and_database_paths(
            cx,
            factory,
            database_paths,
            super::super::bootstrap::BootstrapSettingsSource::InMemory,
        );
        app.test_database_guard = Some(std::sync::Arc::new(database_guard));
        #[cfg(feature = "e2e-test-support")]
        app.voice_settings.assume_legacy_ready_for_e2e();
        app.voice_settings_inputs
            .sync_from_state(&app.voice_settings, cx);
        app
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn new_with_isolated_test_database(cx: &mut Context<Self>) -> Self {
        let (database_paths, database_guard) = crate::database::isolated_test_database()
            .unwrap_or_else(|error| {
                panic!("failed to create isolated e2e database paths: {error}")
            });
        let app_store = storage::AppStore::initialize(&database_paths).unwrap_or_else(|error| {
            panic!("failed to initialize isolated e2e app database: {error}")
        });
        let agent_hook_server = AgentHookServer::start(cx);
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        let mut app = Self::build(
            cx,
            runtime,
            agent_hook_server,
            database_paths,
            app_store,
            super::super::bootstrap::BootstrapSettingsSource::Stored,
            super::super::bootstrap::OrchestrationBootstrap::Skip,
        );
        app.test_database_guard = Some(std::sync::Arc::new(database_guard));
        app
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn new_with_database_paths_and_orchestration_paths(
        cx: &mut Context<Self>,
        database_paths: storage::DatabasePaths,
        install_paths: norma::orchestration::InstallPaths,
    ) -> Self {
        let app_store = storage::AppStore::initialize(&database_paths).unwrap_or_else(|error| {
            panic!("failed to initialize isolated e2e app database: {error}")
        });
        let agent_hook_server = AgentHookServer::start(cx);
        let mut runtime = Runtime::new();
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        Self::build(
            cx,
            runtime,
            agent_hook_server,
            database_paths,
            app_store,
            super::super::bootstrap::BootstrapSettingsSource::Stored,
            super::super::bootstrap::OrchestrationBootstrap::Paths(Box::new(install_paths)),
        )
    }

    pub fn new_with_pty_factory_and_database_paths(
        cx: &mut Context<Self>,
        factory: runtime::PtyFactory,
        database_paths: storage::DatabasePaths,
    ) -> Self {
        Self::build_with_pty_factory_and_database_paths(
            cx,
            factory,
            database_paths,
            super::super::bootstrap::BootstrapSettingsSource::Stored,
        )
    }

    fn build_with_pty_factory_and_database_paths(
        cx: &mut Context<Self>,
        factory: runtime::PtyFactory,
        database_paths: storage::DatabasePaths,
        settings_source: super::super::bootstrap::BootstrapSettingsSource,
    ) -> Self {
        let agent_hook_server = AgentHookServer::start(cx);
        let mut runtime = Runtime::new();
        runtime.set_pty_factory(factory);
        runtime.set_agent_hook_config(agent_hook_server.runtime_config());
        let app_store = storage::AppStore::initialize(&database_paths)
            .unwrap_or_else(|error| panic!("failed to initialize test app database: {error}"));
        let mut app = Self::build(
            cx,
            runtime,
            agent_hook_server,
            database_paths,
            app_store,
            settings_source,
            super::super::bootstrap::OrchestrationBootstrap::Skip,
        );
        app.browsers.disable_native_views();
        app
    }

    pub fn new_with_pty_factory_and_voice_settings_store(
        cx: &mut Context<Self>,
        factory: runtime::PtyFactory,
        store: storage::SettingsStore,
    ) -> Self {
        let database_paths = storage::DatabasePaths::isolated_test_database(store.path());
        let app = Self::new_with_pty_factory_and_database_paths(cx, factory, database_paths);
        #[cfg(feature = "e2e-test-support")]
        let mut app = app;
        #[cfg(feature = "e2e-test-support")]
        app.voice_settings.assume_legacy_ready_for_e2e();
        app.voice_settings_inputs
            .sync_from_state(&app.voice_settings, cx);
        app
    }

    pub fn load_voice_provider_settings_for_e2e(
        store: storage::SettingsStore,
    ) -> (String, String, bool) {
        let state = crate::settings::voice::VoiceSettingsState::load_from_store(store);
        let profile = state.values().active_provider_profile();
        (
            profile.base_url().to_string(),
            profile.model().to_string(),
            profile.credential_configured(),
        )
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn app_database_path_for_e2e(&self) -> std::path::PathBuf {
        self.database_paths.app_db_path().to_path_buf()
    }

    pub fn mobile_review_source_for_e2e(
        &self,
        cx: &gpui::App,
    ) -> Option<async_channel::Sender<crate::mobile::LiveReviewRequest>> {
        self.mobile.read(cx).review_source()
    }
    pub fn mobile_snapshot_source_for_e2e(
        &self,
        cx: &gpui::App,
    ) -> Option<async_channel::Sender<crate::LiveSnapshotRequest>> {
        self.mobile.read(cx).snapshot_source()
    }
    pub fn mobile_approval_source_for_e2e(
        &self,
        cx: &gpui::App,
    ) -> Option<async_channel::Sender<crate::LiveApprovalRequest>> {
        self.mobile.read(cx).approval_source()
    }
    pub fn mobile_terminal_source_for_e2e(
        &self,
        cx: &gpui::App,
    ) -> Option<async_channel::Sender<crate::LiveTerminalRequest>> {
        self.mobile.read(cx).terminal_source()
    }
    pub fn mobile_conversation_source_for_e2e(
        &self,
        cx: &gpui::App,
    ) -> Option<async_channel::Sender<crate::LiveConversationRequest>> {
        self.mobile.read(cx).conversation_source()
    }
    pub fn mobile_command_source_for_e2e(
        &self,
        cx: &gpui::App,
    ) -> Option<async_channel::Sender<crate::LiveCommandRequest>> {
        self.mobile.read(cx).command_source()
    }

    #[cfg(feature = "e2e-test-support")]
    pub fn project_store_for_e2e(
        &self,
        workspace: &std::path::Path,
    ) -> storage::StorageResult<storage::ProjectStore> {
        storage::ProjectStore::for_database_paths(&self.database_paths, workspace)
    }
}
