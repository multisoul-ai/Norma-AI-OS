use crate::app::{AppScreen, MultisoulApp};
use gpui::{
    px, size, App, AppContext as _, Bounds, Context, Entity, Global, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowOptions,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use ui::component::Root;

const APP_DISPLAY_NAME: &str = "Norma OS";

#[derive(Default)]
pub(crate) struct WorkspaceWindowRegistry {
    windows_by_root: HashMap<PathBuf, WindowHandle<Root>>,
}

impl Global for WorkspaceWindowRegistry {}

impl WorkspaceWindowRegistry {
    pub(crate) fn register(&mut self, root: PathBuf, handle: WindowHandle<Root>) {
        self.windows_by_root.insert(root, handle);
    }

    pub(crate) fn unregister(&mut self, root: &Path) {
        self.windows_by_root.remove(root);
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, root: &Path) -> bool {
        self.windows_by_root.contains_key(root)
    }

    pub(crate) fn window_for(&self, root: &Path) -> Option<WindowHandle<Root>> {
        self.windows_by_root.get(root).copied()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.windows_by_root.len()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum InitialWindowState {
    Launchpad,
    Workspace(PathBuf),
    ClonedWorkspace(PathBuf),
}

struct OpenedMultisoulWindow {
    root_handle: WindowHandle<Root>,
    app: Entity<MultisoulApp>,
}

fn open_multisoul_window(
    state: InitialWindowState,
    database_paths: storage::DatabasePaths,
    test_database_guard: Option<std::sync::Arc<crate::database::TestDatabaseGuard>>,
    cx: &mut App,
) -> anyhow::Result<OpenedMultisoulWindow> {
    let app_store = storage::AppStore::initialize(&database_paths)?;
    let title = title_for_initial_state(&state);
    let (width, height) = initial_window_dimensions(visual_preview_name());
    let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
    let mut app_entity = None;
    let existing = match &state {
        InitialWindowState::Workspace(path) | InitialWindowState::ClonedWorkspace(path) => {
            super::sessions::for_root(database_paths.app_db_path(), path, cx)
        }
        InitialWindowState::Launchpad => None,
    };
    let root_handle = cx.open_window(window_options(title, bounds), |window, cx| {
        let app = if let Some(app) = existing {
            app.update(cx, |app, cx| app.attach_project_window(window, cx));
            app
        } else {
            cx.new(|cx| {
                let mut app =
                    MultisoulApp::new_with_database_context(cx, database_paths, app_store);
                app.test_database_guard = test_database_guard;
                #[cfg(feature = "e2e-test-support")]
                if visual_preview_name().as_deref() == Some("github-clone") {
                    app.open_clone_project_dialog(window, cx);
                }
                app.start_dialogue_turn_loop(window, cx);
                app.start_cef_message_pump_loop(window, cx);
                match state {
                    InitialWindowState::Launchpad => {}
                    InitialWindowState::Workspace(path) => {
                        app.open_workspace_path(path, window, cx);
                    }
                    InitialWindowState::ClonedWorkspace(path) => {
                        app.open_workspace_path_with_source(
                            path,
                            workspace::WorkspaceSource::Clone,
                            window,
                            cx,
                        );
                    }
                }
                app
            })
        };
        app_entity = Some(app.clone());
        window.focus(&app.read(cx).focus_handle);
        let root_focus = app.read(cx).focus_handle.clone();
        window.defer(cx, move |window, _cx| {
            root_focus.focus(window);
            window.activate_window();
        });
        cx.new(|cx| Root::new(app, window, cx))
    })?;
    let app = app_entity.ok_or_else(|| {
        anyhow::anyhow!("production window should create a nested MultisoulApp entity")
    })?;
    Ok(OpenedMultisoulWindow { root_handle, app })
}

#[cfg(feature = "e2e-test-support")]
pub(crate) fn visual_preview_name() -> Option<String> {
    std::env::var("NORMA_VISUAL_PREVIEW").ok()
}

#[cfg(not(feature = "e2e-test-support"))]
pub(crate) fn visual_preview_name() -> Option<String> {
    None
}

pub(crate) fn initial_window_dimensions(visual_preview: Option<String>) -> (f32, f32) {
    if visual_preview.as_deref() == Some("orchestration") {
        (1440.0, 900.0)
    } else {
        (1280.0, 800.0)
    }
}

pub(crate) fn title_for_initial_state(state: &InitialWindowState) -> String {
    match state {
        InitialWindowState::Launchpad => APP_DISPLAY_NAME.to_string(),
        InitialWindowState::Workspace(path) | InitialWindowState::ClonedWorkspace(path) => {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or("Workspace");
            format!("{name} - {APP_DISPLAY_NAME}")
        }
    }
}

pub(crate) fn window_options(title: String, bounds: Bounds<gpui::Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: None,
        }),
        window_background: WindowBackgroundAppearance::Blurred,
        ..Default::default()
    }
}

