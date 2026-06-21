use super::*;
use crate::master_layout::{
    close_node_and_update_layout_for_area, enter_master_layout_for_area,
    master_target_from_hover_or_selection, restore_master_layout_for_area, MasterLayoutState,
};
use canvas::{
    compute_master_detail_layout, compute_responsive_grid_layout, CanvasNodeKind, RectNode,
};

fn add_rect(runtime: &mut Runtime, color: u32, bounds: CanvasRect) -> NodeId {
    runtime
        .document
        .add_node(CanvasNodeKind::Rect(RectNode { color }), bounds)
}

/// Master 目标解析：hover 优先于 selection；没有 hover 时使用当前选中卡片兜底。
///
/// 数据构造（含关键数值的推导过程）：
///   A id=1，B id=2（Document::add_node 从 1 递增）
///   selection = {A}
///   hovered_node = Some(B)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. master_target_from_hover_or_selection(Some(B), runtime) → 命中 hover
///   2. master_target_from_hover_or_selection(None, runtime) → 无 hover，回退 selection
///
/// 预期结果：
///   - 正断言：有 hover 时返回 B
///   - 正断言：无 hover 时返回 A
///   - 负断言：有 hover 时不应返回 selection 中的 A
#[test]
fn test_master_layout_target_prefers_hover_over_selection() {
    let mut runtime = Runtime::new();
    let a = add_rect(
        &mut runtime,
        0x4080ff,
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let b = add_rect(
        &mut runtime,
        0xff8040,
        CanvasRect::new(160.0, 0.0, 100.0, 80.0),
    );
    runtime.selection.select_only(a);

    let hover_target = master_target_from_hover_or_selection(Some(b), &runtime);
    let selection_target = master_target_from_hover_or_selection(None, &runtime);

    assert_eq!(hover_target, Some(b), "hovered B 应优先成为 master target");
    assert_eq!(
        selection_target,
        Some(a),
        "没有 hover 时应回退使用当前 selection A"
    );
    assert_ne!(
        hover_target,
        Some(a),
        "有 hover 时不应误用 selection 中的 A"
    );
}

/// Master 布局切换：进入时保存原始 bounds 并应用 master/detail，再次切换时精确恢复。
///
/// 数据构造（含关键数值的推导过程）：
///   A/B/C/D 初始 bounds 分别为 x=0/120/240/360，宽高均为 100x80
///   area = origin(0,0), size(1000x600)
///   zoom = 1.0 → workspace_reflow_config_for_zoom 使用 16px gap 与 96/86/32/84 safe area
///   master = B，期望布局由 compute_master_detail_layout([A,B,C,D], B, area, config) 计算
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enter_master_layout_for_area(B) → 保存四张卡原始 bounds，并写入 master/detail bounds
///   2. restore_master_layout_for_area() → 根据快照恢复仍存在节点
///   3. 查询 A/B/C/D bounds 与状态
///
/// 预期结果：
///   - 正断言：进入成功且 state active master 为 B
///   - 正断言：B bounds 等于 master/detail 计算结果的 master bounds
///   - 正断言：恢复后 A/B/C/D 都回到原始 bounds
///   - 负断言：恢复后 B 不应继续停留在 master bounds
#[test]
fn test_master_layout_enter_saves_snapshot_and_restore_returns_original_bounds() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut state = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1000.0, 600.0);
    let zoom = 1.0;
    let a_original = CanvasRect::new(0.0, 0.0, 100.0, 80.0);
    let b_original = CanvasRect::new(120.0, 0.0, 100.0, 80.0);
    let c_original = CanvasRect::new(240.0, 0.0, 100.0, 80.0);
    let d_original = CanvasRect::new(360.0, 0.0, 100.0, 80.0);
    let a = add_rect(&mut runtime, 0x4080ff, a_original);
    let b = add_rect(&mut runtime, 0xff8040, b_original);
    let c = add_rect(&mut runtime, 0x35d07f, c_original);
    let d = add_rect(&mut runtime, 0xff40c0, d_original);

    let entered =
        enter_master_layout_for_area(&mut state, &mut runtime, &mut motion, b, area, zoom);
    let expected = compute_master_detail_layout(
        &[a, b, c, d],
        b,
        area,
        workspace_reflow_config_for_zoom(zoom),
    );
    let b_master_bounds = expected[0].bounds;
    let b_after_enter = runtime
        .document
        .node(b)
        .expect("B 应在进入 master 后继续存在")
        .bounds;

    restore_master_layout_for_area(&mut state, &mut runtime, &mut motion, zoom);

    assert!(entered, "B 存在时 enter_master_layout_for_area 应成功");
    assert_eq!(
        b_after_enter, b_master_bounds,
        "B 应被移动到 master/detail 布局的左侧 master 区"
    );
    assert_eq!(
        runtime.document.node(a).expect("A 应仍存在").bounds,
        a_original,
        "恢复后 A 应回到进入 master 前的原始 bounds"
    );
    assert_eq!(
        runtime.document.node(b).expect("B 应仍存在").bounds,
        b_original,
        "恢复后 B 应回到进入 master 前的原始 bounds"
    );
    assert_eq!(
        runtime.document.node(c).expect("C 应仍存在").bounds,
        c_original,
        "恢复后 C 应回到进入 master 前的原始 bounds"
    );
    assert_eq!(
        runtime.document.node(d).expect("D 应仍存在").bounds,
        d_original,
        "恢复后 D 应回到进入 master 前的原始 bounds"
    );
    assert_ne!(
        runtime.document.node(b).expect("B 应仍存在").bounds,
        b_master_bounds,
        "恢复后 B 不应继续停留在 master 区 bounds"
    );
}

