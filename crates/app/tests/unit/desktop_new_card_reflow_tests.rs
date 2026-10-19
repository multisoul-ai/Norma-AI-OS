use super::*;
use canvas::{
    compute_configurable_grid_layout, CanvasNodeKind, CanvasRect, RectNode, TermSpec, Vec2,
};
use gpui::{px, size, AppContext, TestAppContext};
use std::time::Duration;

/// 场景：嵌入项目已有两张手动摆放的卡片，新增第三张时默认格子与旧卡片相交。
///
/// 数据构造：945px 宽的真实项目画布；两张终端位于左侧和中部，另有一个隐藏的旧 Rect。
/// 执行过程：按新增终端的真实路径计算下一格，插入第三张，再应用新增卡片排版。
/// 预期结果：三张可见卡片共同重排且互不重叠；隐藏节点的保存坐标不变。
#[test]
fn adding_card_reflows_visible_embedded_nodes_without_overlap() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("New Card Layout").expect("应创建工作空间");
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
        assert!(app.desktop_host.is_some(), "被测画布必须运行于桌面项目内");
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let viewport = app.visible_canvas_rect(window);
        let zoom = app.runtime.document.viewport.zoom;
        let hidden_bounds = CanvasRect::new(
            viewport.origin.x + 10.0,
            viewport.origin.y + 10.0,
            100.0,
            100.0,
        );
        let hidden = app.runtime.document.add_node(
            CanvasNodeKind::Rect(RectNode { color: 0x123456 }),
            hidden_bounds,
        );
        let first = app.runtime.document.add_node(
            CanvasNodeKind::Term(TermSpec),
            CanvasRect::new(viewport.origin.x, viewport.origin.y, 350.0, 280.0),
        );
        let second = app.runtime.document.add_node(
            CanvasNodeKind::Term(TermSpec),
            CanvasRect::new(viewport.origin.x + 600.0, viewport.origin.y, 340.0, 280.0),
        );
        let third_bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
        let third = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), third_bounds);
        let before_second = app
            .runtime
            .document
            .node(second)
            .expect("第二张卡片应存在")
            .bounds;
        assert!(
            third_bounds.origin.x < before_second.origin.x + before_second.size.x,
            "夹具必须复现新增卡片与第二张卡片横向相交"
        );
        assert!(
            third_bounds.origin.y < before_second.origin.y + before_second.size.y
                && before_second.origin.y < third_bounds.origin.y + third_bounds.size.y,
            "夹具必须复现新增卡片与第二张卡片纵向相交"
        );

        app.apply_layout_for_new_node_for_area(third, viewport, zoom);

        let ids = [first, second, third];
        // 稳定画布 = 1280px 窗口减两侧各 14px、顶部 64px、底部 91px。
        // 抽屉只改变展示宽度，不能把单列布局写成长期保存的卡片坐标。
        let stable_area = CanvasRect::new(
            viewport.origin.x,
            viewport.origin.y,
            1252.0 / zoom,
            645.0 / zoom,
        );
        let expected = compute_configurable_grid_layout(
            &ids,
            stable_area,
            app.layout_strategy.grid_config_for_zoom(zoom),
        );
        for assignment in expected {
            assert_eq!(
                app.runtime
                    .document
                    .node(assignment.node_id)
                    .expect("可见卡片应保留")
                    .bounds,
                assignment.bounds,
                "新增卡片后所有可见卡片应按当前画布网格共同重排"
            );
        }
        assert_eq!(
            app.runtime
                .document
                .node(hidden)
                .expect("隐藏节点应保留")
                .bounds,
            hidden_bounds,
            "隐藏的旧节点不得占用格子或改变保存坐标"
        );
        let shown: Vec<_> = ids
            .iter()
            .map(|id| {
                app.presented_node_bounds(*id)
                    .expect("可见卡片应有展示边界")
            })
            .collect();
        for (index, card) in shown.iter().enumerate() {
            assert!(
                card.origin.x >= viewport.origin.x
                    && card.origin.x + card.size.x <= viewport.origin.x + viewport.size.x,
                "第 {index} 张卡片必须完整位于当前抽屉之间"
            );
            for next in shown.iter().skip(index + 1) {
                let overlaps = card.origin.x < next.origin.x + next.size.x
                    && next.origin.x < card.origin.x + card.size.x
                    && card.origin.y < next.origin.y + next.size.y
                    && next.origin.y < card.origin.y + card.size.y;
                assert!(!overlaps, "新增后的可见卡片不能在画布上互相重叠");
            }
        }
        app.layout_strategy.track_count = 2;
        app.refresh_desktop_presentation_for_area(viewport, zoom);
        let expected_two_columns = compute_configurable_grid_layout(
            &ids,
            viewport,
            app.layout_strategy.grid_config_for_zoom(zoom),
        );
        for assignment in expected_two_columns {
            assert_eq!(
                app.presented_node_bounds(assignment.node_id)
                    .expect("自定义布局中的卡片应显示"),
                assignment.bounds,
                "溢出画布的展示排版应尊重当前项目设置的两列布局"
            );
        }
        app.layout_strategy.track_count = 3;
        app.refresh_desktop_presentation_for_area(viewport, zoom);

        // 双抽屉下新增第四张卡片后，收起右侧抽屉应恢复宽画布的并排布局。
        app.sync_desktop_embedded_canvas_size(Vec2::new(472.0, 710.0), window);
        let narrow = app.visible_canvas_rect(window);
        let fourth_bounds = app.next_tiled_node_bounds_for_area(narrow, zoom);
        let fourth = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), fourth_bounds);
        app.apply_layout_for_new_node_for_area(fourth, narrow, zoom);
        assert!(
            crate::app::embedded_canvas::embedded_presentation_overflows_vertically(
                &app.desktop_presentation_bounds,
                narrow.size.y,
            ),
            "窄画布下方有卡片时应启用画布的纵向滚轮平移"
        );
        let fourth_on_screen_before_pan = app.runtime.document.viewport.canvas_to_screen(
            app.presented_node_bounds(fourth)
                .expect("第四张卡片应显示")
                .origin,
        );
        app.runtime.document.viewport.pan.y -= 180.0;
        app.refresh_desktop_presentation_for_area(app.visible_canvas_rect(window), zoom);
        let fourth_on_screen_after_pan = app.runtime.document.viewport.canvas_to_screen(
            app.presented_node_bounds(fourth)
                .expect("平移后第四张卡片应显示")
                .origin,
        );
        assert_eq!(
            fourth_on_screen_after_pan.y,
            fourth_on_screen_before_pan.y - 180.0,
            "窄画布平移后下方卡片必须随视口移动，不能被展示重排抵消"
        );
        app.runtime.document.viewport.pan.y += 180.0;
        app.refresh_desktop_presentation_for_area(app.visible_canvas_rect(window), zoom);
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let first_visible = app.presented_node_bounds(first).expect("首张卡片应显示");
        let second_visible = app.presented_node_bounds(second).expect("第二张卡片应显示");
        assert!(
            second_visible.origin.x >= first_visible.origin.x + first_visible.size.x,
            "收起右抽屉后，窄画布新增卡片应恢复多列布局而非保留单列保存坐标"
        );
        assert_eq!(
            app.runtime
                .document
                .node(hidden)
                .expect("隐藏节点应保留")
                .bounds,
            hidden_bounds,
            "再次排版后隐藏节点坐标仍不应变化"
        );

        // 场景：整个 App 窗口缩小后，稳定保存区域也可能小于单列阈值。
        // 数据构造：模拟缩窗所得 500px 保存宽度，以及双抽屉的 472px 展示宽度。
        // 执行过程：在窄窗口中新增第五张终端，再读取保存坐标和展示坐标。
        // 预期结果：保存坐标仍为多列；眼前窄画布单列展示，放大后能恢复。
        app.sync_desktop_embedded_canvas_size(Vec2::new(472.0, 710.0), window);
        app.desktop_embedded_reflow_size = Some(Vec2::new(500.0, 645.0));
        app.runtime.document.viewport.pan.y -= 180.0;
        let narrow_window_area = app.visible_canvas_rect(window);
        let fifth_bounds = app.next_tiled_node_bounds_for_area(narrow_window_area, zoom);
        let fifth = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), fifth_bounds);
        app.apply_layout_for_new_node_for_area(fifth, narrow_window_area, zoom);
        let ids_after_pan = [first, second, third, fourth, fifth];
        let expected_after_pan = compute_configurable_grid_layout(
            &ids_after_pan,
            CanvasRect::new(
                viewport.origin.x,
                viewport.origin.y,
                945.0 / zoom,
                645.0 / zoom,
            ),
            app.layout_strategy.grid_config_for_zoom(zoom),
        );
        for assignment in expected_after_pan {
            assert_eq!(
                app.runtime
                    .document
                    .node(assignment.node_id)
                    .expect("卡片应保存")
                    .bounds,
                assignment.bounds,
                "滚动画布后新增卡片不能把保存网格跟着平移"
            );
        }
        let saved_first = app
            .runtime
            .document
            .node(first)
            .expect("首张卡片应保存")
            .bounds;
        let saved_second = app
            .runtime
            .document
            .node(second)
            .expect("第二张卡片应保存")
            .bounds;
        assert!(
            saved_second.origin.x >= saved_first.origin.x + saved_first.size.x,
            "缩窗新增卡片不能把项目保存布局永久压成单列"
        );
        let shown_first = app.presented_node_bounds(first).expect("首张卡片应展示");
        let shown_second = app.presented_node_bounds(second).expect("第二张卡片应展示");
        assert!(
            shown_second.origin.y >= shown_first.origin.y + shown_first.size.y,
            "缩窗后的当前展示仍应按窄画布单列排列"
        );
        app.runtime.document.viewport.pan.y += 180.0;
        app.refresh_desktop_presentation_for_area(app.visible_canvas_rect(window), zoom);
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let restored_first = app
            .presented_node_bounds(first)
            .expect("首张卡片应恢复展示");
        let restored_second = app
            .presented_node_bounds(second)
            .expect("第二张卡片应恢复展示");
        assert!(
            restored_second.origin.x >= restored_first.origin.x + restored_first.size.x,
            "缩窗新增卡片后放大画布应恢复并排布局"
        );

        // 场景：项目正在主次布局中，随后又创建了新卡片。
        // 数据构造：以当前保存网格作主次布局的恢复快照，再切回窄抽屉画布。
        // 执行过程：新增第六张终端并应用自动排版。
        // 预期结果：退出旧主次模式，避免下次切换时拿缺少新卡片的快照重排隐藏节点。
        let original_bounds = app
            .runtime
            .document
            .nodes()
            .iter()
            .map(|node| storage::LayoutOriginalBounds {
                node_id: node.id,
                bounds: node.bounds,
            })
            .collect();
        assert!(
            app.master_layout.restore_from_snapshot(
                storage::LayoutStateSnapshot {
                    mode: storage::LayoutMode::MasterDetail,
                    master_id: Some(first),
                    original_bounds,
                },
                &app.runtime,
            ),
            "应建立有效的主次布局快照"
        );
        app.sync_desktop_embedded_canvas_size(Vec2::new(472.0, 710.0), window);
        let master_narrow_area = app.visible_canvas_rect(window);
        let sixth_bounds = app.next_tiled_node_bounds_for_area(master_narrow_area, zoom);
        let sixth = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), sixth_bounds);
        app.apply_layout_for_new_node_for_area(sixth, master_narrow_area, zoom);
        assert!(
            !app.master_layout.is_active(),
            "自动网格重排后应退出过期主次布局，防止再次退出时按窄画布误写保存坐标"
        );
        assert_eq!(
            app.runtime
                .document
                .node(hidden)
                .expect("隐藏节点应保留")
                .bounds,
            hidden_bounds,
            "旧主次模式退出时隐藏节点不能被重排"
        );
    });
}

