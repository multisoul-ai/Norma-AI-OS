use super::*;
use crate::master_layout::{
    enter_master_layout_for_area, reapply_master_layout_for_area, toggle_master_layout_for_area,
};
use canvas::{compute_responsive_grid_layout, CanvasNodeKind, RectNode};

fn add_rect(runtime: &mut Runtime, color: u32, bounds: CanvasRect) -> NodeId {
    runtime
        .document
        .add_node(CanvasNodeKind::Rect(RectNode { color }), bounds)
}

/// Master 模式中新增卡片后退出：应对当前全部卡片执行默认响应式 reflow。
///
/// 数据构造（含关键数值的推导过程）：
///   A/B/C/D 初始 bounds 分别为 x=0/120/240/360，宽高均为 100x80
///   area = origin(0,0), size(1000x600)，zoom=1.0
///   第一次 enter master=B 时快照只包含 A/B/C/D
///   新增 E 初始 bounds = origin(0,0), size(100x80)，不在第一次快照中
///   退出默认模式后 ordered_ids = [A,B,C,D,E]，期望使用 compute_responsive_grid_layout
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enter_master_layout_for_area(B) → 保存 A/B/C/D 快照并应用 master/detail
///   2. 新增 E → 模拟 master 模式中通过菜单/工具栏创建卡片
///   3. reapply_master_layout_for_area() → E 进入右侧 detail 自适应流
///   4. toggle_master_layout_for_area(target=None) → 退出 master 模式
///   5. 查询 A/B/C/D/E 当前 bounds
///
/// 预期结果：
///   - 正断言：退出后 state inactive
///   - 正断言：A/B/C/D/E 都按普通响应式布局重排
///   - 负断言：E 不应停留在退出前的 master/detail detail bounds
#[test]
fn test_master_layout_exit_after_adding_node_reflows_all_current_cards() {
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
        "B 存在时应能进入 master 模式"
    );

    let e = add_rect(
        &mut runtime,
        0xffd340,
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    assert!(
        reapply_master_layout_for_area(&mut state, &mut runtime, &mut motion, area, zoom),
        "master 模式中新增 E 后应能重新应用 master/detail 布局"
    );
    let e_detail_bounds = runtime.document.node(e).expect("E 应在新增后存在").bounds;

    let changed =
        toggle_master_layout_for_area(&mut state, &mut runtime, &mut motion, None, area, zoom);
    let expected = compute_responsive_grid_layout(
        &[a, b, c, d, e],
        area,
        workspace_reflow_config_for_zoom(zoom),
    );

    assert!(
        changed,
        "active master 模式收到 target=None 时应退出默认模式"
    );
    assert!(
        !state.is_active(),
        "退出 master 后 state 应变为 inactive，避免后续新增继续套用 master/detail"
    );
    assert_eq!(
        runtime.document.node(a).expect("A 应仍存在").bounds,
        expected[0].bounds,
        "退出后 A 应进入默认响应式布局的第 1 个位置"
    );
    assert_eq!(
        runtime.document.node(b).expect("B 应仍存在").bounds,
        expected[1].bounds,
        "退出后 B 应进入默认响应式布局的第 2 个位置，而不是恢复旧快照"
    );
    assert_eq!(
        runtime.document.node(c).expect("C 应仍存在").bounds,
        expected[2].bounds,
        "退出后 C 应进入默认响应式布局的第 3 个位置"
    );
    assert_eq!(
        runtime.document.node(d).expect("D 应仍存在").bounds,
        expected[3].bounds,
        "退出后 D 应进入默认响应式布局的第 4 个位置"
    );
    assert_eq!(
        runtime.document.node(e).expect("E 应仍存在").bounds,
        expected[4].bounds,
        "退出后新增 E 应参与默认响应式布局重排"
    );
    assert_ne!(
        runtime.document.node(e).expect("E 应仍存在").bounds,
        e_detail_bounds,
        "退出后 E 不应停留在 master/detail 右侧 detail 区"
    );
}

/// Master 模式未新增卡片时退出：仍应精确恢复第一次进入前的原始 bounds。
///
/// 数据构造（含关键数值的推导过程）：
///   A/B/C 初始 bounds 分别为 x=0/120/240，宽高均为 100x80
///   area = origin(0,0), size(1000x600)，zoom=1.0
///   第一次 enter master=B 时快照包含全部当前节点 A/B/C
///   退出时 document 当前节点仍是 [A,B,C]，没有快照外新节点
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enter_master_layout_for_area(B) → B 进入左侧 master 区
///   2. toggle_master_layout_for_area(target=None) → 退出 master 模式
///   3. 查询 A/B/C bounds 与 state
///
/// 预期结果：
///   - 正断言：退出后 state inactive
///   - 正断言：A/B/C 精确恢复进入 master 前的原始 bounds
///   - 负断言：B 不应继续停留在 master 区 bounds
#[test]
fn test_master_layout_exit_without_new_nodes_restores_original_snapshot() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut state = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1000.0, 600.0);
    let zoom = 1.0;
    let a_original = CanvasRect::new(0.0, 0.0, 100.0, 80.0);
    let b_original = CanvasRect::new(120.0, 0.0, 100.0, 80.0);
    let c_original = CanvasRect::new(240.0, 0.0, 100.0, 80.0);
    let a = add_rect(&mut runtime, 0x4080ff, a_original);
    let b = add_rect(&mut runtime, 0xff8040, b_original);
    let c = add_rect(&mut runtime, 0x35d07f, c_original);
    assert!(
        enter_master_layout_for_area(&mut state, &mut runtime, &mut motion, b, area, zoom),
        "B 存在时应能进入 master 模式"
    );
    let b_master_bounds = runtime
        .document
        .node(b)
        .expect("B 应在进入 master 后存在")
        .bounds;

    let changed =
        toggle_master_layout_for_area(&mut state, &mut runtime, &mut motion, None, area, zoom);

    assert!(changed, "active master 模式收到 target=None 时应退出");
    assert!(!state.is_active(), "退出 master 后 state 应变为 inactive");
    assert_eq!(
        runtime.document.node(a).expect("A 应仍存在").bounds,
        a_original,
        "没有新增节点时 A 应精确恢复进入 master 前的原始 bounds"
    );
    assert_eq!(
        runtime.document.node(b).expect("B 应仍存在").bounds,
        b_original,
        "没有新增节点时 B 应精确恢复进入 master 前的原始 bounds"
    );
    assert_eq!(
        runtime.document.node(c).expect("C 应仍存在").bounds,
        c_original,
        "没有新增节点时 C 应精确恢复进入 master 前的原始 bounds"
    );
    assert_ne!(
        runtime.document.node(b).expect("B 应仍存在").bounds,
        b_master_bounds,
        "退出后 B 不应继续停留在 master 区 bounds"
    );
}
