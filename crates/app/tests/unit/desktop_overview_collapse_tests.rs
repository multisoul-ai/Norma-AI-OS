use super::*;
use crate::workspace::desktop::overview_layout::OverviewGroup;
use gpui::TestAppContext;

/// 概览栏的折叠状态随工作空间保存：折叠后写入库，重新进入该工作空间时还原。
///
/// 数据构造：一个桌面空间；宿主处于该空间，全部栏展开。
/// 执行过程：折叠「当前项目」与「其他项目」；等后台写完；读库；再把宿主的抽屉状态重置为默认并按库里的布局还原。
/// 预期结果：
///   - 正断言：折叠后立即生效（内存里的状态与布局 JSON 都带折叠位）；库里保存的布局带同样的折叠位；还原后仍是这两栏折叠。
///   - 负断言：未折叠的「需要你处理」「想法」在还原后仍展开；再次折叠同一栏的「展开」操作能把它恢复。
#[test]
fn overview_collapse_is_saved_with_the_workspace_and_restored() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let workspace = store
        .desktop_workspace_store()
        .create("Collapse")
        .expect("应创建桌面空间");
    let mut test = TestAppContext::single();
    let (host, cx) = test
        .add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store));
    host.update(cx, |app, cx| {
        app.screen = crate::app::AppScreen::DesktopWorkspace;
        app.desktop_workspace.active_id = Some(workspace.id.clone());
        app.desktop_workspace.workspaces = vec![workspace.clone()];
        app.update_overview_collapse(|collapse| collapse.toggle(OverviewGroup::Current), cx);
        app.update_overview_collapse(|collapse| collapse.toggle(OverviewGroup::Other), cx);
    });
    cx.run_until_parked();

    host.read_with(cx, |app, _| {
        let collapse = app.overview_collapse();
        assert!(collapse.is_collapsed(OverviewGroup::Current));
        assert!(collapse.is_collapsed(OverviewGroup::Other));
        assert!(!collapse.is_collapsed(OverviewGroup::Attention));
        assert!(!collapse.is_collapsed(OverviewGroup::Ideas));
    });
    let saved = storage::AppStore::initialize(&paths)
        .expect("应重新打开存储")
        .desktop_workspace_store()
        .get(&workspace.id)
        .expect("应读库")
        .expect("应存在")
        .drawer_layout_json
        .expect("折叠后应写入布局");
    let parsed = runtime::desktop_drawers::overview_layout_from_preference(Some(&saved));
    assert_eq!(parsed.collapsed, 0b1010, "库里的布局应带折叠位：{saved}");

    host.update(cx, |app, _| {
        app.desktop_workspace.drawers = runtime::desktop_drawers::DesktopDrawerState::default();
        app.desktop_workspace.workspaces[0].drawer_layout_json = Some(saved.clone());
        app.restore_left_drawer_view(&workspace.id);
    });
    host.update(cx, |app, cx| {
        let collapse = app.overview_collapse();
        assert!(
            collapse.is_collapsed(OverviewGroup::Current),
            "还原后应仍折叠"
        );
        assert!(
            collapse.is_collapsed(OverviewGroup::Other),
            "还原后应仍折叠"
        );
        assert!(
            !collapse.is_collapsed(OverviewGroup::Ideas),
            "未折叠的栏应保持展开"
        );
        app.update_overview_collapse(|collapse| collapse.toggle(OverviewGroup::Current), cx);
        assert!(
            !app.overview_collapse().is_collapsed(OverviewGroup::Current),
            "再次点击应展开"
        );
    });
}
