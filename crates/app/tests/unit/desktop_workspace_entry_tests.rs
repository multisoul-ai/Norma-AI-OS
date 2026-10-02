use super::*;
use crate::app::AppScreen;
use gpui::TestAppContext;

fn check_workspace_entry(remember_in_memory: bool) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_a = temp.path().join("project-a");
    let project_b = temp.path().join("project-b");
    std::fs::create_dir(&project_a).expect("应创建项目 A");
    std::fs::create_dir(&project_b).expect("应创建项目 B");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Entry").expect("应创建工作空间");
    let area_a = catalog
        .add_project(&workspace.id, &project_a)
        .expect("应添加 A");
    let area_b = catalog
        .add_project(&workspace.id, &project_b)
        .expect("应添加 B");
    catalog
        .set_expanded_area(&workspace.id, Some(&area_a.id))
        .expect("应保存上次展开的 A");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.workspaces = catalog.list().expect("应读取空间目录");
        app.screen = AppScreen::Launchpad;
        app.motion.reduce_motion = false;
        if remember_in_memory {
            app.desktop_workspace
                .expanded_by_workspace
                .insert(workspace.id.clone(), Some(area_b.id.clone()));
        }
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert!(matches!(app.screen, AppScreen::DesktopWorkspace), "点击工作空间应进入该空间的项目总览");
        assert_eq!(app.desktop_workspace.active_id.as_deref(), Some(workspace.id.as_str()), "必须进入用户点击的工作空间");
        assert_eq!(app.desktop_workspace.areas.len(), 2, "项目 A 和 B 均应加载到总览");
        assert!(app.desktop_workspace.sessions.contains_key(&(workspace.id.clone(), area_a.id.clone())), "总览必须保留 A 的真实会话");
        assert!(app.desktop_workspace.sessions.contains_key(&(workspace.id.clone(), area_b.id.clone())), "总览必须保留 B 的真实会话");
        assert_eq!(app.desktop_workspace.expanded_area_id, None, "没有点击项目时，不得恢复磁盘或内存中上次展开的项目");
        assert_eq!(app.desktop_workspace.active_area_id, None, "工作空间入口不得暗中选择某个项目");
        assert!(app.desktop_workspace.pending_navigation.is_none(), "不得排队执行隐式的项目展开");
        assert!(matches!(app.desktop_workspace.transition.as_ref().map(|transition| &transition.subject), Some(DesktopCanvasSubject::Workspace(id)) if id == &workspace.id), "进入期间只有工作空间镜头动画");

        // 直接执行生产的落定回调，不用等待墙钟时间；这里只验证是否产生第二次导航。
        app.finish_desktop_zoom(window, cx);
        assert!(app.desktop_workspace.transition.is_none(), "工作空间落定后不得启动项目镜头造成二次闪跳");
        assert_eq!(app.desktop_workspace.expanded_area_id, None, "落定仍应停在项目总览");

        app.expand_desktop_project_area(area_b.id.clone(), window, cx);
        assert_eq!(app.desktop_workspace.expanded_area_id.as_deref(), Some(area_b.id.as_str()), "明确选择 B 后仍须正常展开 B");
        assert_ne!(app.desktop_workspace.expanded_area_id.as_deref(), Some(area_a.id.as_str()), "不能展开历史记录中的 A");
        assert!(matches!(app.desktop_workspace.transition.as_ref().map(|transition| &transition.subject), Some(DesktopCanvasSubject::Project { area_id, .. }) if area_id == &area_b.id), "显式项目展开必须保留动画");
    });
}

/// 冷加载工作空间：磁盘记录上次展开项目 A，用户只点击工作空间时必须停在项目总览。
///
/// 数据构造：1 个空间、2 个普通目录项目、0 张卡片；磁盘 expanded_area=A，内存没有历史。
/// 执行过程：调用实际打开入口 → 完成后台目录/会话加载 → 执行镜头落定 → 明确展开 B。
/// 预期结果：A、B 会话均存在；加载和落定都不自动选择 A，不产生第二段项目动画；显式 B 可展开。
#[test]
fn desktop_workspace_entry_ignores_saved_project_expansion() {
    check_workspace_entry(false);
}

/// 已访问工作空间：内存记住项目 B，磁盘记住 A，用户选择工作空间不得自动进入任意项目。
///
/// 数据构造：1 个空间、2 个普通目录项目、0 张卡片；内存 expanded=B 与磁盘 expanded=A 故意不同。
/// 执行过程：调用实际打开入口 → 完成异步加载 → 执行镜头落定 → 明确选择 B。
/// 预期结果：两个真实会话均保留；内存/磁盘历史都不能触发自动展开或二次镜头；选择 B 后才展开。
#[test]
fn desktop_workspace_entry_ignores_remembered_project_expansion() {
    check_workspace_entry(true);
}
