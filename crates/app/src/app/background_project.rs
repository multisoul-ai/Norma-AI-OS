use super::{
    bootstrap::BootstrapSettingsSource, bootstrap::OrchestrationBootstrap, MultisoulApp,
    StoredBootstrap,
};
use gpui::{Context, Window};
use storage::{AppStore, DatabasePaths, ProjectStore, WorkspaceIdentity};

pub(crate) struct PreparedProject {
    paths: DatabasePaths,
    store: AppStore,
    identity: WorkspaceIdentity,
    project: ProjectStore,
    settings: StoredBootstrap,
    document: Option<canvas::DocumentSnapshot>,
    layout: Option<storage::LayoutStateSnapshot>,
    sessions: Vec<storage::NodeSession>,
    checkpoints: Vec<storage::SessionCheckpoint>,
    definitions: crate::settings::agents::AgentDefinitionsState,
    tails: Vec<storage::StoredAgentConversationTail>,
}

impl PreparedProject {
    pub(crate) fn load(paths: DatabasePaths, workspace: &str) -> Result<Self, String> {
        let load = || -> anyhow::Result<Self> {
            let store = AppStore::new(paths.app_db_path().to_owned());
            let identity = store
                .workspace_identity_store()
                .get(workspace)?
                .ok_or_else(|| anyhow::anyhow!("unknown project"))?;
            // Resolve only a locally registered identity, never a network-supplied path.
            anyhow::ensure!(
                identity.canonical_root.is_dir(),
                "project directory unavailable"
            );
            anyhow::ensure!(
                identity.canonical_root.canonicalize()? == identity.canonical_root,
                "project directory changed"
            );
            let project =
                ProjectStore::initialize_for_database_paths(&paths, &identity.canonical_root)?;
            Self::finish_load(paths, store, identity, project)
        };
        load().map_err(|error| error.to_string())
    }

    pub(crate) fn load_for_desktop(
        paths: DatabasePaths,
        area: storage::DesktopProjectArea,
    ) -> Result<Self, String> {
        let load = || -> anyhow::Result<Self> {
            anyhow::ensure!(area.root.is_dir(), "project directory unavailable");
            anyhow::ensure!(
                area.root.canonicalize()? == area.root,
                "project directory changed"
            );
            let store = AppStore::new(paths.app_db_path().to_owned());
            let identity = store
                .workspace_identity_store()
                .resolve_or_create(&area.root, &area.display_name)?;
            let project = ProjectStore::initialize_for_desktop_area(
                &paths,
                &area.root,
                &area.desktop_workspace_id,
                &area.id,
            )?;
            Self::finish_load(paths, store, identity, project)
        };
        load().map_err(|error| error.to_string())
    }

    fn finish_load(
        paths: DatabasePaths,
        store: AppStore,
        identity: WorkspaceIdentity,
        project: ProjectStore,
    ) -> anyhow::Result<Self> {
        let document = project.canvas_store().load_snapshot()?;
        let layout = project.layout_store().load_state()?;
        let sessions = project.node_session_store().load_sessions()?;
        let checkpoints = project.session_store().load_checkpoints()?;
        let mut definitions = crate::settings::agents::AgentDefinitionsState::default();
        definitions.bind_project_store(project.agent_definition_store());
        let tails = project.agent_conversation_tail_store().list()?;
        let settings = StoredBootstrap::load(&store);
        Ok(Self {
            paths,
            store,
            identity,
            project,
            settings,
            document,
            layout,
            sessions,
            checkpoints,
            definitions,
            tails,
        })
    }
}

impl MultisoulApp {
    pub(crate) fn from_background_project(
        prepared: PreparedProject,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut runtime =
            runtime::Runtime::new_for_workspace(prepared.identity.canonical_root.clone());
        if let Some(document) = prepared.document {
            runtime.restore_canvas_snapshot(document);
        }
        runtime.hydrate_agent_session_bindings(&prepared.sessions);
        let hook = crate::settings::agent_hooks::AgentHookServer::start(cx);
        let mut app = Self::build(
            cx,
            runtime,
            hook,
            prepared.paths,
            prepared.store,
            BootstrapSettingsSource::Background(Box::new(prepared.settings)),
            OrchestrationBootstrap::CurrentProcess,
        );
        app.background_restore = Some(crate::workspace::flow::PendingLiveSessionRestore::new(
            prepared.identity.canonical_root.clone(),
            &app.runtime,
            prepared.checkpoints.clone(),
            prepared.sessions,
        ));
        app.workspace_id = Some(prepared.identity.workspace_id);
        app.workspace_state.current_root = Some(prepared.identity.canonical_root);
        app.screen = super::AppScreen::Canvas;
        if let Some(layout) = prepared.layout {
            app.master_layout
                .restore_from_snapshot(layout, &app.runtime);
        }
        app.persistence.set_project_store(prepared.project);
        app.persistence.retained_checkpoints = prepared.checkpoints;
        app.agent_definitions = prepared.definitions;
        app.bind_preloaded_conversation_tails(prepared.tails);
        app
    }

    pub(crate) fn detach_project_window(&mut self, cx: &mut Context<Self>) {
        self.project_window = None;
        self.background_project = true;
        self.window_active = false;
        self.dialogue.on_window_closed();
        self.voice.cancel_speech_task();
        self.voice.cancel_speech_session();
        self.clear_browser_views();
        self.clear_ego_previews(cx);
        self.video_wallpaper = None;
        self.feature_guide_media = super::feature_guide::FeatureGuideMediaState::default();
        self.working_snake_views.clear();
        self.terminal_render_cache.borrow_mut().clear();
        self.runtime.focus.exit_to_canvas();
        self.wake_animation_frame_loop();
    }

    pub(crate) fn attach_project_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.project_window = Some(window.window_handle());
        self.background_project = false;
        self.window_active = true;
        if let Some(pending) = self.background_restore.take() {
            // User explicitly opened this project: restore its remaining desktop cards normally.
            if let Some(store) = self.persistence.project_store() {
                if let Err(error) =
                    crate::workspace::legacy_agent_migration::migrate_legacy_agent_cards(
                        &mut self.runtime,
                        store,
                    )
                {
                    self.workspace_state.error = Some(error);
                }
                self.agent_definitions
                    .bind_project_store(store.agent_definition_store());
            }
            self.spawn_deferred_live_session_restore(pending, cx);
        }
        self.start_dialogue_turn_loop(window, cx);
        self.start_cef_message_pump_loop(window, cx);
        self.reload_all_previews(cx);
        self.last_reflow_window_size = None;
        self.wake_animation_frame_loop();
        cx.notify();
    }

    pub(crate) fn attach_desktop_area_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_window = Some(window.window_handle());
        self.background_project = false;
        self.window_active = true;
        if let Some(pending) = self.background_restore.take() {
            self.spawn_deferred_live_session_restore(pending, cx);
        }
        self.start_dialogue_turn_loop(window, cx);
        self.start_cef_message_pump_loop(window, cx);
        self.reload_all_previews(cx);
        self.last_reflow_window_size = None;
        self.wake_animation_frame_loop();
        cx.notify();
    }

    pub(crate) fn activate_desktop_area_overview(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.background_restore.take() {
            self.spawn_deferred_live_session_restore(pending, cx);
        }
    }
}
