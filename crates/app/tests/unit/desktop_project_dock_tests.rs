use super::*;
use crate::app::AppScreen;
use gpui::{div, prelude::*, px, size, Entity, Modifiers, Render, TestAppContext};

struct DockView(Entity<MultisoulApp>);

impl Render for DockView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |app, cx| {
            div()
                .relative()
                .size_full()
                .child(app.render_desktop_workspace(window, cx))
                .child(app.render_desktop_dock(window, cx))
        })
    }
}

/// 项目 Dock：编号只代表当前空间项目，点击切换真实项目，网格返回项目总览。
///
/// 数据构造：当前空间含项目 A/B，另一空间含 C；A/C 复用目录，B 使用另一目录，
///   用不同区域 ID 验证归属；窗口 1280×800，两个编号应为 01/02，不得出现 03。
/// 执行过程：加载当前空间 → 绘制真实 Dock → 点击 02 → 完成镜头 → 再点 02 → 直接切到 01 → 点击总览。
/// 预期结果：只渲染 A/B；总览进入 B 保留镜头，重复点 B 不重新缩放；B→A 直接平移到 A，
///   不经过项目总览、不排队展开；旧 B 解除窗口绑定但保留会话，总览按钮仍收回 A，
///   快捷键、返回中途切换和减少动态效果下同样直达；B 未加载时先保留 A，加载后直达 B；
///   当前空间不变、C 不被展开。
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
    cx.update(|window, cx| window.draw(cx).clear());
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
    cx.update(|window, cx| window.draw(cx).clear());
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
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-project-switch").is_some(),
        "同级切换必须绘制真实项目过渡层"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-overview").is_none(),
        "同级切换中不能闪出项目总览"
    );
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(a.id.as_str()),
            "点击 01 必须立即显示 A 的 Agent 画布，不能先收回 B"
        );
        assert!(
            app.desktop_workspace.transition.is_some(),
            "同级项目切换必须保留直接平移镜头"
        );
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "不能排队等待第二段项目展开"
        );
        let a_session = app
            .desktop_workspace
            .sessions
            .get(&(workspace.id.clone(), a.id.clone()))
            .expect("A 会话应保留");
        let b_session = app
            .desktop_workspace
            .sessions
            .get(&(workspace.id.clone(), b.id.clone()))
            .expect("B 会话应继续后台运行");
        assert!(
            a_session.read(cx).project_window.is_some(),
            "目标 A 应接管当前窗口"
        );
        assert!(
            b_session.read(cx).project_window.is_none(),
            "旧 B 必须解除窗口绑定，不能遗留浏览器等原生视图"
        );
        app.finish_desktop_zoom(window, cx);
        assert_ne!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(b.id.as_str()),
            "不能仍停留在 B"
        );
        app.finish_desktop_zoom(window, cx);
        app.navigate_desktop_shortcut(crate::shortcuts::ShortcutCommandId::NextProject, window, cx);
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(b.id.as_str()),
            "项目快捷键应直接到 B"
        );
        assert!(
            app.desktop_workspace.transition.is_some(),
            "快捷键应直接平移，不能取消所有动画"
        );
        app.collapse_desktop_project_area(window, cx);
        assert!(app.desktop_project_zoom_active(), "显式返回总览仍须动画");
        app.expand_desktop_project_area(a.id.clone(), window, cx);
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(a.id.as_str()),
            "返回动画中选择 A 也应直达"
        );
        assert!(
            app.desktop_workspace.transition.is_some(),
            "切换应从尚未结束的返回动画转向目标项目"
        );
        app.motion.reduce_motion = true;
        app.expand_desktop_project_area(b.id.clone(), window, cx);
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(b.id.as_str()),
            "减少动态效果下同样直达 B"
        );
        assert!(
            app.desktop_workspace.sessions[&(workspace.id.clone(), a.id.clone())]
                .read(cx)
                .project_window
                .is_none(),
            "减少动态效果不能跳过旧项目窗口解绑"
        );
        app.motion.reduce_motion = false;
        app.expand_desktop_project_area(a.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.sessions.remove(&(workspace.id.clone(), b.id.clone()));
        app.expand_desktop_project_area(b.id.clone(), window, cx);
        assert_eq!(app.desktop_workspace.expanded_area_id.as_deref(), Some(a.id.as_str()), "B 加载期间应保留 A 画布，不能闪回总览");
        assert!(matches!(&app.desktop_workspace.pending_navigation, Some(super::super::scene::DesktopNavigation::Project(id)) if id == &b.id), "未就绪的 B 应只等待加载");
        app.expand_desktop_project_area(a.id.clone(), window, cx);
        assert!(app.desktop_workspace.pending_navigation.is_none(), "重新选择 A 必须取消等待 B 的导航");
        assert!(app.desktop_workspace.transition.is_none(), "重新选择当前 A 不得重播项目展开");
        app.expand_desktop_project_area(b.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        assert_eq!(
            app.desktop_workspace.expanded_area_id.as_deref(),
            Some(b.id.as_str()),
            "B 加载完成应直达 Agent 画布"
        );
        assert!(
            app.desktop_workspace.transition.is_some(),
            "加载完成后应启动直接平移，不能瞬间替换画布"
        );
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "加载请求落定后必须清空"
        );
        app.expand_desktop_project_area(a.id.clone(), window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.update(|window, cx| window.draw(cx).clear());
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
