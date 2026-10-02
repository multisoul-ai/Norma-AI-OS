use super::*;
use crate::app::AppScreen;
use gpui::{px, size, AppContext, TestAppContext};
use ui::component::Root;

fn assert_same_title(a: gpui::Bounds<gpui::Pixels>, b: gpui::Bounds<gpui::Pixels>, pixel: f32) {
    assert!(
        f32::from(a.origin.x - b.origin.x).abs() <= pixel
            && f32::from(a.origin.y - b.origin.y).abs() <= pixel
            && f32::from(a.size.width - b.size.width).abs() <= pixel
            && f32::from(a.size.height - b.size.height).abs() <= pixel,
        "同一真实标题不能跳位置或尺寸（容差一个设备像素）：{a:?} vs {b:?}"
    );
}

/// 项目画布直接返回目录：一次镜头到达嵌套缩略位置，中途反向保留真实卡片和速度。
///
/// 数据构造：1280×800、两个工作空间，第二空间的两个项目中仅A有一张640×400终端；
///   PTY为80列×24行，项目逻辑画布=(1280-28)×(800-127)=1252×673。
///   32ms=2×16ms处于弹簧运动起段；1600ms=100×16ms供弹簧收敛。
///   几何误差上限=1/scale_factor，只容纳GPUI设备像素取整。
/// 执行过程：记录真实目录与展开标题 → 从A打开Workspace → 绘制首帧 → 推进32ms →
///   再次切换反向 → 中途选B再选A → 绘制并继续一帧 → 落定A → 再次打开目录并一次落定
///   → 返回A → 退往目录途中选择同一工作空间，一次到达项目总览。
/// 预期结果：每次改目标真实标题均不跳；速度不重置、反向首帧保持惯性；目录末帧和
///   普通目录的位置一致，无第二份缩略或二次导航；换项目不丢失目录坐标，点击空间不隐式
///   展开A；同一节点、viewport、PTY尺寸始终不变。
#[test]
fn desktop_direct_directory_preserves_real_card_and_reverses_without_queue() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    catalog.create("Empty").expect("应创建目录的第一空间");
    let workspace = catalog.create("Working").expect("应创建第二空间");
    let root = temp.path().join("a");
    std::fs::create_dir(&root).expect("应创建A目录");
    let area = catalog.add_project(&workspace.id, &root).expect("应添加A");
    let other = temp.path().join("b");
    std::fs::create_dir(&other).expect("应创建B目录");
    let other_area = catalog.add_project(&workspace.id, &other).expect("应添加B");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let app = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(app.clone());
        Root::new(app, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.desktop_workspace.workspaces = catalog.list().expect("应读取空间");
        app.desktop_workspace.previews = catalog.list_previews().expect("应读取目录项目");
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    let session = host.read_with(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace.id.clone(), area.id.clone())].clone()
    });
    let node = session.update(cx, |app, cx| {
        app.motion.reduce_motion = true;
        let node = app
            .runtime
            .spawn_terminal(CanvasRect::new(0.0, 0.0, 640.0, 400.0), "/bin/sh", 80, 24)
            .expect("应创建真实终端");
        cx.notify();
        node
    });
    host.update_in(cx, |app, window, cx| {
        app.show_desktop_directory(false, window, cx)
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let directory_title = cx.debug_bounds("card-titlebar").expect("目录应有真实A标题");
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id.clone(), window, cx);
        app.motion.reduce_motion = false;
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let full_title = cx.debug_bounds("card-titlebar").expect("展开应有真实A标题");
    let viewport = session.read_with(cx, |app, _| app.runtime.document.viewport);
    let pixel = cx.update(|window, _| 1.0 / window.scale_factor());
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_switcher(window, cx);
        assert!(
            matches!(app.screen, AppScreen::Launchpad),
            "目录目标必须立即生效"
        );
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "不能排队工作空间动画"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_same_title(
        cx.debug_bounds("card-titlebar").expect("返回首帧应保留A"),
        full_title,
        pixel,
    );
    assert!(
        cx.debug_bounds("desktop-workspace-content-1").is_none(),
        "目录不能另外绘制第二份正在移动的真实A"
    );
    host.update(cx, |app, cx| {
        app.desktop_workspace
            .transition
            .as_mut()
            .expect("应有返回镜头")
            .tick(Duration::from_millis(32));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let midway = cx.debug_bounds("card-titlebar").expect("中途应保留A");
    assert!(
        midway.size.width < full_title.size.width,
        "镜头应在向目录缩小"
    );
    host.update_in(cx, |app, window, cx| {
        let transition = app
            .desktop_workspace
            .transition
            .as_ref()
            .expect("应保留镜头");
        let frame = transition.camera.rect();
        let velocity = transition.camera.velocity;
        assert!(velocity[2] < 0.0, "反向前应有缩小速度");
        app.open_desktop_switcher(window, cx);
        let transition = app
            .desktop_workspace
            .transition
            .as_ref()
            .expect("反向应继续镜头");
        assert_eq!(transition.camera.rect(), frame, "反向不能重设镜头几何");
        assert_eq!(transition.camera.velocity, velocity, "反向不能丢失当前速度");
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "反向不能等待目录先落定"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_same_title(
        cx.debug_bounds("card-titlebar").expect("反向首帧应有A"),
        midway,
        pixel,
    );
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(other_area.id.clone(), window, cx);
        assert!(
            app.desktop_directory_zoom_active(),
            "从目录反向途中切B必须保留同一个目录坐标系，不能突然丢弃外围场景"
        );
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "改选B不能排队"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_same_title(
        cx.debug_bounds("card-titlebar")
            .expect("改选B首帧应保留正在退场的A"),
        midway,
        pixel,
    );
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id.clone(), window, cx)
    });
    host.update(cx, |app, cx| {
        let transition = app
            .desktop_workspace
            .transition
            .as_mut()
            .expect("应有反向镜头");
        let width = transition.camera.rect().size.x;
        transition.tick(Duration::from_millis(16));
        assert!(
            transition.camera.rect().size.x < width,
            "反向第一帧应保留缩小惯性，不能瞬间折返"
        );
        for _ in 0..100 {
            transition.tick(Duration::from_millis(16));
        }
        assert!(!transition.is_active(), "1600ms后反向应收敛");
        cx.notify();
    });
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        assert!(
            app.desktop_workspace.transition.is_none(),
            "反向落定不应启动第二次动画"
        );
        app.open_desktop_switcher(window, cx);
        for _ in 0..100 {
            app.desktop_workspace
                .transition
                .as_mut()
                .expect("应有目录镜头")
                .tick(Duration::from_millis(16));
        }
        assert!(
            !app.desktop_workspace
                .transition
                .as_ref()
                .unwrap()
                .is_active(),
            "目录镜头应一次收敛"
        );
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let last = cx.debug_bounds("card-titlebar").expect("目录末帧应有A");
    assert_same_title(last, directory_title, pixel);
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        assert!(
            app.desktop_workspace.transition.is_none(),
            "到目录后不能再排第二段工作空间动画"
        );
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_same_title(
        cx.debug_bounds("card-titlebar").expect("目录落定后应有A"),
        last,
        pixel,
    );
    host.update_in(cx, |app, window, cx| app.open_desktop_switcher(window, cx));
    cx.update(|window, cx| window.draw(cx).clear());
    assert_same_title(
        cx.debug_bounds("card-titlebar")
            .expect("从目录返回A首帧应连续"),
        last,
        pixel,
    );
    host.update_in(cx, |app, window, cx| {
        for _ in 0..100 {
            app.desktop_workspace
                .transition
                .as_mut()
                .expect("应有直接展开镜头")
                .tick(Duration::from_millis(16));
        }
        app.finish_desktop_zoom(window, cx);
        assert!(
            app.desktop_workspace.transition.is_none(),
            "一次落定必须到A，不先展开工作空间"
        );
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_same_title(
        cx.debug_bounds("card-titlebar").expect("A落定应有真实标题"),
        full_title,
        pixel,
    );
    host.update_in(cx, |app, window, cx| {
        app.open_desktop_switcher(window, cx);
        app.desktop_workspace
            .transition
            .as_mut()
            .expect("应有目录返回镜头")
            .tick(Duration::from_millis(32));
        let camera = app
            .desktop_workspace
            .transition
            .as_ref()
            .unwrap()
            .camera
            .clone();
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "返回途中选择同一空间必须立即改为总览目标"
        );
        assert!(
            matches!(app.screen, AppScreen::DesktopWorkspace),
            "不能先等目录落定再进入空间"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "点击空间不能暗中恢复A"
        );
        assert_eq!(
            app.desktop_workspace
                .transition
                .as_ref()
                .unwrap()
                .camera
                .rect(),
            camera.rect(),
            "中途选择空间不能重置几何"
        );
        assert_eq!(
            app.desktop_workspace
                .transition
                .as_ref()
                .unwrap()
                .camera
                .velocity,
            camera.velocity,
            "中途选择空间不能清除速度"
        );
        for _ in 0..100 {
            app.desktop_workspace
                .transition
                .as_mut()
                .unwrap()
                .tick(Duration::from_millis(16));
        }
        app.finish_desktop_zoom(window, cx);
        assert!(
            app.desktop_workspace.transition.is_none(),
            "一次落定必须到达空间总览"
        );
        cx.notify();
    });
    session.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.document.viewport, viewport,
            "镜头不能改变内容viewport"
        );
        assert_eq!(
            app.runtime.running_terminal_count(),
            1,
            "整个过程不能复制或重建终端"
        );
        let runtime::NodeView::Term { snapshot, .. } =
            app.runtime.node_view(node).expect("原节点必须仍存在");
        assert_eq!(
            (snapshot.cols, snapshot.lines),
            (80, 24),
            "纯镜头不能resize PTY"
        );
    });
}
