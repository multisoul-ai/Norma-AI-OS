use super::*;
use canvas::{CanvasNodeKind, TermSpec};

/// 场景：桌面可见终端作为 Master 时，旧 Browser 不得占据 detail 格子。
/// 数据构造：1454×925 画布、24px 间距，两张终端和一张隐藏 Browser；
/// 唯一可见 detail 高度应为 925−2×24=877，而非被隐藏 Browser 再分一半。
/// 执行过程：通过生产 enter_master_layout_for_area 应用主次布局并读取文档边界。
/// 预期结果：合法 Master 保持有效，detail 占满可用高度，隐藏节点边界不变。
#[test]
fn desktop_master_ignores_hidden_detail_nodes() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut state = MasterLayoutState {
        chrome_in_parent: true,
        ..MasterLayoutState::default()
    };
    let initial = CanvasRect::new(10.0, 20.0, 300.0, 200.0);
    let master = runtime
        .document
        .add_node(CanvasNodeKind::Term(TermSpec), initial);
    let hidden = runtime.spawn_browser("https://example.com".into(), None, initial);
    let detail = runtime
        .document
        .add_node(CanvasNodeKind::Term(TermSpec), initial);
    assert!(
        enter_master_layout_for_area(
            &mut state,
            &mut runtime,
            &mut motion,
            master,
            CanvasRect::new(0.0, 0.0, 1454.0, 925.0),
            1.0
        ),
        "可见终端应能进入 Master 布局"
    );
    assert_eq!(state.master_id(), Some(master), "合法 Master 应保持有效");
    assert_eq!(
        runtime
            .document
            .node(detail)
            .expect("detail 应存在")
            .bounds
            .size
            .y,
        877.0,
        "隐藏 Browser 不得分走可见 detail 的高度"
    );
    assert_eq!(
        runtime
            .document
            .node(hidden)
            .expect("Browser 应保留")
            .bounds,
        initial,
        "隐藏节点不应参与布局或被删除"
    );
}

/// 场景：桌面拒绝将不绘制的 Browser 作为 Master，包括已有项目的恢复。
/// 数据构造：一张终端、一张隐藏网页和旧 MasterDetail 快照。
/// 执行过程：分别调用生产进入入口与快照恢复入口。
/// 预期结果：两条入口都拒绝隐藏 Master，终端边界和隐藏节点均保留。
#[test]
fn desktop_master_rejects_hidden_target_and_snapshot() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut state = MasterLayoutState {
        chrome_in_parent: true,
        ..Default::default()
    };
    let initial = CanvasRect::new(10.0, 20.0, 300.0, 200.0);
    let term = runtime
        .document
        .add_node(CanvasNodeKind::Term(TermSpec), initial);
    let hidden = runtime.spawn_browser("https://example.com".into(), None, initial);
    assert!(
        !enter_master_layout_for_area(
            &mut state,
            &mut runtime,
            &mut motion,
            hidden,
            CanvasRect::new(0.0, 0.0, 1454.0, 925.0),
            1.0
        ),
        "不可进入隐藏 Master"
    );
    assert!(
        !state.restore_from_snapshot(
            LayoutStateSnapshot {
                mode: LayoutMode::MasterDetail,
                master_id: Some(hidden),
                original_bounds: vec![],
            },
            &runtime
        ),
        "不可恢复隐藏 Master"
    );
    assert!(!state.is_active(), "无效 Master 必须退出");
    assert_eq!(
        runtime.document.node(term).expect("终端存在").bounds,
        initial,
        "拒绝隐藏 Master 不应改坏终端布局"
    );
    assert!(
        runtime.document.node(hidden).is_some(),
        "拒绝布局不能以删除内容代替"
    );
}

/// 场景：Master 期间新增隐藏内容，退出时仍恢复用户原来的可见卡片坐标。
/// 数据构造：两张有自定义位置的终端，进入主次布局后新增 Browser。
/// 执行过程：调用生产进入、退出布局函数。
/// 预期结果：隐藏节点不算布局成员变化，原坐标完整恢复，而非触发平铺重排。
#[test]
fn desktop_master_exit_ignores_new_hidden_nodes() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut state = MasterLayoutState {
        chrome_in_parent: true,
        ..Default::default()
    };
    let first = CanvasRect::new(40.0, 30.0, 340.0, 230.0);
    let second = CanvasRect::new(420.0, 50.0, 350.0, 240.0);
    let master = runtime
        .document
        .add_node(CanvasNodeKind::Term(TermSpec), first);
    let detail = runtime
        .document
        .add_node(CanvasNodeKind::Term(TermSpec), second);
    let area = CanvasRect::new(0.0, 0.0, 1454.0, 925.0);
    assert!(
        enter_master_layout_for_area(&mut state, &mut runtime, &mut motion, master, area, 1.0),
        "进入可见 Master"
    );
    runtime.spawn_browser("https://example.com".into(), None, first);
    assert!(
        exit_master_layout_for_area(
            &mut state,
            &mut runtime,
            &mut motion,
            area,
            1.0,
            CanvasLayoutStrategy::default()
        ),
        "退出 Master"
    );
    assert_eq!(
        runtime.document.node(master).expect("Master 存在").bounds,
        first,
        "主卡片应恢复自定义位置"
    );
    assert_eq!(
        runtime.document.node(detail).expect("detail 存在").bounds,
        second,
        "detail 应恢复自定义位置"
    );
    assert!(!state.is_active(), "退出后不得残留 Master");
}

