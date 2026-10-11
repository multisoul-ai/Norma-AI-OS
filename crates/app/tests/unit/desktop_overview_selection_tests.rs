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
    // 回到总览：自动选中刚展开的C，一次裸 → 就移动到A。
    host.update_in(cx, |app, window, cx| {
        app.collapse_desktop_project_area(window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.selected_area_id.as_ref(),
            Some(&ids[2]),
            "返回总览应自动高亮刚展开的C"
        );
    });
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert_eq!(
        selected(cx).as_ref(),
        Some(&ids[0]),
        "返回总览后一次右方向键就应从C循环移动到A"
    );
    cx.simulate_keystrokes("cmd-right");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&ids[0]),
            "⌘→ 应展开一次方向键选中的A"
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
///   收回后自动选中e，一次↑移动到b，⌘↓展开b。
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
    // 收回总览：恢复e的高亮，一次↑就移动到同列上一行的b。
    host.update_in(cx, |app, window, cx| {
        app.collapse_desktop_project_area(window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.selected_area_id.as_ref(),
            Some(&ids[4]),
            "返回总览应自动高亮刚展开的e"
        );
    });
    cx.simulate_keystrokes("up");
    cx.run_until_parked();
    assert_eq!(
        selected(cx).as_ref(),
        Some(&ids[1]),
        "返回总览后一次上方向键应从e移动到同列的b"
    );
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&ids[1]),
            "⌘↓ 应展开一次上方向键选中的b"
        )
    });
}

/// Cmd+↑ 返回总览时立即高亮原项目，返回动画不吞掉第一下方向键。
///
/// 数据构造：一个空间、三个项目a/b/c；1280×800窗口，动画开启，先展开b。
///   加入顺序0/1/2对应两行两列：顶行a/b，底行c。
/// 执行过程：从子项目焦点按Cmd+↑，在动画未完成时按←；完成动画后再次Cmd+↑，
///   按Cmd+↓进入a；再返回总览并立即按↓，完成动画后确认进入c；
///   从目录直接展开b，未落定即Cmd+↑后按←，再真正返回目录。
/// 预期结果：返回立即选中b；一次←选择a且不展开；重复Cmd+↑不重置浏览选择；
///   一次↓从a选择c，动画完成不覆盖选择，Cmd+↓进入对应项目；
///   目录直入的返回动画同样立即响应，但已展开项目与真正的目录不接收总览方向键。
#[test]
fn desktop_overview_restores_selection_and_accepts_arrows_during_return() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Overview return").expect("创建空间");
    let mut ids = Vec::new();
    for name in ["a", "b", "c"] {
        let root = temp.path().join(name);
        std::fs::create_dir(&root).expect("创建项目目录");
        ids.push(
            catalog
                .add_project(&workspace.id, &root)
                .expect("添加项目")
                .id,
        );
    }
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(app, window, cx)
    });
    let host = host.expect("窗口应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = false;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        for area in &mut app.desktop_workspace.areas {
            area.added_at_unix_ms = ids.iter().position(|id| id == &area.id).unwrap() as i64;
        }
        app.expand_desktop_project_area(ids[1].clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert!(
            app.desktop_workspace.transition.is_some(),
            "应处于返回动画中"
        );
        assert_eq!(
            app.desktop_overview_selection().as_ref(),
            Some(&ids[1]),
            "动画开始时原项目b就应处于可见选中态"
        );
    });
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_overview_selection().as_ref(),
            Some(&ids[0]),
            "返回动画期间一次左方向键应立即从b移到a"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "浏览不应展开项目"
        );
        app.finish_desktop_zoom(window, cx);
    });
    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_overview_selection().as_ref(),
            Some(&ids[0]),
            "已在总览时重复Cmd+↑不应重置用户浏览的a"
        );
    });
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&ids[0]),
            "应进入a"
        );
        app.finish_desktop_zoom(window, cx);
    });
    cx.simulate_keystrokes("cmd-up down");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_overview_selection().as_ref(),
            Some(&ids[2]),
            "返回时一次下方向键应从a移到下一行的c"
        );
        app.finish_desktop_zoom(window, cx);
    });
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    host.update(cx, |app, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_ref(),
            Some(&ids[2]),
            "应进入c"
        );
    });
    // 从目录直接进入b的镜头尚未落定时，反向返回总览也必须立即接收方向键。
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        app.show_desktop_directory(false, window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear());
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(ids[1].clone(), window, cx);
        assert!(
            app.desktop_workspace
                .transition
                .as_ref()
                .is_some_and(|transition| transition.directory_content.is_some()),
            "直接展开b应仍携带目录坐标，不能提前完成动画来掩盖反向边界"
        );
        assert!(
            !app.move_desktop_overview_selection_for_key("left", cx),
            "已展开b时裸方向键不能误操作隐藏的总览"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.simulate_keystrokes("cmd-up left");
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert!(
            app.desktop_workspace.transition.is_some(),
            "目录直入后返回总览仍应在动画中"
        );
        assert_eq!(
            app.desktop_overview_selection().as_ref(),
            Some(&ids[0]),
            "目录直入未落定就返回时，一次左方向键也应从b选中a"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "反向浏览应停在总览，不得展开a"
        );
        app.finish_desktop_zoom(window, cx);
        app.expand_desktop_project_area(ids[1].clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
        app.show_desktop_directory(false, window, cx);
        assert!(
            app.desktop_workspace.transition.is_some(),
            "真正返回目录应保留转场"
        );
        assert!(
            !app.move_desktop_overview_selection_for_key("left", cx),
            "真正返回目录期间裸方向键不能误操作隐藏的总览"
        );
        assert!(
            app.desktop_overview_selection().is_none(),
            "目录转场中不应暴露项目总览选中态"
        );
    });
}
