use crate::app::{AppScreen, MultisoulApp};
use gpui::{
    px, size, App, AppContext as _, Bounds, Context, Global, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowOptions,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Default)]
pub(crate) struct WorkspaceWindowRegistry {
    windows_by_root: HashMap<PathBuf, WindowHandle<MultisoulApp>>,
}

impl Global for WorkspaceWindowRegistry {}

impl WorkspaceWindowRegistry {
    pub(crate) fn register(&mut self, root: PathBuf, handle: WindowHandle<MultisoulApp>) {
        self.windows_by_root.insert(root, handle);
    }

    pub(crate) fn unregister(&mut self, root: &Path) {
        self.windows_by_root.remove(root);
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, root: &Path) -> bool {
        self.windows_by_root.contains_key(root)
    }

    pub(crate) fn window_for(&self, root: &Path) -> Option<WindowHandle<MultisoulApp>> {
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
}

pub(crate) fn open_multisoul_window(
    state: InitialWindowState,
    cx: &mut App,
) -> anyhow::Result<WindowHandle<MultisoulApp>> {
    let title = title_for_initial_state(&state);
    let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
    let handle = cx.open_window(window_options(title, bounds), |window, cx| {
        cx.new(|cx| {
            let mut app = MultisoulApp::new(cx);
            if let InitialWindowState::Workspace(path) = state {
                app.open_workspace_path(path, window, cx);
            }
            app
        })
    })?;
    Ok(handle)
}

pub(crate) fn title_for_initial_state(state: &InitialWindowState) -> String {
    match state {
        InitialWindowState::Launchpad => "Multisoul".to_string(),
        InitialWindowState::Workspace(path) => {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .unwrap_or("Workspace");
            format!("{name} - Multisoul")
        }
    }
}

fn window_options(title: String, bounds: Bounds<gpui::Pixels>) -> WindowOptions {
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
    pub(crate) fn unregister_workspace_window_if_needed(&self, cx: &mut App) {
        if let Some(root) = self.workspace_state.current_root.as_deref() {
            cx.default_global::<WorkspaceWindowRegistry>()
                .unregister(root);
        }
    }

    pub(crate) fn open_new_launchpad_window(cx: &mut Context<Self>) {
        if let Err(error) = open_multisoul_window(InitialWindowState::Launchpad, cx) {
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
        let Some(root) = canonical_workspace_root(&path, &mut self.workspace_state.error) else {
            cx.notify();
            return;
        };

        let existing = cx
            .default_global::<WorkspaceWindowRegistry>()
            .window_for(root.as_path());
        if let Some(handle) = existing {
            let activated = handle
                .update(cx, |_app, window, _cx| window.activate_window())
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
        match open_multisoul_window(InitialWindowState::Workspace(root.clone()), cx) {
            Ok(handle) => {
                tracing::info!(
                    stage = "workspace_open_project_window",
                    path = %root.display(),
                    elapsed_ms = open_start.elapsed().as_secs_f64() * 1000.0,
                    "workspace_open_timing"
                );
                let opened = handle
                    .read(cx)
                    .map(|app| app.workspace_state.current_root.as_deref() == Some(root.as_path()))
                    .unwrap_or(false);
                if opened {
                    cx.default_global::<WorkspaceWindowRegistry>()
                        .register(root, handle);
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

        let handle = WindowHandle::<MultisoulApp>::new(gpui::WindowId::from(7));
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

    /// WN-2: workspace 窗口标题应使用项目名，启动页标题保持应用名。
    ///
    /// 数据构造（含关键数值的推导过程）：
    ///   launchpad title = "Multisoul"
    ///   workspace path = /tmp/acme
    ///   file_name(path) = "acme"
    ///   expected workspace title = "acme - Multisoul"
    ///
    /// 执行过程（逐步说明系统如何处理）：
    ///   1. title_for_initial_state(Launchpad)
    ///   2. title_for_initial_state(Workspace(/tmp/acme))
    ///   3. 对比两个标题
    ///
    /// 预期结果：
    ///   - 正断言：启动页标题为 Multisoul
    ///   - 正断言：项目窗口标题包含项目名和 Multisoul
    ///   - 负断言：项目窗口不应继续只显示 Multisoul
    #[test]
    fn test_window_titles_distinguish_workspace_from_launchpad() {
        let launchpad = title_for_initial_state(&InitialWindowState::Launchpad);
        let workspace =
            title_for_initial_state(&InitialWindowState::Workspace(PathBuf::from("/tmp/acme")));

        assert_eq!(launchpad, "Multisoul", "Launchpad 窗口标题应保持应用名");
        assert_eq!(
            workspace, "acme - Multisoul",
            "项目窗口标题应显示项目名，方便 macOS 窗口切换"
        );
        assert_ne!(
            workspace, "Multisoul",
            "项目窗口不应继续只显示 Multisoul，否则多个项目窗口无法区分"
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
