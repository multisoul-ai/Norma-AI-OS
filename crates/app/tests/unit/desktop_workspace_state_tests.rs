use super::*;

/// DWS-01：首页搜索只筛选桌面工作空间，且不会把旧最近项目当成空间。
///
/// 数据构造（含关键数值的推导过程）：
///   新目录 = 2 个桌面空间；旧最近项目 = 1 个目录；搜索 "des" 后匹配数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在独立测试数据库创建 Design 和 Operations 两个桌面工作空间。
///   2. 另建一个旧项目目录并写入 recent_workspaces。
///   3. 重新加载桌面状态，设置搜索为 "des" 并筛选。
///
/// 预期结果：
///   - 正断言：仅 Design 匹配，目录总数为两个桌面工作空间。
///   - 负断言：旧项目不在桌面目录中，Operations 不匹配搜索。
#[test]
fn desktop_workspace_search_excludes_legacy_recent_projects() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let store = storage::DesktopWorkspaceStore::new(db.clone());
    let design = store.create("Design").expect("应创建 Design");
    store.create("Operations").expect("应创建 Operations");
    let old = temp.path().join("old-project");
    std::fs::create_dir(&old).expect("测试应创建旧项目目录");
    storage::RecentStore::new(db)
        .record_opened(&old, workspace::WorkspaceSource::Open)
        .expect("应记录旧项目");

    let mut state = DesktopWorkspaceUiState::load(store);
    state.search_query = "des".into();
    let filtered = state.filtered();

    assert_eq!(state.workspaces.len(), 2, "桌面目录应只含两个新工作空间");
    assert_eq!(filtered.len(), 1, "搜索 des 应只匹配一个工作空间");
    assert_eq!(filtered[0].id, design.id, "搜索结果应是 Design");
    assert!(
        !state
            .workspaces
            .iter()
            .any(|item| item.name == "old-project"),
        "旧最近项目不得自动成为桌面工作空间"
    );
    assert!(
        !filtered.iter().any(|item| item.name == "Operations"),
        "不匹配的 Operations 不得进入搜索结果"
    );
}

/// 项目画布重命名复用弹窗，保存仅更新目标画布，取消不改名。
///
/// 数据构造（含关键数值的推导过程）：
///   1 个“Workspace”空间；同项目添加 1 + 1 = 2 张画布，默认名称均为“project”。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开首张重命名 → 创建目标状态和名称输入框。
///   2. 输入“发布验收”并保存 → 后台更新存储，刷新宿主名称并关闭弹窗。
///   3. 读取存储 → 首张新名称已持久化；打开第二张重命名并取消 → 清除目标状态。
///
/// 预期结果：
///   - 正断言：首张显示并保存“发布验收”，保存和取消后弹窗均关闭。
///   - 负断言：第二张不被同步改名，空间名称不被画布重命名修改。
#[test]
fn desktop_project_canvas_rename_updates_only_target_and_can_cancel() {
    use gpui::{AppContext, TestAppContext};
    use ui::component::Root;
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project = temp.path().join("project");
    std::fs::create_dir(&project).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Workspace").expect("应创建空间");
    let first = catalog
        .add_project(&workspace.id, &project)
        .expect("应创建首张");
    let second = catalog
        .add_project(&workspace.id, &project)
        .expect("应创建第二张");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(cx.new(|_| gpui::EmptyView), window, cx)
    });
    let host = host.expect("宿主应存在");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.areas = catalog.list_projects(&workspace.id).expect("应加载画布");
        app.begin_desktop_rename(first.id.clone(), window, cx);
        assert!(
            app.desktop_workspace.rename_target.is_some(),
            "画布应能打开重命名弹窗"
        );
        let input = app
            .desktop_workspace
            .rename_input
            .clone()
            .expect("应有名称输入框");
        input.update(cx, |input, cx| input.set_value("发布验收", window, cx));
        app.save_desktop_rename(cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let app = host.read(cx);
        assert_eq!(
            app.desktop_workspace
                .areas
                .iter()
                .find(|area| area.id == first.id)
                .expect("首张应存在")
                .display_name,
            "发布验收",
            "宿主应立即显示保存后的画布名"
        );
        assert_eq!(
            app.desktop_workspace
                .areas
                .iter()
                .find(|area| area.id == second.id)
                .expect("第二张应存在")
                .display_name,
            "project",
            "同项目第二张名称不得同步修改"
        );
        assert!(
            app.desktop_workspace.rename_target.is_none(),
            "保存成功应关闭弹窗"
        );
    });
    assert_eq!(
        catalog
            .list_projects(&workspace.id)
            .expect("应读取目录")
            .iter()
            .find(|area| area.id == first.id)
            .expect("首张应存在")
            .display_name,
        "发布验收",
        "画布名必须持久化"
    );
    host.update_in(cx, |app, window, cx| {
        app.begin_desktop_rename(second.id.clone(), window, cx);
        app.cancel_desktop_rename(cx);
        assert!(
            app.desktop_workspace.rename_target.is_none(),
            "取消应关闭弹窗"
        );
    });
    assert_eq!(
        catalog
            .list_projects(&workspace.id)
            .expect("应读取目录")
            .iter()
            .find(|area| area.id == second.id)
            .expect("第二张应存在")
            .display_name,
        "project",
        "取消不得修改第二张名称"
    );
    assert_eq!(
        catalog
            .get(&workspace.id)
            .expect("应读取空间")
            .expect("空间应存在")
            .name,
        "Workspace",
        "画布重命名不得修改空间名称"
    );
}
