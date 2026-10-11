use super::*;
use crate::app::AppScreen;
use gpui::{prelude::*, px, size, Entity, Render, TestAppContext};

struct OriginHost(Entity<MultisoulApp>);

impl Render for OriginHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.clone()
    }
}

fn assert_drawn_origin(actual: CanvasRect, drawn: CanvasRect, window: &Window) {
    // GPUI rounds both edges to backing pixels; allow one physical pixel of logical rounding.
    let tolerance = 1.0 / window.scale_factor();
    assert!(
        (actual.origin.x - drawn.origin.x).abs() <= tolerance
            && (actual.origin.y - drawn.origin.y).abs() <= tolerance
            && (actual.size.x - drawn.size.x).abs() <= tolerance
            && (actual.size.y - drawn.size.y).abs() <= tolerance,
        "镜头起点必须与实际绘制矩形一致，最多允许一个设备像素取整：{actual:?} vs {drawn:?}"
    );
}

/// 镜头来源尚未缓存或展开后窗口变窄，仍必须与实际总览布局一致。
///
/// 数据构造：1280×800 窗口，3 个工作空间，Beta 含 2 个空项目；
///   workspace 卡片由真实 Window::draw 取得，项目内容从标题下方等比适配。
///   比较容差 = 1 / backing scale，仅容纳 GPUI 设备像素取整；错误居中框偏移数十像素。
///   窄窗600×800时两项目仍在上排，第二个区域为(308,223.75,268,150.75)，宽=(600-2×24-16)/2，高=宽×9/16。
///   顶部避让=52px工具栏+12px间距=64px；首行y=64+(645-2×150.75-24)/2=223.75。
///   200×16ms=3.2 秒模拟推进大于弹簧收敛时间，不等待墙钟。
/// 执行过程：绘制目录 → 清空来源缓存 → 打开 Beta → 绘制项目总览 → 清空来源缓存
///   → 展开第二项目 → 改为窄窗 → 返回并推进镜头。
/// 预期结果：两个展开起点与真正绘制位置相同；返回落在新窗右上角，不用居中猜测框或旧列位置。
#[test]
fn desktop_camera_origins_use_real_layout_without_prior_bounds() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    catalog.create("Alpha").expect("应创建 Alpha");
    let workspace = catalog.create("Beta").expect("应创建 Beta");
    catalog.create("Gamma").expect("应创建 Gamma");
    let root_a = temp.path().join("a");
    let root_b = temp.path().join("b");
    std::fs::create_dir(&root_a).expect("应创建项目 A");
    std::fs::create_dir(&root_b).expect("应创建项目 B");
    catalog
        .add_project(&workspace.id, &root_a)
        .expect("应添加 A");
    let area = catalog
        .add_project(&workspace.id, &root_b)
        .expect("应添加 B");
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        OriginHost(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.desktop_workspace.workspaces = catalog.list().expect("应读取目录");
        app.screen = AppScreen::Launchpad;
        app.desktop_workspace.previews = catalog.list_previews().expect("应加载真实预览");
        app.motion.reduce_motion = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let workspace_bounds = cx
        .debug_bounds("desktop-workspace-content-1")
        .expect("应绘制 Beta");
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace
            .workspace_rects
            .lock()
            .unwrap()
            .clear();
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
        let source = app
            .desktop_workspace
            .transition
            .as_ref()
            .expect("应有空间展开动画")
            .rect();
        assert_drawn_origin(source, canvas_rect_from_bounds(workspace_bounds), window);
        assert_ne!(source.origin.x, 0.0, "展开起点不能直接成为全屏");
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let project_bounds = cx
        .debug_bounds("desktop-project-area-1")
        .expect("应绘制项目 B");
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.area_rects.lock().unwrap().clear();
        app.expand_desktop_project_area(area.id.clone(), window, cx);
        let source = app
            .desktop_workspace
            .transition
            .as_ref()
            .expect("应有项目展开动画")
            .rect();
        let expected = super::super::preview_canvas::project_canvas_rect(
            canvas_rect_from_bounds(project_bounds),
            desktop_scene_rect(1280.0, 800.0).size,
            1.0,
        );
        assert_drawn_origin(source, expected, window);
        app.finish_desktop_zoom(window, cx);
        cx.notify();
    });
    cx.simulate_resize(size(px(600.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.collapse_desktop_project_area(window, cx);
        let transition = app
            .desktop_workspace
            .transition
            .as_mut()
            .expect("应有返回动画");
        for _ in 0..200 {
            transition.tick(Duration::from_millis(16));
        }
        let expected = super::super::preview_canvas::project_canvas_rect(
            CanvasRect::new(308.0, 223.75, 268.0, 150.75),
            desktop_scene_rect(600.0, 800.0).size,
            1.0,
        );
        assert_eq!(
            transition.rect(),
            expected,
            "窄窗返回必须落在新网格右上角，不能沿用宽窗缓存"
        );
        assert!(
            !transition.is_active(),
            "3.2秒模拟推进后应收敛，不能依赖多次绘制才修正终点"
        );
    });
}

