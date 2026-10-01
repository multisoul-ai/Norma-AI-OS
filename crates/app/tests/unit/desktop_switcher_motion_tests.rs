use super::*;
use crate::app::AppScreen;
use gpui::TestAppContext;

fn check_switcher_motion(expanded: bool, interrupt: bool) {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Motion").expect("应创建空间");
    let area = catalog
        .add_project(&workspace.id, &root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
    let (host, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.workspaces = catalog.list().expect("应读取空间");
        app.motion.reduce_motion = false;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        if expanded {
            app.expand_desktop_project_area(area.id.clone(), window, cx);
            app.finish_desktop_zoom(window, cx);
        }
        // 与 Dock 和 ⌘⇧K 共用真实入口；测试不复制导航状态机。
        app.open_desktop_switcher(window, cx);
        assert!(
            app.desktop_workspace.switcher_open,
            "应进入工作空间切换模式"
        );
        assert!(
            app.desktop_workspace.transition.is_some(),
            "Workspace 入口必须驱动镜头，不能只覆盖全屏页面"
        );
        assert_eq!(
            app.desktop_workspace.expanded_area_id, None,
            "返回期间应退出项目展开状态"
        );
        assert!(
            matches!(app.screen, AppScreen::Launchpad),
            "目标必须立即成为目录，不能先等待项目总览"
        );
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "目录返回只能有一个镜头目标，不能排队第二段动画"
        );
        let transition = app.desktop_workspace.transition.as_mut().expect("应有动画");
        let start = transition.rect();
        // 80ms = 5 × 16ms，处在原型弹簧的运动中段，尚未落定。
        transition.tick(Duration::from_millis(80));
        let midway = transition.rect();
        assert_ne!(midway, start, "返回必须产生可见的中间几何位置");
        assert!(transition.is_active(), "80ms 时不能直接结束返回动画");
        if !interrupt {
            app.finish_desktop_zoom(window, cx);
            assert!(
                matches!(app.screen, AppScreen::Launchpad),
                "落定应进入统一目录画布"
            );
            assert!(
                app.desktop_workspace.transition.is_none(),
                "目录落定不得持续重复动画"
            );
        }
        // 再次 ⌘⇧K 与 Back to Canvas 使用同一返回行为。
        app.open_desktop_switcher(window, cx);
        assert!(
            !app.desktop_workspace.switcher_open,
            "返回画布应退出切换模式"
        );
        assert!(
            app.desktop_workspace.transition.is_some(),
            "返回原画布也必须有动画"
        );
        if interrupt {
            assert_eq!(
                app.desktop_workspace.transition.as_ref().unwrap().rect(),
                midway,
                "快速反向必须保留当前视觉位置"
            );
            assert_ne!(midway, start, "反向不得重置到原起点");
        }
        app.finish_desktop_zoom(window, cx);
        assert!(
            app.desktop_workspace.transition.is_none(),
            "一次落定必须到达最终层级，不能再启动第二段动画"
        );
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "应回到原工作空间"
        );
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            expanded.then_some(area.id.as_str()),
            "只能恢复用户打开切换器前所在的层级"
        );
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "返回结束不得遗留第二次隐式导航"
        );
    });
}

/// 项目总览打开 Workspace：同一画布缩回目录，再沿镜头回到总览。
///
/// 数据构造：1 个空间、1 个普通项目、0 张卡片；关闭减少动态效果。
/// 执行过程：加载空间并落定 → 打开切换器 → 推进 80ms → 落定 → 再次切换回画布。
/// 预期结果：两个方向均有中间位置；目录与工作空间层级正确，不暗中展开项目。
#[test]
fn desktop_switcher_from_project_overview_animates_both_directions() {
    check_switcher_motion(false, false);
}

/// Agent 画布打开 Workspace：镜头直接缩入工作空间目录，返回时直接恢复原项目。
///
/// 数据构造：1 个空间、1 个已加载并展开的项目、0 张卡片；目录/项目都使用真实入口。
/// 执行过程：展开项目并落定 → 打开切换器 → 推进 80ms → 一次落定到目录 → 再次切换。
/// 预期结果：无瞬时全屏覆盖或中间导航队列；一次返回后仅原项目展开，没有第二段动画。
#[test]
fn desktop_switcher_from_agent_canvas_animates_both_levels() {
    check_switcher_motion(true, false);
}

/// 快速反向：项目返回动画未结束时再次切换，必须从当前画面继续。
///
/// 数据构造：1 个展开项目；80ms = 5 × 16ms，尚未到达目录。
/// 执行过程：打开切换器 → 推进 80ms → 再次切换 → 完成镜头。
/// 预期结果：反向瞬间几何不变、不跳回起点；最终恢复原项目且清空排队导航。
#[test]
fn desktop_switcher_reversal_preserves_current_frame() {
    check_switcher_motion(true, true);
}
