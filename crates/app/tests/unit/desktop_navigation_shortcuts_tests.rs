use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, TestAppContext};
use ui::component::Root;

/// 从真实项目焦点走按键路由：创建/添加归宿主，项目前后循环直达画布，两级返回保留动画。
///
/// 数据构造：一个空间、两个项目A/B，加入顺序显式设为0/1（避免毫秒相同后ID排序）；
///   窗口1280×800，减少动态效果关闭，空项目不启动Agent任务。
/// 执行过程：目录添加无操作 → 创建并取消 → 打开空间/A → 添加/创建表单 →
///   子项目Settings内尝试创建、模态中尝试切换 → 取消 → A到B、B到A、A到B → 项目总览 → 工作空间列表。
/// 预期结果：表单只属于host，不重建子项目；模态不切背景；前后循环不出空间；
///   每次导航有transition；取消不保存空间，列表不残留展开项目。
#[test]
fn desktop_navigation_shortcuts_work_from_project_focus() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Shortcut review").expect("创建空间");
    let a_root = temp.path().join("a");
    let b_root = temp.path().join("b");
    std::fs::create_dir(&a_root).unwrap();
    std::fs::create_dir(&b_root).unwrap();
    let a = catalog.add_project(&workspace.id, &a_root).unwrap();
    let b = catalog.add_project(&workspace.id, &b_root).unwrap();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(app, window, cx)
    });
    let host = host.unwrap();
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, _| window.focus(&app.focus_handle));
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("cmd-shift-a");
    host.update(cx, |app, _| {
        assert!(
            !app.desktop_workspace.add_project_open,
            "尚未选择空间不能添加到上次空间"
        )
    });
    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.create_input.is_some(),
            "目录Cmd+N必须打开命名表单"
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(catalog.list().unwrap().len(), 1, "取消命名不能新增空间");
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = false;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        for area in &mut app.desktop_workspace.areas {
            area.added_at_unix_ms = if area.id == a.id { 0 } else { 1 };
        }
        app.expand_desktop_project_area(a.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    let session = host.update(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace.id.clone(), a.id.clone())].clone()
    });
    session.update_in(cx, |app, window, cx| {
        app.settings_panel.open = true;
        app.navigate_desktop_shortcut(
            crate::shortcuts::ShortcutCommandId::NewWorkspace,
            window,
            cx,
        );
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.create_input.is_none(),
            "子项目Settings开启时不得委托宿主弹出新建"
        )
    });
    session.update(cx, |app, cx| {
        app.settings_panel.open = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("cmd-shift-a");
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        assert!(
            app.desktop_workspace.add_project_open,
            "项目内的添加快捷键必须交给宿主"
        );
        let session = &app.desktop_workspace.sessions[&(workspace.id.clone(), a.id.clone())];
        assert!(
            !session.read(cx).desktop_workspace.add_project_open,
            "不能在子项目创建第二个选择器"
        );
    });
    cx.simulate_keystrokes("cmd-right");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&a.id),
            "添加弹窗未关时不能切换背景项目"
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-n");
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        assert!(
            app.desktop_workspace.create_input.is_some(),
            "项目内Cmd+N必须打开宿主命名表单"
        );
        let session = &app.desktop_workspace.sessions[&(workspace.id.clone(), a.id.clone())];
        assert!(
            session.read(cx).desktop_workspace.create_input.is_none(),
            "子项目不能持有命名表单"
        );
    });
    cx.simulate_keystrokes("cmd-shift-up");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "命名弹窗期间不能跳到目录"
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-right");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&b.id),
            "下一个项目必须直达B画布"
        );
        assert!(
            app.desktop_workspace.transition.is_some(),
            "快捷键应复用切换动画"
        );
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "不得排队先回项目列表再展开"
        );
        app.finish_desktop_zoom(window, cx);
    });
    cx.simulate_keystrokes("cmd-right");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&a.id),
            "末尾向后应循环至A"
        );
        app.finish_desktop_zoom(window, cx);
    });
    cx.simulate_keystrokes("cmd-left");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&b.id),
            "首项向前应循环至B"
        );
        assert_eq!(
            app.desktop_workspace.active_id.as_ref(),
            Some(&workspace.id),
            "项目切换不能跨空间"
        );
        app.finish_desktop_zoom(window, cx);
    });
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "返回项目列表应收回展开项目"
        );
        assert!(
            app.desktop_workspace.transition.is_some(),
            "返回项目列表必须有动画"
        );
        app.finish_desktop_zoom(window, cx);
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "项目列表仍在当前空间"
        );
    });
    cx.simulate_keystrokes("cmd-shift-up");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert!(
            app.desktop_workspace.transition.is_some(),
            "返回工作空间列表必须有动画"
        );
        app.finish_desktop_zoom(window, cx);
        assert!(
            matches!(app.screen, AppScreen::Launchpad),
            "应该回到工作空间列表"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "列表不能残留展开项目"
        );
    });
}

/// 项目内命令输入框聚焦时，⌘←/→ 属于文本编辑（行首/行尾），不得切换项目或工作空间。
///
/// 数据构造：一个空间、两个项目A/B（加入顺序0/1），窗口1280×800，A 已展开并聚焦其命令输入框。
/// 执行过程：在输入框聚焦下依次按 cmd-right、cmd-shift-right、cmd-up、cmd-shift-up。
/// 预期结果：
///   - 正断言：输入框仍保持聚焦（按键留给文本编辑）。
///   - 负断言：展开项目仍为 A、仍在桌面工作空间屏、没有发起 transition，说明没有任何导航被触发。
#[test]
fn desktop_navigation_shortcuts_yield_to_focused_command_input() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Input focus").expect("创建空间");
    let a_root = temp.path().join("a");
    let b_root = temp.path().join("b");
    std::fs::create_dir(&a_root).unwrap();
    std::fs::create_dir(&b_root).unwrap();
    let a = catalog.add_project(&workspace.id, &a_root).unwrap();
    catalog.add_project(&workspace.id, &b_root).unwrap();
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(app, window, cx)
    });
    let host = host.unwrap();
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        for area in &mut app.desktop_workspace.areas {
            area.added_at_unix_ms = if area.id == a.id { 0 } else { 1 };
        }
        app.expand_desktop_project_area(a.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    let session = host.update(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace.id.clone(), a.id.clone())].clone()
    });
    session.update_in(cx, |app, window, cx| app.focus_command_input(window, cx));
    cx.update(|window, cx| window.draw(cx).clear());
    let focused = session.update_in(cx, |app, window, cx| app.command_input_focused(window, cx));
    assert!(focused, "前置条件：子项目命令输入框必须已聚焦");
    for key in ["cmd-right", "cmd-shift-right", "cmd-up", "cmd-shift-up"] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
    }
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&a.id),
            "输入框聚焦时 ⌘+方向键不得切换项目"
        );
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "输入框聚焦时 ⌘⇧↑ 不得回到工作空间目录"
        );
        assert!(
            app.desktop_workspace.transition.is_none(),
            "输入框聚焦时不得发起任何导航动画"
        );
    });
    let still = session.update_in(cx, |app, window, cx| app.command_input_focused(window, cx));
    assert!(still, "按键应留给输入框编辑，焦点不应丢失");
}