/// 场景：项目画布已有五张卡片且展示网格比保存网格窄，新增第六张卡片。
/// 数据构造：1280×800 窗口、945px 嵌入画布、五张终端；保存网格宽于展示网格。
/// 执行过程：以正常动效新增第六张，推进完整布局动效后读取实际绘制矩形。
/// 预期结果：旧卡片仍贴合展示网格，且第六张卡片不与同排旧卡片重叠。
#[test]
fn adding_sixth_card_keeps_motion_in_embedded_presentation_grid() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let project_root = temp.path().join("project");
    std::fs::create_dir(&project_root).expect("应创建项目目录");
    let paths = storage::DatabasePaths::isolated_test(&temp.path().join("data"));
    let store = storage::AppStore::initialize(&paths).expect("应初始化存储");
    let catalog = store.desktop_workspace_store();
    let workspace = catalog.create("Six Cards").expect("应创建工作空间");
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
    cx.run_until_parked();
    let project = host.read_with(cx, |app, _| {
        app.expanded_desktop_project().expect("项目应已展开")
    });
    project.update_in(cx, |app, window, _| {
        app.sync_desktop_embedded_canvas_size(Vec2::new(945.0, 710.0), window);
        let viewport = app.visible_canvas_rect(window);
        let zoom = app.runtime.document.viewport.zoom;
        let mut ids = Vec::new();
        for _ in 0..5 {
            let bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
            let id = app
                .runtime
                .document
                .add_node(CanvasNodeKind::Term(TermSpec), bounds);
            app.apply_layout_for_new_node_for_area(id, viewport, zoom);
            ids.push(id);
        }
        let old_map = app.desktop_presentation_bounds.clone();
        assert_eq!(old_map.len(), 5, "夹具须有五张卡片的展示网格");
        app.motion.reduce_motion = false;
        let bounds = app.next_tiled_node_bounds_for_area(viewport, zoom);
        let sixth = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), bounds);
        app.motion.spawn_node(sixth, bounds);
        app.apply_layout_for_new_node_for_area(sixth, viewport, zoom);
        assert_eq!(
            app.desktop_presentation_bounds.get(&ids[4]),
            old_map.get(&ids[4]),
            "第五张旧卡片的展示目标应保持不变，以复现缓存命中"
        );
        app.motion.tick(Duration::from_millis(500));
        let old_shown = app.presented_node_bounds(ids[4]).expect("旧卡片须展示");
        let old_drawn = app.motion.current_node_rect(ids[4], old_shown);
        let new_shown = app.presented_node_bounds(sixth).expect("新卡片须展示");
        let new_drawn = app.motion.current_node_rect(sixth, new_shown);
        assert_eq!(old_drawn, old_shown, "旧卡片动效结束后仍应贴合展示格子");
        assert_eq!(new_drawn, new_shown, "新卡片应贴合展示格子");
        let overlaps = old_drawn.origin.x < new_drawn.origin.x + new_drawn.size.x
            && new_drawn.origin.x < old_drawn.origin.x + old_drawn.size.x
            && old_drawn.origin.y < new_drawn.origin.y + new_drawn.size.y
            && new_drawn.origin.y < old_drawn.origin.y + old_drawn.size.y;
        assert!(!overlaps, "第六张卡片不得盖住同排旧卡片");
    });
}
