use crate::app::MultisoulApp;
use canvas::{CanvasNodeKind, NodeId, TermSpec, Vec2};
use gpui::{px, size, AppContext, Entity, TestAppContext, VisualTestContext, Window};

pub(super) fn open_wide_embedded_project<'a>(
    test: &'a mut TestAppContext,
    root: &std::path::Path,
    name: &str,
) -> (Entity<MultisoulApp>, &'a mut VisualTestContext) {
    let project_root = root.join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&root.join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create(name).expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
    let mut host = None;
    let (_, cx) = test.add_window_view(|window, cx| {
        let entity = cx.new(|cx| MultisoulApp::new_with_database_context(cx, paths, store));
        host = Some(entity.clone());
        ui::component::Root::new(entity, window, cx)
    });
    let host = host.expect("应创建宿主");
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    host.update_in(cx, |app, window, cx| {
        app.motion.reduce_motion = true;
        app.open_desktop_workspace(workspace.id, window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    cx.run_until_parked();
    host.update_in(cx, |app, window, cx| {
        app.expand_desktop_project_area(area.id, window, cx);
        app.finish_desktop_zoom(window, cx);
    });
    let project = host.read_with(cx, |app, _| {
        app.expanded_desktop_project().expect("项目应已展开")
    });
    (project, cx)
}

pub(super) fn place_tiled_terminals(
    app: &mut MultisoulApp,
    window: &mut Window,
    count: usize,
) -> Vec<NodeId> {
    app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
    let viewport = app.visible_canvas_rect(window);
    let zoom = app.runtime.document.viewport.zoom;
    let mut ids = Vec::with_capacity(count);
    for _ in 0..count {
        let bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
        let id = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), bounds);
        app.apply_layout_for_new_node_for_area(id, viewport, zoom);
        ids.push(id);
    }
    ids
}
