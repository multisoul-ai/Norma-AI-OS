use super::*;
use crate::canvas::master_layout::{enter_master_layout_for_area, MasterLayoutState};
use canvas::{CanvasNodeKind, RectNode};
use storage::{LayoutMode, LayoutOriginalBounds, LayoutStateSnapshot};

fn add_rect(runtime: &mut Runtime, color: u32, bounds: CanvasRect) -> NodeId {
    runtime
        .document
        .add_node(CanvasNodeKind::Rect(RectNode { color }), bounds)
}

/// Master 布局状态快照：active master/detail 应保留 master_id 与进入前原始 bounds。
///
/// 数据构造（含关键数值的推导过程）：
///   A/B 初始 bounds 分别为 (0,0,100,80)/(120,0,100,80)
///   master = B，因此 snapshot.master_id 应为 B
///   original_bounds 数量 = 2，对应 A/B 两个进入 master 前的 bounds
///
/// 执行过程（逐步说明系统如何处理）：
///   1. enter_master_layout_for_area(B) → state active，并保存 A/B 原始 bounds
///   2. state.to_snapshot() → 导出 LayoutStateSnapshot
///   3. fresh_state.restore_from_snapshot(snapshot, runtime) → 恢复 active 状态
///
/// 预期结果：
///   - 正断言：snapshot.mode 为 MasterDetail，master_id 为 B
///   - 正断言：fresh_state 恢复后 active master 仍是 B
///   - 负断言：snapshot 不应退化为 Tiled
#[test]
fn test_master_layout_snapshot_round_trips_active_state() {
    let mut runtime = Runtime::new();
    let mut motion = MotionStore::new(true);
    let mut state = MasterLayoutState::default();
    let area = CanvasRect::new(0.0, 0.0, 1000.0, 600.0);
    let zoom = 1.0;
    let a_original = CanvasRect::new(0.0, 0.0, 100.0, 80.0);
    let b_original = CanvasRect::new(120.0, 0.0, 100.0, 80.0);
    let a = add_rect(&mut runtime, 0x4080ff, a_original);
    let b = add_rect(&mut runtime, 0xff8040, b_original);
    assert!(
        enter_master_layout_for_area(&mut state, &mut runtime, &mut motion, b, area, zoom),
        "B 存在时应能进入 master/detail"
    );

    let snapshot = state.to_snapshot();
    let mut fresh_state = MasterLayoutState::default();
    let restored = fresh_state.restore_from_snapshot(snapshot.clone(), &runtime);

    assert_eq!(
        snapshot.mode,
        LayoutMode::MasterDetail,
        "active master 布局导出时 mode 应为 MasterDetail"
    );
    assert_eq!(
        snapshot.master_id,
        Some(b),
        "active master 布局导出时 master_id 应为 B"
    );
    assert!(
        snapshot
            .original_bounds
            .iter()
            .any(|entry| entry.node_id == a && entry.bounds == a_original),
        "snapshot 应保留 A 进入 master/detail 前的原始 bounds"
    );
    assert!(
        restored,
        "master_id 仍存在时 restore_from_snapshot 应恢复 active 状态"
    );
    assert_eq!(
        fresh_state.master_id(),
        Some(b),
        "恢复后的 master_id 应仍为 B"
    );
    assert_ne!(
        snapshot.mode,
        LayoutMode::Tiled,
        "active master 布局不应被导出为 Tiled"
    );
}

/// Master 布局状态恢复：snapshot 指向不存在的 master_id 时应降级为 tiled。
///
/// 数据构造（含关键数值的推导过程）：
///   runtime 只包含 A(id=1)
///   snapshot.master_id = 999，不存在于 document
///   original_bounds 也包含 node_id=999，不能恢复为 active
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造无效 MasterDetail snapshot
///   2. restore_from_snapshot(snapshot, runtime)
///   3. 查询 state.is_active/master_id
///
/// 预期结果：
///   - 正断言：restore 返回 false，state inactive
///   - 负断言：master_id 不应保留 999
#[test]
fn test_master_layout_restore_invalid_master_falls_back_to_tiled() {
    let mut runtime = Runtime::new();
    let a = add_rect(
        &mut runtime,
        0x4080ff,
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let mut state = MasterLayoutState::default();
    let snapshot = LayoutStateSnapshot {
        mode: LayoutMode::MasterDetail,
        master_id: Some(999),
        original_bounds: vec![LayoutOriginalBounds {
            node_id: 999,
            bounds: CanvasRect::new(10.0, 20.0, 300.0, 200.0),
        }],
    };

    let restored = state.restore_from_snapshot(snapshot, &runtime);

    assert!(
        !restored,
        "master_id 不存在时不应恢复 active master/detail 状态"
    );
    assert!(
        !state.is_active(),
        "无效 master_id 应降级为 inactive/tiled 状态"
    );
    assert_eq!(
        runtime.document.node(a).expect("A 应仍存在").id,
        a,
        "恢复无效 layout state 不应删除已有节点 A"
    );
    assert_ne!(
        state.master_id(),
        Some(999),
        "无效 master_id=999 不应残留在 MasterLayoutState 中"
    );
}
