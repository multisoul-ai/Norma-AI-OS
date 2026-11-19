use super::*;
use gpui::{AppContext, TestAppContext};

/// 场景：冷启动服务尚未就绪时，后台任务预载不锁死重试。
/// 数据构造：已有workspace/route身份；brain settings/client尚未到达；0条已加载任务。
/// 执行过程：连续两次走生产load_staff_tasks入口。
/// 预期结果：不写永久错误、不伪造loaded/loading，服务可用后仍可重试。
#[test]
fn staff_loading_service_does_not_latch_a_list_error() {
    let dir = tempfile::tempdir().expect("isolated data");
    let mut test = TestAppContext::single();
    let (source, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            storage::DatabasePaths::isolated_test(dir.path()),
        )
    });
    source.update(cx, |app, cx| {
        app.workspace_id = Some("w".into());
        app.workspace_route_id = Some("route".into());
        app.load_staff_tasks(cx);
        app.load_staff_tasks(cx);
        assert!(
            app.staff.error.is_none(),
            "服务加载中不能变成锁死预载的永久错误"
        );
        assert!(!app.staff.loaded, "尚未读取任何快照，不能宣称加载完成");
        assert!(!app.staff.loading, "尚无后台List任务，不能残留loading");
    });
}

/// 场景：幕僚在总览聚合项目任务，异步项目到达后更新，操作跟随任务归属。
///
/// 数据构造：工作空间w有a/b两项目，分别有ta/tb；只有a先加载，画布不展开。
/// 执行过程：汇总a → b加载后再汇总 → 选tb并过滤b → 切到空工作空间。
/// 预期结果：任务不依赖画布展开；tb操作归b；过滤不删除ta；切工作空间不泄漏旧任务。
#[test]
fn staff_workspace_aggregates_late_projects_and_routes_selected_owner() {
    let dir = tempfile::tempdir().expect("isolated data");
    let paths = storage::DatabasePaths::isolated_test(dir.path());
    let mut test = TestAppContext::single();
    let (host, cx) = test.add_window_view(|_, cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let a = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths.clone(),
        )
    });
    let b = cx.new(|cx| {
        MultisoulApp::new_with_pty_factory_and_database_paths(
            cx,
            Box::new(|_, _, _, tx| Ok(Box::new(terminal::TestPtyBackend::echo(tx)))),
            paths,
        )
    });
    a.update(cx, |app, _| {
        app.staff.loaded = true;
        app.staff
            .tasks
            .insert("ta".into(), Task::new("ta", "w", "a", "Alpha"));
    });
    b.update(cx, |app, _| {
        app.staff.loaded = true;
        app.staff
            .tasks
            .insert("tb".into(), Task::new("tb", "w", "b", "Beta"));
    });
    host.update_in(cx, |app, window, cx| {
        app.screen = crate::app::AppScreen::DesktopWorkspace;
        app.staff.enabled = true;
        app.desktop_workspace.active_id = Some("w".into());
        app.desktop_workspace.areas = ["a", "b"]
            .map(|id| storage::DesktopProjectArea {
                id: id.into(),
                desktop_workspace_id: "w".into(),
                project_key: id.into(),
                root: dir.path().join(id),
                display_name: id.into(),
                added_at_unix_ms: 0,
                wallpaper: None,
            })
            .to_vec();
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "a".into()), a.clone());
        app.sync_staff_workspace(window, cx);
        assert!(
            app.staff.tasks.contains_key("ta"),
            "总览必须显示已加载项目任务"
        );
        assert!(
            !app.staff.tasks.contains_key("tb"),
            "未加载项目不能捏造任务"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "汇总不能强制展开画布"
        );
        app.desktop_workspace
            .sessions
            .insert(("w".into(), "b".into()), b.clone());
        app.sync_staff_workspace(window, cx);
        assert!(app.staff.tasks.contains_key("tb"), "迟到项目必须进入汇总");
        app.select_staff_project(Some("b".into()), window, cx);
        assert_eq!(
            app.staff.selected.as_deref(),
            Some("tb"),
            "过滤b后选择b任务"
        );
        assert_eq!(
            app.staff_project().map(|p| p.entity_id()),
            Some(b.entity_id()),
            "消息必须发给b"
        );
        assert_eq!(
            app.desktop_content_area_id().as_deref(),
            Some("b"),
            "文件面板必须跟随任务项目"
        );
        assert!(
            app.staff.tasks.contains_key("ta"),
            "过滤不能删除其他项目缓存"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "筛选不改变画布导航"
        );
        app.desktop_workspace.active_id = None;
        app.sync_staff_workspace(window, cx);
        assert!(app.staff.tasks.is_empty(), "新工作空间不能泄漏旧任务");
        assert!(app.staff.selected.is_none(), "不能保留旧任务选择");
        assert!(app.staff.project_filter.is_none(), "不能保留旧项目过滤");
        assert!(app.staff_project().is_none(), "不能向旧项目继续发送");
    });
}
