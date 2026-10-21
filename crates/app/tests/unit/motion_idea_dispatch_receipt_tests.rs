use super::*;

/// 想法行作为流星起点：presentation 保留窗口坐标，不退化为 Dock 或 Agent 来源。
///
/// 数据构造：起点 (120, 340)（窗口像素）；目标 Agent node 972；receipt 为 idea:i1。
/// 执行过程：spawn 一次，再用同一 receipt 与目标重复 spawn；用不同想法再 spawn 一次。
/// 预期结果：
///   - 正断言：首次 Spawned 且 origin 精确为 IdeaRow{120,340}、起始相位为 Flight、未开减少动态效果。
///   - 负断言：同一想法同一目标重复为 Duplicate，不出现第二条 effect；起点不等于 CommandDock。
#[test]
fn test_idea_row_origin_is_preserved_and_deduplicated() {
    let mut store = MotionStore::new(false);
    let origin = DispatchVisualOrigin::IdeaRow { x: 120, y: 340 };
    let first = store.spawn_dispatch_receipt_from(
        "idea:i1",
        origin,
        972,
        DispatchReceiptVisualStatus::Submitted,
    );
    assert_eq!(
        first,
        DispatchEffectSpawnOutcome::Spawned,
        "首条想法流星应立即成为 active effect"
    );
    let shown = store.dispatch_receipt_presentations();
    assert_eq!(shown.len(), 1, "应恰好有一条 effect");
    assert_eq!(shown[0].origin, origin, "起点必须精确保留想法行的窗口坐标");
    assert_ne!(
        shown[0].origin,
        DispatchVisualOrigin::CommandDock,
        "不得退化为 Dock 来源"
    );
    assert_eq!(
        shown[0].phase,
        DispatchEffectPhase::Flight,
        "新 effect 应从飞行相位开始"
    );
    assert!(!shown[0].reduce_motion, "默认不应是减少动态效果");
    let again = store.spawn_dispatch_receipt_from(
        "idea:i1",
        origin,
        972,
        DispatchReceiptVisualStatus::Submitted,
    );
    assert_eq!(
        again,
        DispatchEffectSpawnOutcome::Duplicate,
        "同一想法同一目标不应重复播放"
    );
    assert_eq!(
        store.dispatch_receipt_presentations().len(),
        1,
        "重复 spawn 不应产生第二条 effect"
    );
}

/// 没有来源节点的流星：来源不存在不构成取消理由，目标删除才取消。
///
/// 数据构造：起点 IdeaRow，目标 972。
/// 执行过程：retain_nodes([972]) 模拟其他卡片变化；再 retain_nodes([]) 模拟目标被删。
/// 预期结果：
///   - 正断言：目标仍在时 effect 保留；目标被删后 effect 消失且取消计数为 1。
///   - 负断言：第一次 retain 不得误取消（IdeaRow 没有来源节点可“被删除”）。
#[test]
fn test_idea_row_effect_survives_unrelated_changes_and_cancels_with_target() {
    let mut store = MotionStore::new(false);
    store.spawn_dispatch_receipt_from(
        "idea:i2",
        DispatchVisualOrigin::IdeaRow { x: 10, y: 20 },
        972,
        DispatchReceiptVisualStatus::Queued,
    );
    store.retain_nodes([972]);
    assert_eq!(
        store.dispatch_receipt_presentations().len(),
        1,
        "目标仍在时不得取消想法流星"
    );
    assert_eq!(
        store.diagnostics().dispatch_cancelled_effects,
        0,
        "无关变化不应记取消"
    );
    store.retain_nodes([]);
    assert!(
        store.dispatch_receipt_presentations().is_empty(),
        "目标被删后不应保留悬空 effect"
    );
    assert_eq!(
        store.diagnostics().dispatch_cancelled_effects,
        1,
        "目标被删应精确记一次取消"
    );
}

/// 减少动态效果：想法流星走既有的无位移脉冲路径。
///
/// 数据构造：MotionStore::new(true)。
/// 预期结果：
///   - 正断言：presentation 标记 reduce_motion，起点仍是 IdeaRow（供落点定位）。
///   - 负断言：不会因为起点类型而绕过减少动态效果。
#[test]
fn test_idea_row_effect_honors_reduced_motion() {
    let mut store = MotionStore::new(true);
    let origin = DispatchVisualOrigin::IdeaRow { x: 5, y: 6 };
    store.spawn_dispatch_receipt_from(
        "idea:i3",
        origin,
        972,
        DispatchReceiptVisualStatus::Submitted,
    );
    let shown = store.dispatch_receipt_presentations();
    assert_eq!(shown.len(), 1, "减少动态效果下仍应有落点反馈");
    assert!(shown[0].reduce_motion, "必须沿用减少动态效果路径");
    assert_eq!(shown[0].origin, origin, "起点仍应保留");
}
