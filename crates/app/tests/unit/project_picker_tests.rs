use super::*;
use crate::workspace::windowing::{window_options, WorkspaceWindowRegistry};
use gpui::{px, size, AppContext as _, Bounds, Entity, TestAppContext, WindowHandle};
use ui::component::Root;

fn open_component_root_launchpad(
    test_cx: &mut TestAppContext,
    database_paths: storage::DatabasePaths,
) -> (WindowHandle<Root>, Entity<MultisoulApp>) {
    let app_store =
        storage::AppStore::initialize(&database_paths).expect("测试 AppStore 应完成迁移");
    test_cx.update(|cx| {
        ui::init_components(cx);
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        let mut app_entity = None;
        let root_handle = cx
            .open_window(
                window_options("Project Picker Test".to_string(), bounds, None),
                |window, cx| {
                    let app = cx.new(|cx| {
                        MultisoulApp::new_with_database_context(cx, database_paths, app_store)
                    });
                    app_entity = Some(app.clone());
                    cx.new(|cx| Root::new(app, window, cx))
                },
            )
            .expect("测试应创建由 gpui-component Root 包裹的 Launchpad 窗口");
        let app = app_entity.expect("Launchpad 窗口应创建嵌套的 MultisoulApp");
        (root_handle, app)
    })
}

/// Project Picker 回归：组件 Root 窗口选择目录后进入工作空间归属选择。
///
/// 数据构造（含关键数值的推导过程）：
///   初始窗口数 = 1（Root<MultisoulApp> Launchpad）。
///   选择目录数 = 1（workspace）。
///   成功后的窗口数 = 1（归属选择仍在当前窗口）。
///   未选定目标工作空间前创建的项目数据库数 = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用隔离 DatabasePaths 创建真实 gpui-component Root 窗口。
///   2. 向项目选择回调注入 `Some([workspace])`，模拟用户在原生文件选择器确认目录。
///   3. 推进 GPUI executor，等待回调保存待归属目录。
///   4. 查询待归属路径、窗口注册表与项目数据库文件。
///
/// 预期结果：
///   - 正断言：待归属路径是用户选中的目录，窗口总数仍为 1。
///   - 负断言：没有注册旧项目窗口，也没有提前创建项目数据库。
#[test]
fn test_project_picker_selection_awaits_desktop_workspace_destination() {
    let temp = tempfile::tempdir().expect("Project Picker 测试应创建隔离目录");
    let workspace = temp.path().join("selected-workspace");
    std::fs::create_dir(&workspace).expect("Project Picker 测试应创建待选择 workspace");
    let database_root = temp.path().join("test-database");
    let database_paths = storage::DatabasePaths::isolated_test(&database_root);
    let mut test_cx = TestAppContext::single();
    let (root_handle, app) = open_component_root_launchpad(&mut test_cx, database_paths);
    let (sender, receiver) = oneshot::channel();

    root_handle
        .update(&mut test_cx, |_root, window, cx| {
            app.update(cx, |app, cx| {
                app.handle_project_picker_receiver(receiver, window, cx);
            });
        })
        .expect("Project Picker 测试应从 Root 窗口启动异步选择回调");
    sender
        .send(Ok(Some(vec![workspace.clone()])))
        .expect("Project Picker 测试应把单个 workspace 选择结果交给回调");
    test_cx.run_until_parked();

    let canonical_workspace = workspace
        .canonicalize()
        .expect("测试 workspace 应可 canonicalize");
    let registered = test_cx.update(|cx| {
        cx.default_global::<WorkspaceWindowRegistry>()
            .contains(&canonical_workspace)
    });
    let pending = app.read_with(&test_cx, |app, _| {
        app.desktop_workspace.pending_project_path.clone()
    });
    assert_eq!(
        pending,
        Some(workspace.clone()),
        "选择目录后应等待用户指定工作空间"
    );
    assert!(!registered, "归属未确定前不得注册旧项目窗口");
    assert!(
        !workspace
            .join(".multisoul")
            .join("project.test.sqlite")
            .exists(),
        "归属未确定前不得创建旧项目数据库"
    );
    assert_eq!(
        test_cx.windows().len(),
        1,
        "选择目标工作空间前应保留当前窗口"
    );
    assert!(
        !workspace.join(".multisoul").join("project.sqlite").exists(),
        "测试模式不得因 Open Project 创建生产 project.sqlite"
    );
}
