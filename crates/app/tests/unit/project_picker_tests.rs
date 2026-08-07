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
                window_options("Project Picker Test".to_string(), bounds),
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

/// Project Picker 回归：组件 Root 窗口选择目录后必须打开项目窗口。
///
/// 数据构造（含关键数值的推导过程）：
///   初始窗口数 = 1（Root<MultisoulApp> Launchpad）。
///   选择目录数 = 1（workspace）。
///   成功后的窗口数 = 1（新项目窗口 1 - 已关闭 Launchpad 1 + 初始窗口 1）。
///   测试模式项目库 = workspace/.multisoul/project.test.sqlite。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用隔离 DatabasePaths 创建真实 gpui-component Root 窗口。
///   2. 向项目选择回调注入 `Some([workspace])`，模拟用户在原生文件选择器确认目录。
///   3. 推进 GPUI executor，等待回调创建项目窗口并关闭 Launchpad。
///   4. 查询 WorkspaceWindowRegistry 与项目数据库文件。
///
/// 预期结果：
///   - 正断言：canonical workspace 已注册项目窗口。
///   - 正断言：测试项目数据库 `project.test.sqlite` 已创建。
///   - 正断言：窗口总数仍为 1，证明发生了 Launchpad → 项目窗口替换。
///   - 负断言：不得创建生产数据库 `project.sqlite`。
#[test]
fn test_project_picker_selection_opens_workspace_from_component_root_window() {
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
    assert!(
        registered,
        "选择目录后 canonical workspace 应注册项目窗口；未注册说明回调被静默跳过"
    );
    assert!(
        workspace
            .join(".multisoul")
            .join("project.test.sqlite")
            .is_file(),
        "测试模式选择目录后必须创建 project.test.sqlite"
    );
    assert_eq!(
        test_cx.windows().len(),
        1,
        "打开项目后应关闭 Launchpad，仅保留 1 个项目窗口"
    );
    assert!(
        !workspace.join(".multisoul").join("project.sqlite").exists(),
        "测试模式不得因 Open Project 创建生产 project.sqlite"
    );
}
