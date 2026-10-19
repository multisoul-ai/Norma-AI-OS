use super::*;
use canvas::{CanvasNodeKind, CanvasRect, TermSpec, Vec2};
use gpui::{px, size, TestAppContext};

/// 场景：宽画布已排好三张卡片后，用户打开抽屉把中央画布收窄到 472px。
///
/// 数据构造：945px 画布上三张终端并排；随后 sync 到 472px（≤650 窄画布阈值）。
/// 执行过程：只改变画布宽度并刷新展示，不新增卡片。
/// 预期结果：展示变为单列垂直堆叠；保存坐标仍保持多列。
#[test]
fn drawer_narrow_adapts_presentation_without_new_cards() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Drawer Adapt").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
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
    project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let viewport = app.visible_canvas_rect(window);
        let zoom = app.runtime.document.viewport.zoom;
        let mut ids = Vec::new();
        for _ in 0..3 {
            let bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
            let id = app
                .runtime
                .document
                .add_node(CanvasNodeKind::Term(TermSpec), bounds);
            app.apply_layout_for_new_node_for_area(id, viewport, zoom);
            ids.push(id);
        }
        let wide_first = app.presented_node_bounds(ids[0]).expect("首张应展示");
        let wide_second = app.presented_node_bounds(ids[1]).expect("第二张应展示");
        assert!(
            wide_second.origin.x >= wide_first.origin.x + wide_first.size.x,
            "夹具必须先得到并排布局"
        );
        let saved_before: Vec<_> = ids
            .iter()
            .map(|id| app.runtime.document.node(*id).expect("卡片应保存").bounds)
            .collect();

        app.sync_desktop_embedded_canvas_size(Vec2::new(472.0, 710.0), window);
        let shown_first = app.presented_node_bounds(ids[0]).expect("收窄后首张应展示");
        let shown_second = app
            .presented_node_bounds(ids[1])
            .expect("收窄后第二张应展示");
        assert!(
            shown_second.origin.y >= shown_first.origin.y + shown_first.size.y,
            "打开抽屉收窄画布后，卡片必须自动改为单列堆叠"
        );
        for (index, id) in ids.iter().enumerate() {
            assert_eq!(
                app.runtime.document.node(*id).expect("卡片应保存").bounds,
                saved_before[index],
                "抽屉收窄不得改写第 {index} 张卡片的保存坐标（须逐字段相等，不能只保持多列）"
            );
        }
    });
}

/// 场景：窗口分辨率变大后，嵌入项目的保存网格应跟着 FitViewport 放大。
///
/// 数据构造：先在 1280×800（场景约 1252×645）排好两张卡片；再把窗口改为 1600×900，
/// 并 sync 对应更大的画布宽度。
/// 执行过程：更新 reflow 尺寸与画布尺寸（模拟宿主在 render 时 sync）。
/// 预期结果：保存卡片宽度显著变大；不得继续沿用旧窗口下的窄格子。
#[test]
fn window_resize_reflows_saved_embedded_cards() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Window Reflow").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
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
    let mut before_width = 0.0;
    project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let viewport = app.visible_canvas_rect(window);
        let zoom = app.runtime.document.viewport.zoom;
        for _ in 0..2 {
            let bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
            let id = app
                .runtime
                .document
                .add_node(CanvasNodeKind::Term(TermSpec), bounds);
            app.apply_layout_for_new_node_for_area(id, viewport, zoom);
        }
        before_width = app
            .runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
            .expect("应有终端卡片")
            .bounds
            .size
            .x;
    });
    cx.simulate_resize(size(px(1600.0), px(900.0)));
    let mut wide_width = 0.0;
    project.update_in(cx, |app, window, _| {
        // 1600 窗口场景宽 = 1600 - 28 = 1572；画布随抽屉布局变宽。
        app.sync_desktop_embedded_canvas_size(Vec2::new(1200.0, 745.0), window);
        wide_width = app
            .runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
            .expect("应有终端卡片")
            .bounds
            .size
            .x;
        assert!(
            wide_width > before_width + 40.0,
            "窗口变大后保存网格应重新 FitViewport：before={before_width}, after={wide_width}"
        );
    });
    // 再缩回小窗口：保存格子应重新变窄，证明缩小路径同样重排。
    cx.simulate_resize(size(px(1280.0), px(800.0)));
    project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let shrunk_width = app
            .runtime
            .document
            .nodes()
            .iter()
            .find(|node| matches!(node.kind, CanvasNodeKind::Term(_)))
            .expect("应有终端卡片")
            .bounds
            .size
            .x;
        assert!(
            shrunk_width + 40.0 < wide_width,
            "窗口变小后保存网格应重新收窄：wide={wide_width}, shrunk={shrunk_width}"
        );
    });
}

/// 场景：首次 sync 只记录窗口基线，不得把已恢复的自定义坐标 FitViewport 掉。
///
/// 数据构造：清空 reflow/canvas 尺寸缓存后写入非网格坐标，再第一次 sync。
/// 执行过程：`sync_desktop_embedded_canvas_size`（reflow_size 从 None → Some）。
/// 预期结果：保存 bounds 逐字段不变；仅建立基线供后续窗变触发 reflow。
#[test]
fn first_desktop_sync_does_not_reflow_restored_bounds() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("First Sync").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
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
    project.update_in(cx, |app, window, _| {
        app.desktop_embedded_reflow_size = None;
        app.desktop_embedded_canvas_size = None;
        let custom = CanvasRect::new(40.0, 50.0, 220.0, 180.0);
        let id = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), custom);
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        assert_eq!(
            app.runtime.document.node(id).expect("卡片应存在").bounds,
            custom,
            "首次 sync 只能记窗口基线，不得重写恢复后的保存坐标"
        );
        assert!(
            app.desktop_embedded_reflow_size.is_some(),
            "首次 sync 后应记下 reflow 基线，供后续窗变比较"
        );
    });
}

