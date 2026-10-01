use super::*;
use crate::app::AppScreen;
use gpui::{div, point, prelude::*, px, size, Entity, Modifiers, Render, TestAppContext};

struct DockView(Entity<MultisoulApp>);

impl Render for DockView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().relative().size_full().child(
            self.0
                .update(cx, |app, cx| app.render_desktop_dock(window, cx)),
        )
    }
}

/// 项目 Dock：编号只代表当前空间项目，点击切换真实项目，网格返回项目总览。
///
/// 数据构造：当前空间含项目 A/B，另一空间含 C；A/C 复用目录，B 使用另一目录，
///   用不同区域 ID 验证归属；窗口 1280×800，两个编号应为 01/02，不得出现 03。
/// 执行过程：加载当前空间 → 绘制真实 Dock → 点击 02 → 完成镜头 → 再点 02 → 切到 01 → 点击总览。
/// 预期结果：只渲染 A/B；02 展开 B 并保留镜头，重复点 B 不重新缩放；01 展开 A，总览收回 A，
///   当前空间不变、C 不被展开，整个流程不依赖旧 e2e 套件。
#[test]
fn desktop_project_dock_navigates_only_the_current_workspace() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("shared-project");
    std::fs::create_dir(&root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Current").expect("应创建当前空间");
    let other = catalog.create("Other").expect("应创建其他空间");
    let a = catalog.add_project(&workspace.id, &root).expect("应添加 A");
    // 同一空间的两区域需要不同项目路径；跨空间 C 仍复用 A 的目录。
    let root_b = temp.path().join("project-b");
    std::fs::create_dir(&root_b).expect("应创建 B 目录");
    let b = catalog
        .add_project(&workspace.id, &root_b)
        .expect("应添加 B");
    let c = catalog.add_project(&other.id, &root).expect("应添加 C");
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        DockView(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.workspaces = catalog.list().expect("应读取空间");
        app.motion.reduce_motion = false;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| app.finish_desktop_zoom(window, cx));
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(1280.0), px(800.0)),
        |_, _| gpui::AnyView::from(view.clone()).into_element(),
    );
    assert!(
        cx.debug_bounds("toolbar-project-01").is_some(),
        "当前空间 A 应有 01 入口"
    );
    let second = cx
        .debug_bounds("toolbar-project-02")
        .expect("当前空间 B 应有 02 入口");
    assert!(
        cx.debug_bounds("toolbar-project-03").is_none(),
        "其他空间 C 不能进入当前 Dock"
    );
    assert!(
        cx.debug_bounds("toolbar-project-overview").is_some(),
        "应保留项目总览入口"
    );
    cx.simulate_click(second.center(), Modifiers::none());
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(b.id.as_str()),
            "02 必须展开 B"
        );
        assert_ne!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(a.id.as_str()),
            "不能误开 A"
        );
        assert!(
            app.desktop_project_zoom_active(),
            "Dock 切换必须复用项目镜头"
        );
        app.finish_desktop_zoom(window, cx);
    });
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(1280.0), px(800.0)),
        |_, _| gpui::AnyView::from(view.clone()).into_element(),
    );
    let selected = cx
        .debug_bounds("toolbar-project-02")
        .expect("展开 B 后仍应显示编号");
    cx.simulate_click(selected.center(), Modifiers::none());
    host.update_in(cx, |app, _, _| {
        assert!(
            app.desktop_workspace.transition.is_none(),
            "重复点击当前项目不得重新启动缩放"
        );
    });
    let first = cx
        .debug_bounds("toolbar-project-01")
        .expect("B 画布仍可切换到 A");
    cx.simulate_click(first.center(), Modifiers::none());
    host.update_in(cx, |app, window, cx| {
        assert!(
            app.desktop_project_zoom_active(),
            "项目之间切换应沿已有镜头收回 B"
        );
        app.finish_desktop_zoom(window, cx);
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(a.id.as_str()),
            "收回 B 后必须展开用户点击的 A"
        );
        assert_ne!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(b.id.as_str()),
            "不能仍停留在 B"
        );
        app.finish_desktop_zoom(window, cx);
    });
    cx.draw(
        point(px(0.0), px(0.0)),
        size(px(1280.0), px(800.0)),
        |_, _| gpui::AnyView::from(view.clone()).into_element(),
    );
    let overview = cx
        .debug_bounds("toolbar-project-overview")
        .expect("展开项目后应有总览入口");
    cx.simulate_click(overview.center(), Modifiers::none());
    host.update_in(cx, |app, _, _| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id, None,
            "总览按钮应收回项目"
        );
        assert!(
            app.desktop_project_zoom_active(),
            "返回总览必须保留反向动画"
        );
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "项目总览不能跳到空间目录"
        );
        assert_eq!(
            app.desktop_workspace.active_id.as_deref(),
            Some(workspace.id.as_str()),
            "工作空间身份应保持"
        );
        assert!(
            !app.desktop_workspace
                .sessions
                .contains_key(&(other.id, c.id)),
            "不能误加载其他空间项目"
        );
    });
}
