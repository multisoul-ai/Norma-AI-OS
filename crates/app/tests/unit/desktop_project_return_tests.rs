use super::*;
use gpui::{div, prelude::*, px, size, Entity, Render, TestAppContext};

struct ReturnView(Entity<MultisoulApp>);

impl Render for ReturnView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.0.update(cx, |app, cx| {
            div()
                .relative()
                .size_full()
                .child(app.render_desktop_workspace(window, cx))
        })
    }
}

/// 同级项目切换中返回总览：真实 A 只呈现一次，连续缩回其原网格位置。
///
/// 数据构造：1280×800 窗口，A/B/C 三项目，C未加载；仅 A 有一张 640×400 的真实终端，
///   初始 PTY 为80列×24行。32ms=2×16ms，处于180ms淡入和350ms尺寸过渡内。
///   几何容差为1个设备像素（1/scale_factor），只容纳GPUI布局取整。
/// 执行过程：读取总览真实标题位置 → 展开A → 切B并推进32ms → 记录A标题 →
///   立即返回总览并绘制 → 推进100×16ms=1600ms直到落定 → 去掉动画状态再次绘制。
/// 预期结果：返回瞬间A的位置、尺寸、透明度与镜头速度不跳，不叠加总览中的第二份A；
///   动画末帧及落定后标题均回到原位置，A恢复可见、C保留加载占位；节点、viewport和PTY不被重建或resize。
#[test]
fn desktop_project_return_keeps_one_continuous_live_canvas() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化隔离存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Return").expect("应创建空间");
    let root_a = temp.path().join("a");
    let root_b = temp.path().join("b");
    std::fs::create_dir(&root_a).expect("应创建A目录");
    std::fs::create_dir(&root_b).expect("应创建B目录");
    let a = catalog
        .add_project(&workspace.id, &root_a)
        .expect("应添加A");
    let b = catalog
        .add_project(&workspace.id, &root_b)
        .expect("应添加B");
    let root_c = temp.path().join("c");
    std::fs::create_dir(&root_c).expect("应创建C目录");
    let c = catalog
        .add_project(&workspace.id, &root_c)
        .expect("应添加C");
    let mut test = TestAppContext::single();
    let (view, cx) = test.add_window_view(|_, cx| {
        ReturnView(cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store)))
    });
    let host = view.update(cx, |view, _| view.0.clone());
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.desktop_workspace.workspaces = catalog.list().expect("应读取空间");
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id.clone(), window, cx);
    });
    cx.run_until_parked();
    host.update(cx, |app, _| {
        app.desktop_workspace
            .sessions
            .remove(&(workspace.id.clone(), c.id.clone()));
    });
    let session = host.read_with(cx, |app, _| {
        app.desktop_workspace.sessions[&(workspace.id.clone(), a.id.clone())].clone()
    });
    let node = session.update(cx, |app, cx| {
        let node = app
            .runtime
            .spawn_terminal(CanvasRect::new(0.0, 0.0, 640.0, 400.0), "/bin/sh", 80, 24)
            .expect("应启动A真实终端");
        app.motion.reduce_motion = true;
        cx.notify();
        node
    });
    host.update(cx, |_, cx| cx.notify());
    cx.update(|window, cx| window.draw(cx).clear());
    let overview_title = cx
        .debug_bounds("card-titlebar")
        .expect("总览应呈现A真实标题");
    let viewport = session.read_with(cx, |app, _| app.runtime.document.viewport);
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(a.id.clone(), window, cx);
        app.motion.reduce_motion = false;
        app.expand_desktop_project_area(b.id.clone(), window, cx);
        app.desktop_workspace
            .transition
            .as_mut()
            .expect("应有同级镜头")
            .tick(Duration::from_millis(32));
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let before = cx
        .debug_bounds("card-titlebar")
        .expect("切换中A仍有真实标题");
    host.update_in(cx, |app, window, cx| {
        let before = app
            .desktop_workspace
            .transition
            .as_ref()
            .expect("应有同级镜头");
        let camera = before.camera.rect();
        let opacity = before.projects.as_ref().expect("应有项目层").layers[0].opacity;
        app.collapse_desktop_project_area(window, cx);
        let after = app
            .desktop_workspace
            .transition
            .as_ref()
            .expect("返回须继续动画");
        assert_eq!(after.camera.rect(), camera, "返回不能重置镜头位置或尺寸");
        assert_eq!(
            after.projects.as_ref().expect("应保留项目层").layers[0].opacity,
            opacity,
            "返回不能重置正在淡出的A"
        );
        assert!(
            app.desktop_workspace.expanded_area_id.is_none(),
            "目标应立即改为总览"
        );
        assert!(
            app.desktop_workspace.pending_navigation.is_none(),
            "不能排队二次返回"
        );
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let after = cx
        .debug_bounds("card-titlebar")
        .expect("返回中不能丢失A真实标题");
    assert!(
        cx.debug_bounds("desktop-project-live-0").is_none(),
        "同级切换层已经绘制真实A，总览不得再画第二份A"
    );
    assert!(
        cx.debug_bounds("desktop-project-live-1").is_none(),
        "同级切换层已经绘制真实B，总览不得再画第二份B"
    );
    assert!(
        cx.debug_bounds("desktop-project-live-2").is_some(),
        "C没有真实会话可供切换层绘制，必须保留总览的未加载占位"
    );
    let pixel = cx.update(|window, _| 1.0 / window.scale_factor());
    assert!(
        (f32::from(after.origin.x - before.origin.x)).abs() <= pixel
            && (f32::from(after.origin.y - before.origin.y)).abs() <= pixel
            && (f32::from(after.size.width - before.size.width)).abs() <= pixel,
        "返回瞬间A必须保持唯一的原呈现，不能画出另一份总览A：{before:?} -> {after:?}"
    );
    host.update(cx, |app, cx| {
        let transition = app
            .desktop_workspace
            .transition
            .as_mut()
            .expect("应有返回镜头");
        for _ in 0..100 {
            transition.tick(Duration::from_millis(16));
        }
        assert!(!transition.is_active(), "1600ms后应停止返回动画");
        let layers = &transition
            .projects
            .as_ref()
            .expect("应保留项目层直到末帧")
            .layers;
        assert_eq!(
            layers[0].opacity, 1.0,
            "总览中的A必须恢复可见，不能末帧突然出现"
        );
        assert_eq!(layers[1].opacity, 1.0, "总览中的B也应可见");
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    let last = cx.debug_bounds("card-titlebar").expect("返回末帧应呈现A");
    assert!(
        (f32::from(last.origin.x - overview_title.origin.x)).abs() <= pixel
            && (f32::from(last.origin.y - overview_title.origin.y)).abs() <= pixel
            && (f32::from(last.size.width - overview_title.size.width)).abs() <= pixel,
        "动画必须落到真实总览位置：{last:?} vs {overview_title:?}"
    );
    host.update_in(cx, |app, window, cx| {
        app.finish_desktop_zoom(window, cx);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert_eq!(
        cx.debug_bounds("card-titlebar"),
        Some(overview_title),
        "去掉动画后不能再换位置"
    );
    assert!(
        cx.debug_bounds("desktop-project-switch").is_none(),
        "落定后不应残留切换层"
    );
    session.read_with(cx, |app, _| {
        assert_eq!(
            app.runtime.document.viewport, viewport,
            "纯镜头不能改变项目viewport"
        );
        assert_eq!(
            app.runtime.running_terminal_count(),
            1,
            "A真实终端必须保留且不能复制"
        );
        let runtime::NodeView::Term { snapshot, .. } =
            app.runtime.node_view(node).expect("原节点应保留");
        assert_eq!(
            (snapshot.cols, snapshot.lines),
            (80, 24),
            "镜头不能触发PTY resize"
        );
    });
}