/// Master 布局删除 detail：删除右侧节点后保持 master 模式，并让剩余右侧卡片自适应重排。
///
/// 数据构造（含关键数值的推导过程）：
///   A/B/C/D 四张卡，master=B，删除 detail C
///   area = 1000x600, zoom = 1.0
///   删除后 ordered_ids = [A,B,D]，期望仍用 compute_master_detail_layout([A,B,D], B, area, config)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enter_master_layout_for_area(B)
///   2. close_node_and_update_layout_for_area(C)
///   3. 查询 state、C、B/D bounds
///
/// 预期结果：
///   - 正断言：C 被删除且 state 仍 active master=B
///   - 正断言：B 保持在 master 区，D 进入重算后的右侧 detail 区
///   - 负断言：C 不应继续存在
#[test]
fn test_master_layout_delete_detail_keeps_master_active_and_reflows_details() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut state = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1000.0, 600.0);
    let zoom = 1.0;
    let a = add_rect(
        &mut runtime,
        0x4080ff,
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let b = add_rect(
        &mut runtime,
        0xff8040,
        CanvasRect::new(120.0, 0.0, 100.0, 80.0),
    );
    let c = add_rect(
        &mut runtime,
        0x35d07f,
        CanvasRect::new(240.0, 0.0, 100.0, 80.0),
    );
    let d = add_rect(
        &mut runtime,
        0xff40c0,
        CanvasRect::new(360.0, 0.0, 100.0, 80.0),
    );
    assert!(
        enter_master_layout_for_area(&mut state, &mut runtime, &mut motion, b, area, zoom),
        "进入 master 布局应成功，供删除 detail 场景使用"
    );

    let removed =
        close_node_and_update_layout_for_area(&mut runtime, &mut motion, &mut state, c, area, zoom);
    let expected =
        compute_master_detail_layout(&[a, b, d], b, area, workspace_reflow_config_for_zoom(zoom));

    assert!(removed, "删除 detail C 应返回 true");
    assert_eq!(
        state.master_id(),
        Some(b),
        "删除非 master 节点后应继续保持 master=B"
    );
    assert!(
        runtime.document.node(c).is_none(),
        "C 应从 document 中删除，不能继续渲染或参与布局"
    );
    assert_eq!(
        runtime.document.node(b).expect("B 应仍存在").bounds,
        expected[0].bounds,
        "B 应保持在 master 区"
    );
    assert_eq!(
        runtime.document.node(d).expect("D 应仍存在").bounds,
        expected[2].bounds,
        "D 应按删除 C 后的右侧 detail 布局重排"
    );
}

/// Master 布局删除 master：删除左侧 master 后退出 master 模式，并对剩余卡片执行普通 reflow。
///
/// 数据构造（含关键数值的推导过程）：
///   A/B/C 三张卡，master=B，删除 B
///   area = 1000x600, zoom = 1.0
///   删除后普通 ordered_ids = [A,C]，期望使用 compute_responsive_grid_layout([A,C], area, config)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enter_master_layout_for_area(B)
///   2. close_node_and_update_layout_for_area(B)
///   3. 查询 state、B、A/C bounds
///
/// 预期结果：
///   - 正断言：B 被删除且 state inactive
///   - 正断言：A/C 使用普通响应式 reflow
///   - 负断言：B 不应继续存在
#[test]
fn test_master_layout_delete_master_exits_and_reflows_remaining_cards() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut state = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1000.0, 600.0);
    let zoom = 1.0;
    let a = add_rect(
        &mut runtime,
        0x4080ff,
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let b = add_rect(
        &mut runtime,
        0xff8040,
        CanvasRect::new(120.0, 0.0, 100.0, 80.0),
    );
    let c = add_rect(
        &mut runtime,
        0x35d07f,
        CanvasRect::new(240.0, 0.0, 100.0, 80.0),
    );
    assert!(
        enter_master_layout_for_area(&mut state, &mut runtime, &mut motion, b, area, zoom),
        "进入 master 布局应成功，供删除 master 场景使用"
    );

    let removed =
        close_node_and_update_layout_for_area(&mut runtime, &mut motion, &mut state, b, area, zoom);
    let expected =
        compute_responsive_grid_layout(&[a, c], area, workspace_reflow_config_for_zoom(zoom));

    assert!(removed, "删除 master B 应返回 true");
    assert!(
        !state.is_active(),
        "删除 master 后应退出 master layout，避免持有失效 master id"
    );
    assert!(
        runtime.document.node(b).is_none(),
        "B 应从 document 删除，不能继续作为 master 存在"
    );
    assert_eq!(
        runtime.document.node(a).expect("A 应仍存在").bounds,
        expected[0].bounds,
        "删除 master 后 A 应按普通响应式布局重排"
    );
    assert_eq!(
        runtime.document.node(c).expect("C 应仍存在").bounds,
        expected[1].bounds,
        "删除 master 后 C 应按普通响应式布局重排"
    );
}