/// 场景：桌面项目处于主次布局且抽屉收窄时，窗变/交换 reflow 不得把窄画布写入保存坐标。
///
/// 数据构造：两张终端进入 master；再 sync 到 472px 窄画布。
/// 执行过程：调用 `apply_layout_for_current_mode`（与窗变 reflow / finish_card_swap 同路径）。
/// 预期结果：master 保存宽度仍接近稳定区；presentation 刷新为窄画布可用布局。
#[test]
fn desktop_master_reflow_uses_stable_area_then_presentation() {
    use crate::canvas::master_layout::enter_master_layout_for_area;

    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Master Stable").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
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
    project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let viewport = app.visible_canvas_rect(window);
        let zoom = app.runtime.document.viewport.zoom;
        let mut ids = Vec::new();
        for _ in 0..2 {
            let bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
            let id = app
                .runtime
                .document
                .add_node(CanvasNodeKind::Term(TermSpec), bounds);
            app.apply_layout_for_new_node_for_area(id, viewport, zoom);
            ids.push(id);
        }
        let layout_area = app.visible_canvas_rect(window);
        assert!(
            enter_master_layout_for_area(
                &mut app.master_layout,
                &mut app.runtime,
                &mut app.motion,
                ids[0],
                layout_area,
                zoom,
            ),
            "应进入主次布局"
        );
        app.sync_desktop_embedded_canvas_size(Vec2::new(472.0, 710.0), window);
        let before_master = app
            .runtime
            .document
            .node(ids[0])
            .expect("master 应存在")
            .bounds;
        app.apply_layout_for_current_mode(window);
        let after_master = app
            .runtime
            .document
            .node(ids[0])
            .expect("master 应存在")
            .bounds;
        assert!(
            after_master.size.x * zoom > 600.0,
            "桌面 master reflow 必须写稳定区宽度，不能把 472px 抽屉画布写入保存坐标：got {}",
            after_master.size.x * zoom
        );
        assert!(
            (after_master.size.x - before_master.size.x).abs() < 1.0
                || after_master.size.x * zoom > 600.0,
            "窄画布下 master 保存尺寸不得塌成抽屉宽度"
        );
        let shown = app
            .presented_node_bounds(ids[0])
            .expect("master 应有展示边界");
        assert!(
            shown.size.x * zoom <= 480.0,
            "presentation 应按窄画布刷新：shown_w={}",
            shown.size.x * zoom
        );
    });
}

/// 场景：桌面项目中把一张卡片拖到另一张上交换位置。
///
/// 数据构造：两张并排终端；记录交换前保存边界，再 swap_node_order 并走 mouse_up 后的 reflow 路径。
/// 执行过程：交换文档顺序后调用与桌面 mouse_up 相同的布局收口（`finish_card_swap`）。
/// 预期结果：两张卡片的保存格子对调；展示位置也随之对调。
#[test]
fn desktop_card_swap_exchanges_grid_slots() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Card Swap").expect("应创建工作空间");
    let area = catalog
        .add_project(&workspace.id, &project_root)
        .expect("应添加项目");
    let mut test = TestAppContext::single();
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
    project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let viewport = app.visible_canvas_rect(window);
        let zoom = app.runtime.document.viewport.zoom;
        let mut ids = Vec::new();
        for _ in 0..2 {
            let bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
            let id = app
                .runtime
                .document
                .add_node(CanvasNodeKind::Term(TermSpec), bounds);
            app.apply_layout_for_new_node_for_area(id, viewport, zoom);
            ids.push(id);
        }
        let before_a = app.runtime.document.node(ids[0]).expect("A 应存在").bounds;
        let before_b = app.runtime.document.node(ids[1]).expect("B 应存在").bounds;
        assert!(
            before_b.origin.x > before_a.origin.x,
            "夹具必须是 A 在左、B 在右"
        );

        assert!(
            app.runtime.document.swap_node_order(ids[0], ids[1]),
            "交换顺序应成功"
        );
        // 走与 canvas mouse_up 相同的桌面收口：交换后必须重排格子，不能只改 Vec 顺序。
        app.finish_card_swap(window);

        let after_a = app.runtime.document.node(ids[0]).expect("A 应存在").bounds;
        let after_b = app.runtime.document.node(ids[1]).expect("B 应存在").bounds;
        assert_eq!(after_a, before_b, "交换后 A 应占据原先 B 的保存格子");
        assert_eq!(after_b, before_a, "交换后 B 应占据原先 A 的保存格子");
        assert_eq!(
            app.presented_node_bounds(ids[0]).expect("A 应展示"),
            after_a,
            "宽画布下展示应跟随交换后的保存格子"
        );
        assert_eq!(
            app.presented_node_bounds(ids[1]).expect("B 应展示"),
            after_b,
            "宽画布下展示应跟随交换后的保存格子"
        );
    });
}
