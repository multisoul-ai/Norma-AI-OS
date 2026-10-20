use super::*;
use crate::app::background_project::PreparedProject;
use gpui::TestAppContext;
use runtime::desktop_drawers::DesktopLeftView;

/// 项目画布里按 ⌘⇧E 必须切换桌面外壳的左栏，而不是项目实例自己的抽屉状态。
///
/// 数据构造：1 个桌面工作空间、1 个展开的项目；项目实例通过 desktop_host 指向桌面外壳，左栏默认是对话。
/// 执行过程：在项目实例上触发 ⌘⇧E（toggle_run_overview）两次。
/// 预期结果：第一次后外壳左栏展开并显示运行概览；第二次回到对话；项目实例自己的抽屉状态始终不变。
#[test]
fn project_canvas_run_overview_shortcut_toggles_the_desktop_host_drawer() {
    let temp = tempfile::tempdir().expect("应创建隔离测试目录");
    let root = temp.path().join("plain-project");
    std::fs::create_dir(&root).expect("应创建普通项目");
    let root = root.canonicalize().expect("应规范化所选目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离数据库");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Test").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加普通项目");
    let prepared =
        PreparedProject::load_for_desktop(paths.clone(), area.clone()).expect("应准备项目实例");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    cx.run_until_parked();
    let session = cx.new(|cx| {
        let mut app = MultisoulApp::from_background_project(prepared, cx);
        app.desktop_host = Some(host.downgrade());
        app
    });
    host.update(cx, |app, _| {
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = vec![area.clone()];
        app.desktop_workspace.expanded_area_id = Some(area.id.clone());
        app.desktop_workspace
            .sessions
            .insert((workspace.id.clone(), area.id.clone()), session.clone());
    });
    let own_before = session.read_with(cx, |app, _| app.desktop_workspace.drawers);

    session.update_in(cx, |app, window, cx| app.toggle_run_overview(window, cx));
    let opened = host.read_with(cx, |app, _| app.desktop_workspace.drawers);
    assert_eq!(
        opened.left_view,
        DesktopLeftView::Overview,
        "第一次 ⌘⇧E 应让外壳左栏显示运行概览"
    );
    assert!(opened.left_open, "进入运行概览时外壳左栏应展开");

    session.update_in(cx, |app, window, cx| app.toggle_run_overview(window, cx));
    let closed = host.read_with(cx, |app, _| app.desktop_workspace.drawers);
    assert_eq!(
        closed.left_view,
        DesktopLeftView::Chat,
        "第二次 ⌘⇧E 应回到对话"
    );
    let own_after = session.read_with(cx, |app, _| app.desktop_workspace.drawers);
    assert_eq!(own_after, own_before, "项目实例自己的抽屉状态不能被改动");
}
