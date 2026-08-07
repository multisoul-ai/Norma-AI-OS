use super::*;

/// DR-A2A-M-1：来源 Agent Card 被删除后，相关飞行必须取消且不得误在目标处爆发。
///
/// 数据构造（含关键数值的推导过程）：
///   source node = 971；target node = 972
///   active effects before retain = 1
///   retained live nodes = {972}（目标仍在、来源已删）
///   removed effects = 1 - 0 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建一条来源 971 指向目标 972 的 Agent-to-Agent Flight。
///   2. 调用 retain_nodes([972]) 模拟来源卡片在飞行中被删除。
///   3. 读取 presentation、取消计数和 Canvas 动画活动。
///
/// 预期结果：
///   - 正断言：删除前 presentation 精确保留 AgentNode(971) 与目标 972。
///   - 正断言：删除来源后取消计数精确增加一次。
///   - 负断言：目标仍存在也不得保留悬空 Flight/Impact，不得继续请求动画帧。
#[test]
fn test_agent_dispatch_receipt_source_removal_cancels_effect_without_target_impact() {
    let mut store = MotionStore::new(false);
    let outcome = store.spawn_dispatch_receipt_from(
        "runtime-a2a:1",
        DispatchVisualOrigin::AgentNode(971),
        972,
        DispatchReceiptVisualStatus::Submitted,
    );
    let before = store.dispatch_receipt_presentations();

    assert_eq!(
        outcome,
        DispatchEffectSpawnOutcome::Spawned,
        "空 store 的首条 Agent-to-Agent receipt 必须立即成为 active effect"
    );
    assert_eq!(
        before.len(),
        1,
        "删除来源前必须恰好存在一条 Agent-to-Agent Flight"
    );
    assert_eq!(
        before[0].origin,
        DispatchVisualOrigin::AgentNode(971),
        "presentation 必须精确保留来源 Agent node 971"
    );
    assert_eq!(
        before[0].target_node_id, 972,
        "presentation 必须精确保留目标 Agent node 972"
    );
    assert_ne!(
        before[0].origin,
        DispatchVisualOrigin::CommandDock,
        "Agent-to-Agent Flight 不得退化为 Command Dock 来源"
    );

    store.retain_nodes([972]);
    let diagnostics = store.diagnostics();

    assert!(
        store.dispatch_receipt_presentations().is_empty(),
        "来源 node 971 删除后，即使目标 972 仍在也不得保留悬空 effect"
    );
    assert_eq!(
        diagnostics.dispatch_cancelled_effects, 1,
        "删除唯一来源应精确记录一次 dispatch effect 取消"
    );
    assert!(
        !store.activity_summary().canvas_active,
        "取消最后一条来源缺失的 effect 后 Canvas 必须恢复 idle"
    );
}