impl MultisoulApp {
    #[cfg(feature = "e2e-test-support")]
    pub fn reopen_project_window_for_e2e(
        app: &Entity<Self>,
        path: PathBuf,
        cx: &mut App,
    ) -> anyhow::Result<(Entity<MultisoulApp>, gpui::AnyWindowHandle)> {
        let (paths, guard) = app.read_with(cx, |app, _| {
            (app.database_paths.clone(), app.test_database_guard.clone())
        });
        let opened = open_multisoul_window(InitialWindowState::Workspace(path), paths, guard, cx)?;
        Ok((opened.app, opened.root_handle.into()))
    }
    pub(crate) fn unregister_workspace_window_if_needed(&mut self, cx: &mut App) {
        if let (Some(workspace_id), Some(route_id)) = (
            self.workspace_id.as_deref(),
            self.workspace_route_id.as_deref(),
        ) {
            crate::norma::endpoint::unregister_workspace(workspace_id, route_id, cx);
            self.agent_hook_server.unregister(cx);
            self.runtime.set_agent_hook_config(None);
        }
        if let Some(root) = self.workspace_state.current_root.as_deref() {
            let registry = cx.default_global::<WorkspaceWindowRegistry>();
            if registry.window_for(root).map(Into::into) == self.project_window {
                registry.unregister(root);
            }
        }
    }

    pub(crate) fn open_new_launchpad_window(&self, cx: &mut Context<Self>) {
        if let Err(error) = open_multisoul_window(
            InitialWindowState::Launchpad,
            self.database_paths.clone(),
            self.test_database_guard.clone(),
            cx,
        ) {
            tracing::warn!(%error, "failed to open launchpad window");
        }
    }

    pub(crate) fn open_workspace_in_project_window(
        &mut self,
        path: PathBuf,
        close_current_launchpad: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.open_workspace_in_project_window_with_state(
            path,
            close_current_launchpad,
            false,
            window,
            cx,
        );
    }