/// 快捷导航选择被搜索隐藏的工作空间，先恢复包含目标的目录布局再开始动画。
///
/// 数据构造：1280×800，Alpha/Beta 两空间；搜索 Alpha 后仅剩一张主预览卡片。
///   几何容差 = 1 / backing scale，覆盖 GPUI 的设备像素取整。
/// 执行过程：记录未过滤 Beta 的真实位置 → 搜索 Alpha → 通过公共入口打开 Beta。
/// 预期结果：目标恢复到未过滤布局的位置；清空过滤而非借用 Alpha 的来源或虚构中心框；不隐式展开项目。
#[test]
fn desktop_camera_origins_reveal_filtered_keyboard_target() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    catalog.create("Alpha").expect("应创建 Alpha");
    let workspace = catalog.create("Beta").expect("应创建 Beta");
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        OriginHost(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.desktop_workspace.workspaces = catalog.list().expect("应读取目录");
        app.screen = AppScreen::Launchpad;
        app.desktop_workspace.previews = catalog.list_previews().expect("应加载真实预览");
        app.motion.reduce_motion = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let beta = cx
        .debug_bounds("desktop-workspace-content-1")
        .expect("应绘制 Beta");
    host.update(cx, |app, cx| {
        app.desktop_workspace.search_query = "Alpha".into();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-workspace-content-1").is_none(),
        "过滤后 Beta 必须确实不可见"
    );
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
        assert!(
            app.desktop_workspace.search_query.is_empty(),
            "选择隐藏目标时必须恢复包含目标的真实目录布局"
        );
        assert_drawn_origin(
            app.desktop_workspace
                .transition
                .as_ref()
                .expect("应有目标动画")
                .rect(),
            canvas_rect_from_bounds(beta),
            window,
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "选择空间不能越级展开项目"
        );
    });
}

/// 目录滚动后展开和返回，镜头与实际可见卡片必须保留同一滚动偏移。
///
/// 数据构造：1280×800、12 个空空间，首页主预览和侧卡之后按2列排列；紧凑卡片高132，行距16。
///   滚动 -500px 后第5张的缩略图位于约184px，仍在视口中。
///   几何容差为 1 / backing scale；200×16ms=3.2秒模拟推进用于弹簧收敛。
/// 执行过程：真实绘制长目录 → 滚动500px → 打开第5个空间 → 返回目录并推进镜头 → 再绘制。
/// 预期结果：起点和终点均等于滚动后的卡片位置；返回不重置到第一页，不虚构中心起点。
#[test]
fn desktop_camera_origins_preserve_directory_scroll() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    for index in 0..12 {
        catalog
            .create(&format!("Space {index}"))
            .expect("应创建空间");
    }
    let workspaces = catalog.list().expect("应读取固定顺序目录");
    let target = workspaces[4].id.clone();
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        OriginHost(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update(cx, |app, cx| {
        app.desktop_workspace.workspaces = workspaces;
        app.screen = AppScreen::Launchpad;
        app.desktop_workspace.previews = catalog.list_previews().expect("应加载真实预览");
        app.motion.reduce_motion = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    host.update(cx, |app, cx| {
        app.desktop_directory_scroll
            .set_offset(gpui::point(px(0.0), px(-500.0)));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let before = cx
        .debug_bounds("desktop-workspace-content-4")
        .expect("应看到第二行目标");
    assert!(
        f32::from(before.origin.y) < 200.0,
        "长目录必须确实滚动，不能用未滚动快照代替"
    );
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_workspace(target, window, cx);
        assert_drawn_origin(
            app.desktop_workspace
                .transition
                .as_ref()
                .expect("应有展开动画")
                .rect(),
            canvas_rect_from_bounds(before),
            window,
        );
        app.finish_desktop_zoom(window, cx);
        app.show_desktop_launchpad(window, cx);
        let transition = app
            .desktop_workspace
            .transition
            .as_mut()
            .expect("应有返回动画");
        for _ in 0..200 {
            transition.tick(Duration::from_millis(16));
        }
        assert_drawn_origin(transition.rect(), canvas_rect_from_bounds(before), window);
        assert!(!transition.is_active(), "返回必须收敛至原滚动位置");
        app.finish_desktop_zoom(window, cx);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        cx.debug_bounds("desktop-workspace-content-4"),
        Some(before),
        "返回目录不能重置滚动位置造成末帧跳动"
    );
}