/// 场景：旧项目的 Master 是已被 prune 删除的网页，或仍保留但不绘制的 Ego。
/// 数据构造：模拟恢复顺序（先快照、后桌面绑定），终端存有 62px 的坏布局。
/// 执行过程：通过生产首帧尺寸同步获取 1454×925 画布，再检查文档和展示边界。
/// 预期结果：两条恢复路径均清除隐藏 Master 并重排；修复只消费一次且标记需保存。
#[test]
fn desktop_master_first_size_sync_repairs_missing_or_hidden_master() {
    use gpui::TestAppContext;
    for keep_hidden in [false, true] {
        let temp = tempfile::tempdir().expect("隔离目录");
        let paths = storage::DatabasePaths::isolated_test(temp.path());
        let store = storage::AppStore::initialize(&paths).expect("隔离存储");
        let mut test = TestAppContext::single();
        let (app, cx) =
            test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
        app.update_in(cx, |app, window, cx| {
            app.desktop_host = Some(cx.entity().downgrade());
            app.motion.reduce_motion = true;
            app.layout_strategy.chrome_in_parent = true;
            let strip = CanvasRect::new(969.0, 8.0, 476.0, 62.0);
            let term = app
                .runtime
                .document
                .add_node(CanvasNodeKind::Term(TermSpec), strip);
            let hidden = app.runtime.spawn_ego_browser(None, strip);
            if !keep_hidden {
                app.runtime.remove_node(hidden);
            }
            app.master_layout.restore_from_snapshot(
                LayoutStateSnapshot {
                    mode: LayoutMode::MasterDetail,
                    master_id: Some(hidden),
                    original_bounds: vec![],
                },
                &app.runtime,
            );
            app.master_layout.bind_desktop(&app.runtime);
            // 后台项目可能先保存再展开；中途持久化不能消掉待修复信息。
            let pending = app.master_layout.to_snapshot();
            app.master_layout = MasterLayoutState::default();
            app.master_layout
                .restore_from_snapshot(pending, &app.runtime);
            app.master_layout.bind_desktop(&app.runtime);
            app.sync_desktop_embedded_canvas_size(canvas::Vec2::new(1454.0, 925.0), window);
            assert!(
                !app.master_layout.is_active(),
                "旧隐藏 Master 不得恢复为活跃状态"
            );
            assert!(
                app.persistence.has_pending_work(),
                "旧布局修复必须进入异步持久化队列"
            );
            let repaired = app.runtime.document.node(term).expect("终端仍存在").bounds;
            assert!(
                repaired.size.y > 200.0,
                "首帧须修复细条，而非只清 Master 标志：{repaired:?}"
            );
            assert!(
                repaired.origin.x < 100.0,
                "可见首卡应占第一格，不留隐藏 Master 空档"
            );
            assert_eq!(
                app.presented_node_bounds(term),
                Some(repaired),
                "实际展示不能仍使用旧细条边界"
            );
            assert!(
                !app.master_layout.take_repair_pending(),
                "修复标记只能消费一次"
            );
        });
    }
}

/// 场景：合法可见 Master 带隐藏 detail 的旧快照，在展开前可能再次保存。
/// 数据构造：可见主终端、detail 终端与隐藏网页，快照包含三者的原坐标。
/// 执行过程：恢复、绑定桌面、快照往返，再应用生产 Master 布局。
/// 预期结果：仍需修复隐藏占位；保留可见 Master，detail 获得 925-2*8=909px 全高。
#[test]
fn desktop_master_pending_snapshot_preserves_visible_master() {
    let mut runtime = Runtime::new();
    let bounds = CanvasRect::new(0.0, 0.0, 300.0, 200.0);
    let master = runtime
        .document
        .add_node(CanvasNodeKind::Term(TermSpec), bounds);
    let detail = runtime
        .document
        .add_node(CanvasNodeKind::Term(TermSpec), bounds);
    let hidden = runtime.spawn_browser("https://example.com/".into(), None, bounds);
    let snapshot = LayoutStateSnapshot {
        mode: LayoutMode::MasterDetail,
        master_id: Some(master),
        original_bounds: [master, detail, hidden]
            .into_iter()
            .map(|node_id| storage::LayoutOriginalBounds { node_id, bounds })
            .collect(),
    };
    let mut state = MasterLayoutState::default();
    assert!(
        state.restore_from_snapshot(snapshot, &runtime),
        "经典画布可恢复旧 Master"
    );
    state.bind_desktop(&runtime);
    let pending = state.to_snapshot();
    let mut restored = MasterLayoutState::default();
    assert!(
        restored.restore_from_snapshot(pending, &runtime),
        "可见 Master 不能被降为平铺"
    );
    restored.bind_desktop(&runtime);
    assert!(
        restored.clone().take_repair_pending(),
        "中间保存不能消掉隐藏 detail 修复原因"
    );
    let mut motion = MotionStore::new(true);
    assert!(
        reapply_master_layout_for_area(
            &mut restored,
            &mut runtime,
            &mut motion,
            CanvasRect::new(0.0, 0.0, 1454.0, 925.0),
            1.0
        ),
        "可见 Master 应正常应用"
    );
    assert_eq!(restored.master_id(), Some(master), "必须保留合法 Master");
    assert_eq!(
        runtime.document.node(detail).expect("detail").bounds.size.y,
        877.0,
        "隐藏 detail 不得占位"
    );
    assert!(
        !restored
            .to_snapshot()
            .original_bounds
            .iter()
            .any(|entry| entry.node_id == hidden),
        "修复完成后的快照不再保留隐藏布局成员"
    );
}

