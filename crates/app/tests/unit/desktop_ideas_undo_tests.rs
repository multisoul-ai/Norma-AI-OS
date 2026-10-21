use super::*;
use gpui::TestAppContext;

/// 清空「已完成」有撤销窗口：窗口内只是不显示，撤销能恢复；窗口过后才真正删除。
///
/// 数据构造：一个工作空间，库里有两条已完成想法和一条待办。
/// 执行过程：清空「已完成」；看界面与库；撤销；再清空；让撤销窗口到期。
/// 预期结果：
///   - 正断言：清空后界面不再有已完成，但库里两条仍在；撤销后界面又有 2 条；再次清空并到期后库里已完成为 0。
///   - 负断言：待办想法在整个过程中不受影响；撤销栈在到期后为空。
#[test]
fn clearing_done_ideas_can_be_undone_until_the_window_expires() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Undo")
        .expect("应创建桌面空间");
    let ideas = storage::IdeasStore::new(paths.app_db_path().to_owned());
    for (id, done) in [("d1", true), ("d2", true), ("t1", false)] {
        ideas
            .add(&workspace.id, id, id, storage::IdeaSource::Chat)
            .expect("应记下");
        if done {
            ideas
                .begin_dispatch(&workspace.id, id, &format!("r-{id}"))
                .expect("不应出错")
                .expect("应开始派发");
            ideas
                .mark_assigned(&workspace.id, id, "area", 1, None)
                .expect("不应出错")
                .expect("应指派");
            assert!(ideas
                .complete_by_task_ref(&format!("r-{id}"))
                .expect("应完成"));
        }
    }
    let mut test = TestAppContext::single();
    let (host, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    host.update(cx, |app, cx| {
        app.screen = crate::app::AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.refresh_ideas(cx);
    });
    cx.run_until_parked();
    let done_on_screen = |cx: &mut gpui::VisualTestContext| {
        host.update(cx, |app, cx| app.ideas_screen(cx).done.len())
    };
    let done_in_db = || {
        ideas
            .list(&workspace.id)
            .expect("应列出")
            .iter()
            .filter(|idea| idea.state == workspace::ideas::IdeaState::Done)
            .count()
    };
    assert_eq!(done_on_screen(cx), 2, "起始应有 2 条已完成");

    host.update(cx, |app, cx| app.clear_done_ideas(cx));
    assert_eq!(done_on_screen(cx), 0, "清空后界面不再显示已完成");
    assert_eq!(done_in_db(), 2, "撤销窗口内库里的数据还在");

    host.update(cx, |app, cx| app.undo_last_idea_action(cx));
    assert_eq!(done_on_screen(cx), 2, "撤销后应恢复显示");

    host.update(cx, |app, cx| {
        app.clear_done_ideas(cx);
        app.expire_idea_undo(cx);
    });
    cx.run_until_parked();
    assert_eq!(done_in_db(), 0, "撤销窗口到期后才真正删除");
    host.read_with(cx, |app, _| {
        assert!(
            app.desktop_workspace.ideas_undo.is_empty(),
            "到期后撤销栈应清空"
        );
        assert!(app.desktop_workspace.ideas_pending_purge.is_empty());
    });
    assert!(
        ideas
            .list(&workspace.id)
            .expect("应列出")
            .iter()
            .any(|idea| idea.id == "t1"),
        "待办不应受影响"
    );
}