    pub(super) fn open_workspace_in_project_window_with_state(
        &mut self,
        path: PathBuf,
        close_current_launchpad: bool,
        cloned: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = canonical_workspace_root(&path, &mut self.workspace_state.error) else {
            cx.notify();
            return;
        };

        let existing = cx
            .default_global::<WorkspaceWindowRegistry>()
            .window_for(root.as_path());
        if let Some(handle) = existing {
            let activated = handle
                .update(cx, |_root, window, _cx| window.activate_window())
                .is_ok();
            if !activated {
                cx.default_global::<WorkspaceWindowRegistry>()
                    .unregister(root.as_path());
            }
            if close_current_launchpad && matches!(self.screen, AppScreen::Launchpad) {
                window.remove_window();
            }
            if activated {
                return;
            }
        }

        let open_start = Instant::now();
        let initial_state = if cloned {
            InitialWindowState::ClonedWorkspace(root.clone())
        } else {
            InitialWindowState::Workspace(root.clone())
        };
        match open_multisoul_window(
            initial_state,
            self.database_paths.clone(),
            self.test_database_guard.clone(),
            cx,
        ) {
            Ok(opened_window) => {
                tracing::info!(
                    stage = "workspace_open_project_window",
                    path = %root.display(),
                    elapsed_ms = open_start.elapsed().as_secs_f64() * 1000.0,
                    "workspace_open_timing"
                );
                let opened = opened_window
                    .app
                    .read(cx)
                    .workspace_state
                    .current_root
                    .as_deref()
                    == Some(root.as_path());
                if opened {
                    cx.default_global::<WorkspaceWindowRegistry>()
                        .register(root, opened_window.root_handle);
                    if close_current_launchpad && matches!(self.screen, AppScreen::Launchpad) {
                        window.remove_window();
                    }
                } else {
                    self.workspace_state.error =
                        Some("Could not initialize project window.".to_string());
                    cx.notify();
                }
            }
            Err(error) => {
                tracing::warn!(%error, "failed to open workspace window");
                self.workspace_state.error =
                    Some(format!("Could not open project window: {error}"));
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod visual_acceptance_tests {
    use super::initial_window_dimensions;

    /// Orchestration 真实 App 验收：仅 canonical 预览请求 1440×900 规范 viewport
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   normal window = 1280×800（现有生产默认值）。
    ///   orchestration acceptance = 1440×900（SPEC 与 headless 布局证据的固定 viewport）。
    ///   size delta = (1440 - 1280, 900 - 800) = (160, 100)px。
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 无视觉预览名称 → 使用生产默认窗口。
    ///   2. Voice 视觉预览名称 → 继续使用生产默认窗口。
    ///   3. Orchestration 视觉预览名称 → 请求 canonical 1440×900 窗口。
    ///
    /// 预期结果：
    ///   - 正断言：Orchestration 验收尺寸精确为 1440×900。
    ///   - 负断言：普通与其他预览不得放大生产窗口。
    #[test]
    fn orchestration_visual_preview_requests_canonical_window_dimensions() {
        assert_eq!(
            initial_window_dimensions(Some("orchestration".to_string())),
            (1440.0, 900.0),
            "orchestration visual preview must request the canonical 1440x900 viewport"
        );
        assert_eq!(
            initial_window_dimensions(None),
            (1280.0, 800.0),
            "normal startup must preserve the existing 1280x800 production default"
        );
        assert_eq!(
            initial_window_dimensions(Some("voice-ready".to_string())),
            (1280.0, 800.0),
            "non-orchestration previews must not inherit the larger acceptance window"
        );
    }
}

fn canonical_workspace_root(path: &Path, error_slot: &mut Option<String>) -> Option<PathBuf> {
    if !path.is_dir() {
        let message = format!("Project folder does not exist: {}", path.display());
        *error_slot = Some(message);
        return None;
    }

    match path.canonicalize() {
        Ok(root) => {
            *error_slot = None;
            Some(root)
        }
        Err(error) => {
            *error_slot = Some(format!("Could not resolve project path: {error}"));
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WN-1: registry 以 canonical workspace root 去重，重复项目不应注册第二个窗口。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   root_a = /tmp/project-a
    ///   root_b = /tmp/project-b
    ///   初始 registry count = 0
    ///   注册 root_a 后 count = 1
    ///   重复查询 root_a 应命中，查询 root_b 应未命中
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 构造空 WorkspaceWindowRegistry
    ///   2. 不注册任何窗口时查询 root_a/root_b
    ///   3. 使用一个占位 WindowHandle 注册 root_a
    ///   4. 查询 root_a/root_b 并注销 root_a
    ///
    /// 预期结果：
    ///   - 正断言：注册后 root_a 存在且 registry count = 1
    ///   - 正断言：注销后 root_a 不存在且 registry count = 0
    ///   - 负断言：root_b 不应被 root_a 的注册误命中
    #[test]
    fn test_workspace_window_registry_tracks_one_window_per_root() {
        let root_a = PathBuf::from("/tmp/project-a");
        let root_b = PathBuf::from("/tmp/project-b");
        let mut registry = WorkspaceWindowRegistry::default();

        assert!(!registry.contains(&root_a), "空 registry 不应命中 root_a");
        assert_eq!(registry.len(), 0, "空 registry 的窗口数量应为 0");

        let handle = WindowHandle::<Root>::new(gpui::WindowId::from(7));
        registry.register(root_a.clone(), handle);

        assert!(
            registry.contains(&root_a),
            "注册 root_a 后，重复打开同一项目应能命中已有窗口"
        );
        assert!(
            !registry.contains(&root_b),
            "注册 root_a 不应让不同项目 root_b 被误判为已打开"
        );
        assert_eq!(
            registry.len(),
            1,
            "注册一个项目窗口后 registry count 应为 1"
        );

        registry.unregister(&root_a);

        assert!(
            !registry.contains(&root_a),
            "项目窗口关闭后应从 registry 移除 root_a"
        );
        assert_eq!(
            registry.len(),
            0,
            "注销唯一项目窗口后 registry count 应回到 0"
        );
    }

    /// WN-2: workspace 窗口标题应使用项目名，启动页标题保持 Norma OS 应用名。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   launchpad title = "Norma OS"
    ///   workspace path = /tmp/acme
    ///   file_name(path) = "acme"
    ///   expected workspace title = "acme - Norma OS"
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. title_for_initial_state(Launchpad)
    ///   2. title_for_initial_state(Workspace(/tmp/acme))
    ///   3. 对比两个标题
    ///
    /// 预期结果：
    ///   - 正断言：启动页标题为 Norma OS
    ///   - 正断言：项目窗口标题包含项目名和 Norma OS
    ///   - 负断言：项目窗口不应继续只显示旧品牌 Multisoul
    #[test]
    fn test_window_titles_distinguish_workspace_from_launchpad() {
        let launchpad = title_for_initial_state(&InitialWindowState::Launchpad);
        let workspace =
            title_for_initial_state(&InitialWindowState::Workspace(PathBuf::from("/tmp/acme")));

        assert_eq!(
            launchpad, "Norma OS",
            "Launchpad 窗口标题应保持 Norma OS 应用名"
        );
        assert_eq!(
            workspace, "acme - Norma OS",
            "项目窗口标题应显示项目名，方便 macOS 窗口切换"
        );
        assert_ne!(
            workspace, "Multisoul",
            "项目窗口不应继续只显示旧品牌 Multisoul，否则重命名不完整"
        );
    }

    /// WN-3: canonical_workspace_root 对缺失路径应写入错误且不返回 root。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   missing path = temp/missing，未创建目录，因此 is_dir=false
    ///   初始 error = None
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. 调用 canonical_workspace_root(missing)
    ///   2. 查询返回值与 error slot
    ///
    /// 预期结果：
    ///   - 正断言：返回 None 且 error 包含 missing
    ///   - 负断言：缺失路径不应产生 Some(root)
    #[test]
    fn test_canonical_workspace_root_rejects_missing_project() {
        let temp = tempfile::tempdir().expect("test should create temp dir");
        let missing = temp.path().join("missing");
        let mut error = None;

        let root = canonical_workspace_root(&missing, &mut error);

        assert!(root.is_none(), "缺失项目目录不应返回 canonical root");
        assert!(
            error.as_deref().unwrap_or_default().contains("missing"),
            "缺失项目目录应写入包含路径名的错误，实际: {:?}",
            error
        );
        assert_ne!(root, Some(missing), "缺失路径不应被当作可打开项目 root");
    }
}

#[cfg(test)]
#[path = "../../tests/unit/windowing_feature_guide_tests.rs"]
mod feature_guide_tests;