/// 场景：旧版进入可见 Master 后新增网页，切到网页再切回终端；首次快照不含网页。
/// 数据构造：用经典画布的生产主次算法保留该切换顺序，再模拟加载器清除网页。
/// 执行过程：合法 Master 快照恢复、绑定桌面并执行首帧尺寸同步，最后退出 Master。
/// 预期结果：唯一可见 detail 与 Master 等高，不再给已清除网页留空档；退出恢复原坐标。
#[test]
fn desktop_master_first_sync_reflows_details_missing_from_original_snapshot() {
    use gpui::TestAppContext;
    let temp = tempfile::tempdir().expect("隔离目录");
    let paths = storage::DatabasePaths::isolated_test(temp.path());
    let store = storage::AppStore::initialize(&paths).expect("隔离存储");
    let mut test = TestAppContext::single();
    let (app, cx) =
        test.add_window_view(|_, cx| MultisoulApp::new_with_database_context(cx, paths, store));
    app.update_in(cx, |app, window, cx| {
        app.desktop_host = Some(cx.entity().downgrade());
        app.motion.reduce_motion = true;
        let first = CanvasRect::new(40.0, 30.0, 340.0, 230.0);
        let second = CanvasRect::new(420.0, 50.0, 350.0, 240.0);
        let master = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), first);
        let detail = app
            .runtime
            .document
            .add_node(CanvasNodeKind::Term(TermSpec), second);
        let area = CanvasRect::new(0.0, 0.0, 1454.0, 925.0);
        assert!(
            enter_master_layout_for_area(
                &mut app.master_layout,
                &mut app.runtime,
                &mut app.motion,
                master,
                area,
                1.0
            ),
            "经典画布先进入可见 Master"
        );
        let hidden = app
            .runtime
            .spawn_browser("https://example.com/".into(), None, first);
        for target in [hidden, master] {
            assert!(
                switch_master_layout_for_area(
                    &mut app.master_layout,
                    &mut app.runtime,
                    &mut app.motion,
                    target,
                    area,
                    1.0
                ),
                "生产切换必须保留首次 original_bounds"
            );
        }
        let snapshot = app.master_layout.to_snapshot();
        assert!(
            !snapshot
                .original_bounds
                .iter()
                .any(|entry| entry.node_id == hidden),
            "新网页不在首次原始坐标中，这是恢复检测的缺口"
        );
        assert!(
            app.runtime
                .document
                .node(detail)
                .expect("detail")
                .bounds
                .size
                .y
                < app
                    .runtime
                    .document
                    .node(master)
                    .expect("Master")
                    .bounds
                    .size
                    .y,
            "旧算法必须真的为网页分走 detail 高度"
        );
        app.runtime.remove_node(hidden);
        app.master_layout = MasterLayoutState::default();
        assert!(
            app.master_layout
                .restore_from_snapshot(snapshot, &app.runtime),
            "可见 Master 仍合法，不应被降为平铺"
        );
        app.master_layout.bind_desktop(&app.runtime);
        app.layout_strategy.chrome_in_parent = true;
        app.sync_desktop_embedded_canvas_size(canvas::Vec2::new(1454.0, 925.0), window);
        assert_eq!(
            app.master_layout.master_id(),
            Some(master),
            "首帧保留合法 Master"
        );
        assert_eq!(
            app.runtime
                .document
                .node(detail)
                .expect("detail")
                .bounds
                .size
                .y,
            app.runtime
                .document
                .node(master)
                .expect("Master")
                .bounds
                .size
                .y,
            "只剩一个可见 detail 时必须占满主区域同等高度"
        );
        assert!(app.persistence.has_pending_work(), "首帧修复应持久化");
        assert!(
            exit_master_layout_for_area(
                &mut app.master_layout,
                &mut app.runtime,
                &mut app.motion,
                area,
                1.0,
                app.layout_strategy
            ),
            "合法 Master 可以退出"
        );
        assert_eq!(
            app.runtime.document.node(master).expect("Master").bounds,
            first,
            "退出应恢复主卡片原坐标"
        );
        assert_eq!(
            app.runtime.document.node(detail).expect("detail").bounds,
            second,
            "退出应恢复 detail 原坐标"
        );
    });
}
