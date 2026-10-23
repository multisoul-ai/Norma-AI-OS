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
            let definitions_store = project.agent_definition_store();
            Self::finish_load(paths, store, identity, project, definitions_store)
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
            let mut identity = store
                .workspace_identity_store()
                .resolve_or_create(&area.root, &workspace::display_name_for_path(&area.root))?;
            // CLI routing belongs to this canvas, even when another canvas uses the same root.
            // Keep the persisted project identity unchanged for legacy/mobile project lookup.
            identity.workspace_id = format!("desktop:{}:{}", area.desktop_workspace_id, area.id);
            let project = ProjectStore::initialize_for_desktop_area(
                &paths,
                &area.root,
                &area.desktop_workspace_id,
                &area.id,
            )?;
            // This loader runs off the GPUI foreground thread. Remove only legacy
            // ordinary content cards before constructing the live Runtime; Ego tasks stay.
            project.canvas_store().prune_desktop_content_cards()?;
            let definitions_store =
                ProjectStore::initialize_for_desktop_definitions(&paths, &area.project_key)?
                    .agent_definition_store();
            let canvas_store = project.canvas_store();
            let mut migration_runtime = runtime::Runtime::new_for_workspace(area.root.clone());
            migration_runtime.load_canvas_snapshot(&canvas_store)?;
            // Definitions are shared by sibling areas; one area's canvas cannot decide which
            // quick definitions are unused by the whole project.
            crate::workspace::legacy_agent_migration::migrate_legacy_agent_cards_with_stores(
                &mut migration_runtime,
                &definitions_store,
                &canvas_store,
                false,
            )
            .map_err(anyhow::Error::msg)?;
            Self::finish_load(paths, store, identity, project, definitions_store)
        };
        load().map_err(|error| error.to_string())
    }

    fn finish_load(
        paths: DatabasePaths,
        store: AppStore,
        identity: WorkspaceIdentity,
        project: ProjectStore,
        definitions_store: storage::AgentDefinitionStore,
    ) -> anyhow::Result<Self> {
        let document = project.canvas_store().load_snapshot()?;
        let layout = project.layout_store().load_state()?;
        let sessions = project.node_session_store().load_sessions()?;
        let checkpoints = project.session_store().load_checkpoints()?;
        let mut definitions = crate::settings::agents::AgentDefinitionsState::default();
        definitions.bind_project_store(definitions_store);
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
        // 豆包播报通道随后台项目常驻；只停止收音。
        self.stop_doubao_listening(cx);
        self.stop_gpt_live_voice(cx);
        self.project_window = None;
        self.background_project = true;
        self.window_active = false;
        self.dialogue.on_window_closed();
        self.voice.cancel_speech_task();
        self.voice.cancel_speech_session();
        if self.desktop_host.is_some() {
            // Overview and expanded presentations own the same live Browser instance.
            self.release_all_browser_input();
        } else {
            self.clear_browser_views();
        }
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
                    self.workspace_state.error = Some(
                        ui::i18n::LocalizedText::new("common.operation_failed")
                            .with_arg("reason", error),
                    );
                }
                self.agent_definitions
                    .bind_project_store(store.agent_definition_store());
            }
            self.spawn_deferred_live_session_restore(pending, cx);
        }
        self.start_dialogue_turn_loop(window, cx);
        self.start_cef_message_pump_loop(window, cx);
        self.reload_all_previews(cx);
        self.resync_voice_settings(true, cx);
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
        self.resync_voice_settings(true, cx);
        self.wake_animation_frame_loop();
        cx.notify();
    }

    pub(crate) fn activate_desktop_area_overview(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.background_restore.take() {
            self.spawn_deferred_live_session_restore(pending, cx);
        }
        self.resync_voice_settings(true, cx);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/desktop_legacy_agent_background_tests.rs"]
mod legacy_background_tests;

#[cfg(test)]
mod desktop_project_tests {
    use super::*;

    /// DWS-08：删除桌面区域后，同一项目的 Agent 定义应由项目数据库保留。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   项目数 = 1；桌面区域数 = 2；定义数 = 1。
    ///   删除区域甲后，存活区域数 = 2 - 1 = 1，定义数仍为 1。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 在项目级数据库创建一条 Agent 定义。
    ///   2. 分别加载甲、乙两个区域，核对两者均可读取同一定义。
    ///   3. 删除甲区域后再加载乙，核对项目定义仍存在。
    ///
    /// 预期结果：
    ///   - 正断言：乙和重新加载的乙均含该定义。
    ///   - 负断言：甲区域已删除，不能继续出现在工作空间目录。
    #[test]
    fn desktop_area_removal_preserves_project_agent_definitions() {
        let temp = tempfile::tempdir().expect("应创建隔离目录");
        let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
        let project = temp.path().join("project");
        std::fs::create_dir(&project).expect("应创建项目目录");
        let catalog = storage::DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
        let alpha = catalog.create("Alpha").expect("应创建甲");
        let beta = catalog.create("Beta").expect("应创建乙");
        let area_a = catalog
            .add_project(&alpha.id, &project)
            .expect("甲应添加项目");
        let area_b = catalog
            .add_project(&beta.id, &project)
            .expect("乙应添加项目");
        let project_store =
            ProjectStore::initialize_for_desktop_definitions(&paths, &area_a.project_key)
                .expect("应初始化项目级定义数据库");
        let definition = project_store
            .agent_definition_store()
            .create(&storage::NewAgentDefinition {
                name: "SharedAgent".into(),
                runtime_profile: canvas::AgentProfile::Codex,
                workflow_description: "Shared across desktop workspaces".into(),
            })
            .expect("应保存项目级定义");

        let loaded_a =
            PreparedProject::load_for_desktop(paths.clone(), area_a.clone()).expect("甲区域应加载");
        let loaded_b =
            PreparedProject::load_for_desktop(paths.clone(), area_b.clone()).expect("乙区域应加载");
        assert!(
            loaded_a
                .definitions
                .definitions()
                .iter()
                .any(|item| item.id == definition.id),
            "甲应读取项目级 Agent 定义"
        );
        assert!(
            loaded_b
                .definitions
                .definitions()
                .iter()
                .any(|item| item.id == definition.id),
            "乙应读取项目级 Agent 定义"
        );

        catalog
            .remove_project(&alpha.id, &area_a.id)
            .expect("应移除甲区域");
        let reloaded_b =
            PreparedProject::load_for_desktop(paths, area_b).expect("删除甲后乙区域应加载");
        assert!(
            reloaded_b
                .definitions
                .definitions()
                .iter()
                .any(|item| item.id == definition.id),
            "删除甲后项目级 Agent 定义应保留"
        );
        assert!(
            !catalog
                .list_projects(&alpha.id)
                .expect("应读取甲区域")
                .iter()
                .any(|item| item.id == area_a.id),
            "删除后甲区域不得残留"
        );
    }

    /// 画布名称隔离：加载同目录的不同画布时不能把画布名称写入共享项目身份。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   根目录 project 1 个；同空间画布 2 张，第一张改名 Release canvas。
    ///   共享身份数始终为 1，其项目名称应来自目录 project，而不是任一画布标题。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 创建共享项目身份和两张画布，重命名第一张。
    ///   2. 用生产 PreparedProject 接口加载改名画布，再加载另一张画布。
    ///   3. 读取身份和画布目录，检查两种名称的作用域。
    ///
    /// 预期结果：
    ///   - 正断言：两次加载的项目名和存储身份名均为 project，第一张画布保持新标题。
    ///   - 负断言：共享项目名不能变为 Release canvas，第二张标题不能被覆盖。
    #[test]
    fn desktop_canvas_rename_does_not_change_project_identity() {
        let temp = tempfile::tempdir().expect("应创建隔离目录");
        let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
        let project = temp.path().join("project");
        std::fs::create_dir(&project).expect("应创建项目目录");
        let catalog = storage::DesktopWorkspaceStore::new(paths.app_db_path().to_owned());
        let workspace = catalog.create("Workspace").expect("应创建空间");
        let first = catalog
            .add_project(&workspace.id, &project)
            .expect("应添加第一张画布");
        let second = catalog
            .add_project(&workspace.id, &project)
            .expect("应添加第二张画布");
        let identities = storage::WorkspaceIdentityStore::new(paths.app_db_path().to_owned());
        let shared = identities
            .resolve_or_create(&project, "project")
            .expect("应创建共享身份");
        catalog
            .rename_project(&workspace.id, &first.id, "Release canvas")
            .expect("应改名第一张画布");
        let areas = catalog
            .list_projects(&workspace.id)
            .expect("应读取画布目录");
        let renamed = areas
            .iter()
            .find(|area| area.id == first.id)
            .expect("应有改名画布")
            .clone();

        let loaded =
            PreparedProject::load_for_desktop(paths.clone(), renamed).expect("改名画布应可加载");
        assert_eq!(
            loaded.identity.display_name, "project",
            "画布加载必须使用项目名称注册身份"
        );
        let stored = identities
            .get(&shared.workspace_id)
            .expect("应读取共享身份")
            .expect("身份应保留");
        assert_ne!(
            stored.display_name, "Release canvas",
            "画布标题不能写入共享项目身份"
        );
        let sibling =
            PreparedProject::load_for_desktop(paths, second.clone()).expect("另一张画布应可加载");
        assert_eq!(
            sibling.identity.display_name, "project",
            "另一张画布必须读取相同的项目名称"
        );
        let stored = identities
            .get(&shared.workspace_id)
            .expect("应重新读取共享身份")
            .expect("身份应保留");
        assert_eq!(stored.display_name, "project", "共享项目名称应保持稳定");
        let areas = catalog
            .list_projects(&workspace.id)
            .expect("应重新读取画布目录");
        let renamed = areas
            .iter()
            .find(|area| area.id == first.id)
            .expect("应有改名画布");
        let sibling = areas
            .iter()
            .find(|area| area.id == second.id)
            .expect("应有另一张画布");
        assert_eq!(
            renamed.display_name, "Release canvas",
            "独立画布标题应保留改名结果"
        );
        assert_eq!(
            sibling.display_name, "project",
            "另一张画布标题不能被改名覆盖"
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/desktop_content_migration_tests.rs"]
mod desktop_content_migration_tests;
