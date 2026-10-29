use crate::app::MultisoulApp;
use canvas::{CanvasNodeKind, CanvasRect, Document};
use gpui::{px, size, AppContext, TestAppContext};

/// 首页把真实节点几何绘成示意窗口，不要求打开项目或读取终端输出。
/// 数据：1个项目，左侧400×400、右上200×180、右下200×180，横距420、纵距220；
///   三个窗口的比例2:1，左右间距相对宽度为420/400=1.05。
/// 执行：保存快照→绘制深浅首页→注入只有一个节点的实时会话→清空实时会话。
/// 预期：快照恰好3个且比例、相对位置一致；实时会话恰好1个；清空后0个，
///   不回退旧快照，不渲染真实终端控件，也不启动项目会话。
#[test]
fn desktop_home_preview_tracks_saved_and_live_geometry_without_terminal_content() {
    let temp = tempfile::tempdir().expect("创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Preview geometry").expect("创建空间");
    let area = catalog.add_project(&workspace.id, &root).expect("添加项目");
    let project =
        storage::ProjectStore::initialize_for_desktop_area(&paths, &root, &workspace.id, &area.id)
            .expect("初始化项目存储");
    let mut document = Document::new();
    for bounds in [
        CanvasRect::new(-100.0, -60.0, 400.0, 400.0),
        CanvasRect::new(320.0, -60.0, 200.0, 180.0),
        CanvasRect::new(320.0, 160.0, 200.0, 180.0),
    ] {
        document.add_node(CanvasNodeKind::Term(Default::default()), bounds);
    }
    project
        .canvas_store()
        .save_snapshot(&document.to_snapshot())
        .expect("保存三窗口快照");
    let mut test = TestAppContext::single();
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity =
            cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths.clone(), store.clone()));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    cx.run_until_parked();
    for theme in [ui::ThemeId::SapphireNight, ui::ThemeId::PaperBlue] {
        host.update(cx, |app, cx| {
            app.theme_id = theme;
            app.theme = ui::Theme::for_id(theme);
            assert!(
                app.desktop_workspace.sessions.is_empty(),
                "静止首页不能为预览启动项目会话"
            );
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear());
        let left = cx
            .debug_bounds("desktop-home-node-0-0-1")
            .expect("保存的左窗口应显示为示意卡");
        let upper = cx
            .debug_bounds("desktop-home-node-0-0-2")
            .expect("右上窗口");
        let lower = cx
            .debug_bounds("desktop-home-node-0-0-3")
            .expect("右下窗口");
        let project = cx
            .debug_bounds("desktop-workspace-preview-0-0")
            .expect("保留相机项目锚点");
        assert!(
            project.contains(&left.origin) && project.contains(&lower.bottom_right()),
            "节点应位于自身项目区域内"
        );
        assert!(
            (left.size.width / upper.size.width - 2.0).abs() < 0.02,
            "真实宽度比例400:200须保留"
        );
        assert!(
            (left.size.width / left.size.height - 1.0).abs() < 0.02,
            "统一缩放不能拉伸真实方形"
        );
        assert!(
            ((upper.left() - left.left()) / left.size.width - 1.05).abs() < 0.02,
            "保留真实横向间距"
        );
        assert!(
            (upper.left() - lower.left()).abs() < px(1.0),
            "右侧两窗口同列"
        );
        assert!(lower.top() > upper.bottom(), "保留右侧上下布局及间隔");
        assert!(
            cx.debug_bounds("desktop-home-node-0-0-4").is_none(),
            "不能虚构第四个终端"
        );
        assert!(
            cx.debug_bounds("card-titlebar").is_none(),
            "首页不嵌入真实终端控件"
        );
    }
    let session = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
    session.update(cx, |app, _| {
        app.runtime.document.add_node(
            CanvasNodeKind::Term(Default::default()),
            CanvasRect::new(0.0, 0.0, 500.0, 200.0),
        );
    });
    host.update(cx, |app, cx| {
        app.desktop_workspace
            .sessions
            .insert((workspace.id, area.id), session.clone());
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-home-node-0-0-1").is_some(),
        "实时会话节点应显示"
    );
    assert!(
        cx.debug_bounds("desktop-home-node-0-0-2").is_none(),
        "实时会话优先于旧快照"
    );
    session.update(cx, |app, _| app.runtime.document = Document::new());
    host.update(cx, |_, cx| cx.notify());
    cx.update(|window, cx| window.draw(cx).clear());
    assert!(
        cx.debug_bounds("desktop-home-node-0-0-1").is_none(),
        "清空实时画布后不能复活保存节点"
    );
    assert!(
        cx.debug_bounds("desktop-workspace-preview-0-0").is_some(),
        "空项目仍保留转场锚点"
    );
}
