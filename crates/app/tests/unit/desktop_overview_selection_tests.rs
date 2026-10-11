use crate::app::{AppScreen, MultisoulApp};
use gpui::{px, size, AppContext, TestAppContext};
use ui::component::Root;

/// 项目总览中裸 ←/→ 循环移动键盘选中态，⌘+方向键进入被选中的画布。
///
/// 数据构造：一个空间、三个项目A/B/C，加入顺序显式设为0/1/2；窗口1280×800，关闭动画。
/// 执行过程：进入空间总览（无展开项目）→ → → → ← → ← → ⌘→ 。
///   1. 首次 → 无选中，从头开始，选中A
///   2. → 选中B
///   3. ← 回到A，再 ← 向前循环到C
///   4. ⌘→ 进入当前选中的C，而不是 C 的下一个项目
///
/// 预期结果：
///   - 每一步 selected 精确等于期望项目；裸方向键不展开任何项目
///   - 最终 expanded_area_id == C，且 A、B 未被展开
#[test]
fn desktop_overview_arrow_selects_and_command_arrow_enters() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Overview selection").expect("创建空间");
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        let root = temp.path().join(name);
        std::fs::create_dir(&root).unwrap();
        ids.push(catalog.add_project(&workspace.id, &root).unwrap().id);
    }
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
        for area in app.desktop_workspace.areas.iter_mut() {
            area.added_at_unix_ms = ids.iter().position(|id| id == &area.id).unwrap() as i64;
        }
        window.focus(&app.focus_handle);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let selected = |cx: &mut gpui::VisualTestContext| {
        host.update(cx, |app, _| app.desktop_workspace.selected_area_id.clone())
    };
    for (key, expected) in [("right", 0), ("right", 1), ("left", 0), ("left", 2)] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        assert_eq!(
            selected(cx).as_ref(),
            Some(&ids[expected]),
            "按 {key} 后选中项应为 {expected} 号项目"
        );
        host.update(cx, |app, _| {
            assert!(
                app.desktop_workspace.expanded_area_id.is_none(),
                "裸方向键只移动选中，不得展开项目"
            )
        });
    }
    cx.simulate_keystrokes("cmd-right");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "仍在当前空间"
        );
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&ids[2]),
            "⌘→ 必须进入选中的C，而不是C的下一个项目"
        );
    });
    // 回到总览：展开过的项目不应被当作键盘选中项，⌘→ 恢复为切到下一个项目
    host.update_in(cx, |app, window, cx| {
        app.collapse_desktop_project_area(window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.selected_area_id.is_none(),
            "展开项目再返回总览后不应保留键盘选中态"
        )
    });
    cx.simulate_keystrokes("cmd-right");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&ids[0]),
            "无选中项时 ⌘→ 仍按旧语义，从上次项目C切到下一个A"
        )
    });
}

/// 项目总览中裸 ↑/↓ 在两行网格间移动选中态，⌘↓ 展开选中的项目。
///
/// 数据构造：一个空间、五个项目a..e，加入顺序显式设为0..4；窗口1280×800 →
///   总览为两行三列（行0=a/b/c，行1=d/e）；关闭动画。
/// 执行过程：进入总览 → ↓（无选中落首项a）→ ↓（同列下行到d）→ ↑（回到a）→
///   →（右移到b）→ ↓（底行缺格吸附到e）→ ⌘↓ → 收回总览 → ⌘↓。
/// 预期结果：每步 selected 等于期望项目且不展开任何项目；⌘↓ 展开选中的e；
///   收回后选中态清空，⌘↓ 无选中时回退展开当前项目e。
#[test]
fn desktop_overview_vertical_arrows_select_and_command_down_expands() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Overview vertical").expect("创建空间");
    let mut ids = Vec::new();
    for name in ["a", "b", "c", "d", "e"] {
        let root = temp.path().join(name);
        std::fs::create_dir(&root).unwrap();
        ids.push(catalog.add_project(&workspace.id, &root).unwrap().id);
    }
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
        for area in app.desktop_workspace.areas.iter_mut() {
            area.added_at_unix_ms = ids.iter().position(|id| id == &area.id).unwrap() as i64;
        }
        window.focus(&app.focus_handle);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let selected = |cx: &mut gpui::VisualTestContext| {
        host.update(cx, |app, _| app.desktop_workspace.selected_area_id.clone())
    };
    // 两行三列：行0=a/b/c（索引0/1/2），行1=d/e（索引3/4）。
    for (key, expected) in [
        ("down", 0),
        ("down", 3),
        ("up", 0),
        ("right", 1),
        ("down", 4),
    ] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        assert_eq!(
            selected(cx).as_ref(),
            Some(&ids[expected]),
            "按 {key} 后选中项应为 {expected} 号项目"
        );
        host.update(cx, |app, _| {
            assert!(
                app.desktop_workspace.expanded_area_id.is_none(),
                "裸方向键只移动选中，不得展开项目"
            )
        });
    }
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "仍在当前空间"
        );
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&ids[4]),
            "⌘↓ 必须展开选中的e"
        );
    });
    // 收回总览：选中态清空后，⌘↓ 回退展开当前项目（active_area_id 仍为 e）。
    host.update_in(cx, |app, window, cx| {
        app.collapse_desktop_project_area(window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.selected_area_id.is_none(),
            "展开项目再返回总览后不应保留键盘选中态"
        )
    });
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&ids[4]),
            "无选中项时 ⌘↓ 应展开当前项目e"
        )
    });
}
