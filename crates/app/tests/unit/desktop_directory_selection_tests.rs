use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, Modifiers, TestAppContext};

const SELECTED: [&str; 12] = [
    "desktop-directory-selected-0",
    "desktop-directory-selected-1",
    "desktop-directory-selected-2",
    "desktop-directory-selected-3",
    "desktop-directory-selected-4",
    "desktop-directory-selected-5",
    "desktop-directory-selected-6",
    "desktop-directory-selected-7",
    "desktop-directory-selected-8",
    "desktop-directory-selected-9",
    "desktop-directory-selected-10",
    "desktop-directory-selected-11",
];

/// 所有工作空间目录按实际三列网格键盘浏览，跨屏高亮可见，⌘↓进入高亮项。
/// 数据：12个空空间；1280×800三列四行；关闭动画避免依赖计时；以渲染顺序取ID。
/// 执行：首个↓选0，→选1，↓选4，←选3，↑回0；再连下到9，⌘↓进入；
///   返回目录，↑应从9到6；聚焦搜索输入时按方向键/⌘↓。
/// 预期：裸方向键只移动高亮；9号滚入视野；进入的是9号空间且不展开项目；
///   返回保持空间记忆；搜索输入不触发目录导航或打开空间。
#[test]
fn desktop_directory_arrows_scroll_and_command_down_enters_selected_workspace() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    for i in 0..12 {
        store
            .desktop_workspace_store()
            .create(&format!("Workspace {i:02}"))
            .expect("创建空间");
    }
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        ui::component::Root::new(app, window, cx)
    });
    let host = host.expect("创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    let ids = host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.show_desktop_directory(false, window, cx);
        app.desktop_workspace
            .filtered()
            .iter()
            .map(|w| w.id.clone())
            .collect::<Vec<_>>()
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    for (key, index) in [
        ("down", 0),
        ("right", 1),
        ("down", 4),
        ("left", 3),
        ("up", 0),
        ("down", 3),
        ("down", 6),
        ("down", 9),
        ("up", 6),
        ("up", 3),
        ("up", 0),
        ("down", 3),
        ("down", 6),
        ("down", 9),
    ] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear());
        let selected = cx
            .debug_bounds(SELECTED[index])
            .expect("方向键应绘制目标空间选中环");
        let scroll = cx
            .debug_bounds("desktop-workspace-scroll")
            .expect("目录滚动区");
        assert!(
            selected.top() >= scroll.top() && selected.bottom() <= scroll.bottom(),
            "{index}号高亮必须滚入视野"
        );
        assert!(
            selected.top() >= px(72.0),
            "选中卡片不能被顶部62px切换器遮住：key={key} index={index} bounds={selected:?} scroll={scroll:?}"
        );
        host.read_with(cx, |app, _| {
            assert_eq!(app.screen, AppScreen::Launchpad, "裸方向键不能进入空间")
        });
    }
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_workspace.active_id.as_ref(),
            Some(&ids[9]),
            "⌘↓进入键盘选中空间"
        );
        assert_eq!(app.screen, AppScreen::DesktopWorkspace, "应进入工作空间");
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "不能越级打开项目"
        );
        app.show_desktop_directory(false, window, cx);
    });
    cx.run_until_parked();
    // 最近访问会重排目录，以返回后该空间的实际位置计算上一行。
    let expected = host.read_with(cx, |app, _| {
        let items = app.desktop_workspace.filtered();
        items
            .iter()
            .position(|w| w.id == ids[9])
            .expect("已打开空间仍在目录")
            .saturating_sub(3)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("up");
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds(SELECTED[expected]).is_some(),
        "返回后从原空间按实际布局向上移动"
    );
    let search = cx
        .debug_bounds("desktop-workspace-search")
        .expect("目录搜索输入");
    cx.simulate_click(search.center(), Modifiers::none());
    cx.simulate_keystrokes("right down cmd-down");
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.screen,
            AppScreen::Launchpad,
            "输入框聚焦时⌘↓不能误打开空间"
        )
    });

    // 同一个网格从切换器进入时，空搜索框默认聚焦也必须立即响应方向键。
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.directory.selected = None;
        app.open_desktop_switcher(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("down right");
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds(SELECTED[1]).is_some(),
        "切换器空搜索框不能吞掉方向键"
    );
    cx.simulate_input("Workspace");
    cx.simulate_keystrokes("left down cmd-down");
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.screen,
            AppScreen::Launchpad,
            "正在编辑搜索时不能误打开空间"
        );
    });
    cx.simulate_keystrokes("cmd-a backspace");
    for (width, expected) in [(800.0, 3), (500.0, 2)] {
        cx.simulate_resize(size(px(width), px(800.0)));
        host.update(cx, |app, cx| {
            app.desktop_workspace.directory.selected = Some(ids[1].clone());
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        cx.simulate_keystrokes("down");
        host.read_with(cx, |app, _| {
            assert_eq!(
                app.desktop_workspace.directory.selected.as_ref(),
                Some(&ids[expected]),
                "{width}px按实际列数向下移动"
            );
        });
    }
    host.update(cx, |app, cx| {
        app.desktop_workspace.actions_workspace_id = Some(ids[2].clone());
        cx.notify();
    });
    cx.simulate_keystrokes("right cmd-down");
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.directory.selected.as_ref(),
            Some(&ids[2]),
            "菜单打开时保持目录选中项"
        );
        assert_eq!(app.screen, AppScreen::Launchpad, "菜单打开时不能进入空间");
    });
    host.update(cx, |app, cx| {
        app.desktop_workspace.actions_workspace_id = None;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.active_id.as_ref(),
            Some(&ids[2]),
            "切换器空搜索框中的⌘↓也须打开选中空间"
        );
        assert_eq!(
            app.screen,
            AppScreen::DesktopWorkspace,
            "切换器键盘确认必须真正进入空间"
        );
    });
    host.update_in(cx, |app, window, cx| {
        app.show_desktop_directory(false, window, cx);
        app.focus_command_input(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("down right cmd-down");
    host.read_with(cx, |app, _| {
        assert_eq!(
            app.screen,
            AppScreen::Launchpad,
            "命令输入框聚焦时不能进入空间"
        );
        assert_eq!(
            app.desktop_workspace.directory.selected.as_ref(),
            Some(&ids[2]),
            "命令输入不能移动背后网格选择"
        );
    });
}
