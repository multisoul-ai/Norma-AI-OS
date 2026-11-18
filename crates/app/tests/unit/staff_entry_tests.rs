use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, Modifiers, TestAppContext};

/// 场景：画布底栏最左的头像入口进入幕僚，幕僚页右上角的「返回画布」回到画布。
///
/// 数据构造：隔离存储中一个空间、一个空项目；窗口 1280×800；展开项目后进入画布。
/// 执行过程：绘制画布 → 读取头像入口与工作空间入口 → 点击头像 → 绘制幕僚页 →
///   读取返回按钮 → 点击返回 → 再次绘制。
/// 预期结果：
///   - 正断言：头像在底栏所有控件之左；点击后 staff.enabled 为 true；返回按钮位于窗口右半边且在顶栏内；
///     点击返回后回到画布并重新出现底栏。
///   - 负断言：顶栏不再有旧的「画布/幕僚」双按钮；幕僚页没有画布底栏；返回按钮不在窗口正中。
#[test]
fn dock_avatar_enters_staff_and_top_right_button_returns() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Staff Entry").expect("应创建空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        app.expand_desktop_project_area(area.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());

    let avatar = cx
        .debug_bounds("toolbar-staff")
        .expect("画布底栏应有幕僚头像入口");
    let workspace_button = cx
        .debug_bounds("toolbar-workspace")
        .expect("应有工作空间入口");
    let dock = cx.debug_bounds("workspace-command-dock").expect("应有底栏");
    assert!(
        avatar.right() <= workspace_button.left() && avatar.left() < dock.left() + px(16.0),
        "头像应插在底栏最左：avatar={avatar:?} workspace={workspace_button:?} dock={dock:?}"
    );
    assert!(
        cx.debug_bounds("desktop-staff-toggle").is_none()
            && cx.debug_bounds("desktop-canvas-mode").is_none(),
        "顶栏不应再有画布/幕僚双按钮"
    );

    cx.simulate_click(avatar.center(), Modifiers::none());
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(app.staff.enabled, "点击头像应进入幕僚");
        assert_eq!(
            app.screen,
            AppScreen::DesktopWorkspace,
            "幕僚仍在桌面工作区内"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("workspace-command-dock").is_none(),
        "幕僚页不显示画布底栏"
    );
    let back = cx
        .debug_bounds("staff-back-to-canvas")
        .expect("幕僚页应有返回画布");
    assert!(
        back.left() > px(640.0) && back.right() <= px(1280.0) && back.bottom() <= px(52.0),
        "返回画布应在顶栏右侧而非正中：{back:?}"
    );

    cx.simulate_click(back.center(), Modifiers::none());
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(!app.staff.enabled, "点击返回应回到画布")
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("workspace-command-dock").is_some()
            && cx.debug_bounds("toolbar-staff").is_some(),
        "回到画布后底栏与头像入口应重新出现"
    );
}
