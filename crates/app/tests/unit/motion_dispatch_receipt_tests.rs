use super::*;
use std::time::Duration;

/// DR-M-1：一条已接受的 dispatch receipt 必须经历飞行、命中并在结束后停止帧活动。
///
/// 数据构造（含关键数值的推导过程）：
///   target_node_id = 901
///   previous flight boundary = 480ms = 9 × 50ms + 30ms
///   flight duration = 900ms = 480ms + 8 × 50ms + 20ms（与想法流星一致）
///   previous impact boundary = 260ms = 5 × 50ms + 10ms
///   impact duration = 430ms = 260ms + 3 × 50ms + 20ms
///   active effects = 1 → 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向空 MotionStore 写入一条 Submitted receipt。
///   2. 先推进旧 Flight 边界 480ms，确认仍未提前命中，再推进到 900ms。
///   3. 先推进旧 Impact 边界 260ms，确认仍未提前消失，再推进到 430ms。
///   4. 读取 presentation 与 activity summary，确认 effect 在新边界被回收。
///
/// 预期结果：
///   - 正断言：spawn 后存在唯一 Flight presentation，目标为 901。
///   - 正断言：900ms 后进入 Impact，且 Canvas 仍保持 active。
///   - 正断言：再过 430ms 后 presentation 为空且 Canvas 回到 idle。
///   - 负断言：旧 480ms/260ms 边界不能提前切相或卸载，结束后不能继续请求动画帧。
#[test]
fn test_dispatch_receipt_runs_flight_impact_then_returns_canvas_to_idle() {
    let mut store = MotionStore::new(false);

    let outcome =
        store.spawn_dispatch_receipt("runtime-a:17", 901, DispatchReceiptVisualStatus::Submitted);
    let initial = store.dispatch_receipt_presentations();

    assert_eq!(
        outcome,
        DispatchEffectSpawnOutcome::Spawned,
        "首条 Submitted receipt 应创建可见 dispatch effect"
    );
    assert!(
        store.has_dispatch_receipts(),
        "存在 Flight 时 O(1) render 门禁必须报告 dispatch activity"
    );
    assert_eq!(
        initial.len(),
        1,
        "spawn 后应恰好存在一个 presentation，而不是重复创建"
    );
    assert_eq!(
        initial[0].target_node_id, 901,
        "dispatch presentation 必须保留真实 receipt 的目标 node_id"
    );
    assert_eq!(
        initial[0].phase,
        DispatchEffectPhase::Flight,
        "新建 effect 必须从 Flight 开始"
    );
    assert_eq!(
        initial[0].particle_count, 28,
        "Submitted effect 必须使用固定的 20 主粒子+8 次级火花=28 粒子预算"
    );
    assert_ne!(
        initial[0].phase,
        DispatchEffectPhase::Impact,
        "Flight 首帧不得提前显示命中爆发"
    );

    // previous boundary = 480ms = 9 × 50ms + 30ms；新节奏在此仍须保持 Flight。
    for _ in 0..9 {
        assert!(
            store.tick(Duration::from_millis(50)),
            "Flight 每个 50ms 帧都应推进 presentation"
        );
    }
    assert!(
        store.tick(Duration::from_millis(30)),
        "Flight 应继续推进剩余 30ms 到旧 480ms 边界"
    );
    let former_flight_boundary = store.dispatch_receipt_presentations();
    assert_eq!(
        former_flight_boundary[0].phase,
        DispatchEffectPhase::Flight,
        "Flight 在旧 480ms 边界不得提前进入 Impact"
    );

    // remaining flight = 900ms - 480ms = 420ms = 8 × 50ms + 20ms。
    for _ in 0..8 {
        assert!(
            store.tick(Duration::from_millis(50)),
            "延长后的 Flight 每个 50ms 帧都应继续推进 presentation"
        );
    }
    assert!(
        store.tick(Duration::from_millis(20)),
        "Flight 最后 20ms 必须推进到 900ms 命中边界"
    );
    let impact = store.dispatch_receipt_presentations();
    assert_eq!(
        impact[0].phase,
        DispatchEffectPhase::Impact,
        "Flight 累计 900ms 后必须进入 Impact"
    );
    assert!(
        store.activity_summary().canvas_active,
        "Impact 尚未结束时 Canvas 必须维持动画帧活动"
    );

    // previous boundary = 260ms = 5 × 50ms + 10ms；新节奏在此仍须保持 Impact。
    for _ in 0..5 {
        assert!(
            store.tick(Duration::from_millis(50)),
            "Impact 前五个 50ms 帧都应推进粒子和脉冲"
        );
    }
    assert!(
        store.tick(Duration::from_millis(10)),
        "Impact 应继续推进剩余 10ms 到旧 260ms 边界"
    );
    assert_eq!(
        store.dispatch_receipt_presentations()[0].phase,
        DispatchEffectPhase::Impact,
        "Impact 在旧 260ms 边界不得提前卸载"
    );

    // remaining impact = 430ms - 260ms = 170ms = 3 × 50ms + 20ms。
    for _ in 0..3 {
        assert!(
            store.tick(Duration::from_millis(50)),
            "延长后的 Impact 每个 50ms 帧都应继续推进粒子和脉冲"
        );
    }
    assert!(
        store.tick(Duration::from_millis(20)),
        "Impact 最后 20ms 必须推进到 430ms 回收边界"
    );

    assert!(
        store.dispatch_receipt_presentations().is_empty(),
        "Impact 累计 430ms 后必须回收 presentation"
    );
    assert!(
        !store.activity_summary().canvas_active,
        "最后一个 dispatch effect 结束后不得继续请求连续动画帧"
    );
    assert!(
        !store.has_dispatch_receipts(),
        "effect store 清空后 render 热路径必须能 O(1) 提前返回"
    );
}

/// DR-M-2：Reduce Motion 必须取消跨屏飞行，只保留短促的命中脉冲反馈。
///
/// 数据构造（含关键数值的推导过程）：
///   reduce_motion = true
///   target_node_id = 902
///   reduced pulse duration = 150ms = 3 × 50ms
///   spatial flight frames = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 Reduce Motion MotionStore 中写入一条 Queued receipt。
///   2. 立即读取首帧 presentation。
///   3. 连续推进 3 个 50ms 帧到达 reduced pulse 终点。
///
/// 预期结果：
///   - 正断言：首帧直接处于 Impact，并保留 Queued 状态和目标 902。
///   - 正断言：150ms 后 effect 被清理。
///   - 负断言：Reduce Motion 下任何首帧都不能进入 Flight。
#[test]
fn test_dispatch_receipt_reduce_motion_skips_flight_and_keeps_short_pulse() {
    let mut store = MotionStore::new(true);

    store.spawn_dispatch_receipt("runtime-a:18", 902, DispatchReceiptVisualStatus::Queued);
    let initial = store.dispatch_receipt_presentations();

    assert_eq!(
        initial.len(),
        1,
        "Reduce Motion 下仍应保留一次非空间位移反馈"
    );
    assert_eq!(
        initial[0].phase,
        DispatchEffectPhase::Impact,
        "Reduce Motion 首帧必须直接进入目标脉冲阶段"
    );
    assert_eq!(
        initial[0].status,
        DispatchReceiptVisualStatus::Queued,
        "Reduced presentation 必须保留 Queued 的克制视觉语义"
    );
    assert_eq!(
        initial[0].particle_count, 0,
        "Reduce Motion 必须把空间爆炸粒子预算降为零"
    );
    assert_ne!(
        initial[0].phase,
        DispatchEffectPhase::Flight,
        "Reduce Motion 不得出现从 Dock 跨屏飞向卡片的位移"
    );

    // reduced pulse = 150ms = 3 × 50ms；每帧均不超过 clamp。
    for _ in 0..3 {
        assert!(
            store.tick(Duration::from_millis(50)),
            "Reduced pulse 的三个 50ms 帧都应推进透明度反馈"
        );
    }

    assert!(
        store.dispatch_receipt_presentations().is_empty(),
        "Reduced pulse 累计 150ms 后必须被清理"
    );
    assert!(
        !store.activity_summary().canvas_active,
        "Reduced pulse 结束后不得残留 Canvas 动画活动"
    );
}

/// DR-M-3：突发 receipt 必须受三路并发、八条等待和 task_ref 去重约束保护。
///
/// 数据构造（含关键数值的推导过程）：
///   concurrent cap = 3
///   pending cap = 8
///   launch stagger = 40ms
///   initial burst = 1 active + 8 pending = 9 retained effects
///   after 2 × 40ms = 3 active + 6 pending
///   refill = 2 pending → 3 active + 8 pending = 11 retained effects
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入首条 receipt，并用相同 task_ref/target 再写一次。
///   2. 再连续写入八条唯一 receipt，填满 pending 队列。
///   3. 尝试写入第九条等待 receipt，验证容量拒绝。
///   4. 推进两个 40ms stagger，将两条 pending 提升为 active，再补满 pending。
///
/// 预期结果：
///   - 正断言：相同 task_ref/target 只保留一次并返回 Duplicate。
///   - 正断言：任一时刻 active 不超过 3、pending 不超过 8。
///   - 正断言：每个 40ms 边界至多启动一个新 effect。
///   - 负断言：超出容量的 receipt 不得进入 active 或 pending。
#[test]
fn test_dispatch_receipt_deduplicates_and_bounds_burst_capacity() {
    let mut store = MotionStore::new(false);

    let first =
        store.spawn_dispatch_receipt("runtime-b:1", 911, DispatchReceiptVisualStatus::Submitted);
    let duplicate =
        store.spawn_dispatch_receipt("runtime-b:1", 911, DispatchReceiptVisualStatus::Submitted);
    for index in 2..=9 {
        let outcome = store.spawn_dispatch_receipt(
            format!("runtime-b:{index}"),
            910 + index,
            DispatchReceiptVisualStatus::Queued,
        );
        assert_eq!(
            outcome,
            DispatchEffectSpawnOutcome::Queued,
            "pending 尚有容量时，第 {index} 条唯一 receipt 应排队等待 stagger"
        );
    }
    let full =
        store.spawn_dispatch_receipt("runtime-b:10", 920, DispatchReceiptVisualStatus::Untracked);
    let initial_counts = store.dispatch_receipt_counts();

    assert_eq!(
        first,
        DispatchEffectSpawnOutcome::Spawned,
        "空 store 的首条 receipt 应立即成为 active effect"
    );
    assert_eq!(
        duplicate,
        DispatchEffectSpawnOutcome::Duplicate,
        "同一 task_ref 与 target 的重复 receipt 必须被去重"
    );
    assert_eq!(
        full,
        DispatchEffectSpawnOutcome::DroppedCapacity,
        "pending 已有八条时，第九条等待 receipt 必须按容量策略丢弃"
    );
    assert_eq!(
        initial_counts,
        DispatchEffectCounts {
            active: 1,
            pending: 8,
        },
        "初始 burst 必须保持 1 active + 8 pending，重复与溢出均不计入"
    );

    // launch stagger = 40ms；两次边界分别只提升一条 pending。
    assert!(
        store.tick(Duration::from_millis(40)),
        "第一个 40ms stagger 应启动第二个 active effect"
    );
    assert_eq!(
        store.dispatch_receipt_counts(),
        DispatchEffectCounts {
            active: 2,
            pending: 7,
        },
        "第一个 stagger 后只能增加一个 active，不能同时发射全部 pending"
    );
    assert!(
        store.tick(Duration::from_millis(40)),
        "第二个 40ms stagger 应启动第三个 active effect"
    );
    assert_eq!(
        store.dispatch_receipt_counts(),
        DispatchEffectCounts {
            active: 3,
            pending: 6,
        },
        "active 达到三路并发上限后必须停止从 pending 提升"
    );
    let active_particle_budget = store
        .dispatch_receipt_presentations()
        .iter()
        .map(|presentation| presentation.particle_count)
        .sum::<usize>();
    assert!(
        active_particle_budget <= 3 * 28,
        "三路 active effect 的固定粒子池总量不得超过 3×28=84；实际：{active_particle_budget}"
    );
    assert_ne!(
        active_particle_budget,
        4 * 28,
        "并发限流不得分配第四路 28 粒子预算"
    );

    for index in 10..=11 {
        let outcome = store.spawn_dispatch_receipt(
            format!("runtime-b:{index}"),
            920 + index,
            DispatchReceiptVisualStatus::Untracked,
        );
        assert_eq!(
            outcome,
            DispatchEffectSpawnOutcome::Queued,
            "active 已满但 pending 有空位时，第 {index} 条 receipt 应排队"
        );
    }
    let overflow =
        store.spawn_dispatch_receipt("runtime-b:12", 932, DispatchReceiptVisualStatus::Submitted);
    let overflow_retry =
        store.spawn_dispatch_receipt("runtime-b:12", 932, DispatchReceiptVisualStatus::Submitted);

    assert_eq!(
        store.dispatch_receipt_counts(),
        DispatchEffectCounts {
            active: 3,
            pending: 8,
        },
        "refill 后必须精确停在 3 active + 8 pending 的硬上限"
    );
    assert_eq!(
        overflow,
        DispatchEffectSpawnOutcome::DroppedCapacity,
        "总容量满后不得继续保留第十二条 effect"
    );
    assert_eq!(
        overflow_retry,
        DispatchEffectSpawnOutcome::Duplicate,
        "容量跳过的旧 receipt 也必须进入去重历史，不能在稍后重放"
    );
}

/// DR-M-4：单次超长帧间隔必须被限制为 50ms，避免恢复前台时特效瞬间跳完。
///
/// 数据构造（含关键数值的推导过程）：
///   supplied delta = 1000ms
///   max accepted delta = 50ms
///   flight duration = 900ms
///   expected progress = 50 / 900 ≈ 0.05555556
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建一条 Flight effect。
///   2. 模拟 App 从后台恢复，一次传入 1000ms delta。
///   3. 读取 effect 当前 phase 与 progress。
///
/// 预期结果：
///   - 正断言：progress 精确限制在约 0.05555556。
///   - 负断言：effect 不得跳到 Impact，也不得在一帧内结束。
#[test]
fn test_dispatch_receipt_clamps_long_frame_delta() {
    let mut store = MotionStore::new(false);
    store.spawn_dispatch_receipt("runtime-c:1", 941, DispatchReceiptVisualStatus::Submitted);

    // applied = min(supplied 1000ms, max 50ms) = 50ms；progress = 50 / 900 ≈ 0.05555556。
    assert!(
        store.tick(Duration::from_millis(1_000)),
        "超长 delta 仍应推进一个受限动画帧"
    );
    let presentation = store.dispatch_receipt_presentations()[0];

    assert_eq!(
        presentation.phase,
        DispatchEffectPhase::Flight,
        "1000ms 输入经 clamp 后仍必须停留在 Flight"
    );
    assert!(
        (presentation.progress - 0.055_555_56).abs() < f32::EPSILON,
        "progress 应为 50ms / 900ms ≈ 0.05555556，实际为 {}",
        presentation.progress
    );
    assert_ne!(
        presentation.phase,
        DispatchEffectPhase::Impact,
        "后台恢复的单帧不得让特效直接跳到 Impact"
    );
}

/// DR-M-5：动态目标定位必须读取卡片当前 presentation rect，而不是未来布局终点。
///
/// 数据构造（含关键数值的推导过程）：
///   origin rect = (0, 0, 200, 120)
///   target rect = (100, 80, 200, 120)
///   layout duration = 420ms
///   sampled elapsed = 210ms = 420ms / 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 以 origin rect 初始化 node 951。
///   2. 设置新的 target rect，但在 tick 前读取当前 presentation rect。
///   3. 推进 210ms 后再次读取当前 presentation rect。
///
/// 预期结果：
///   - 正断言：tick 前命中点仍锚定 origin rect。
///   - 正断言：中途 rect 已朝 target 移动。
///   - 负断言：中途 rect 不得提前等于最终 target rect。
#[test]
fn test_dispatch_receipt_target_uses_current_animated_node_rect() {
    let mut store = MotionStore::new(false);
    let origin = CanvasRect::new(0.0, 0.0, 200.0, 120.0);
    let target = CanvasRect::new(100.0, 80.0, 200.0, 120.0);
    store.ensure_node(951, origin);
    store.set_layout_target(951, target);

    assert_eq!(
        store.current_node_rect(951, target),
        origin,
        "layout tick 前特效目标必须使用卡片当前 origin rect"
    );

    // sampled elapsed = layout duration 420ms / 2 = 210ms。
    assert!(
        store.tick(Duration::from_millis(210)),
        "布局动画推进 210ms 时 MotionStore 应报告 presentation 变化"
    );
    let midway = store.current_node_rect(951, target);

    assert!(
        midway.origin.x > origin.origin.x && midway.origin.y > origin.origin.y,
        "中途 presentation rect 必须已从 origin 朝 target 移动；实际：{midway:?}"
    );
    assert_ne!(
        midway, target,
        "210ms 仅为 420ms 布局时长的一半，命中点不得提前跳到最终 target rect"
    );
    assert_ne!(
        midway, origin,
        "布局动画中途命中点不得停留在过期 origin rect"
    );
}

/// DR-M-6：目标 Agent Card 被删除后必须立即取消相关特效并恢复空闲调度。
///
/// 数据构造（含关键数值的推导过程）：
///   live target = node 961（1 个）
///   active effects before retain = 1
///   retained live nodes = 0 → removed effects = 1 - 0 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 node 961 创建一条 Submitted Flight。
///   2. 以空节点集合调用 `retain_nodes`，模拟目标卡片被删除。
///   3. 读取 presentation、诊断计数与动画 activity。
///
/// 预期结果：
///   - 正断言：删除前存在唯一活动特效，删除后取消计数增加一次。
///   - 负断言：删除后不得保留 Flight/Impact，也不得继续请求 Canvas 动画帧。
#[test]
fn test_dispatch_receipt_target_removal_cancels_effect_without_impact() {
    let mut store = MotionStore::new(false);
    store.spawn_dispatch_receipt("runtime-d:1", 961, DispatchReceiptVisualStatus::Submitted);

    assert_eq!(
        store.dispatch_receipt_presentations().len(),
        1,
        "目标删除前必须存在唯一 Flight，才能验证取消路径"
    );

    store.retain_nodes(std::iter::empty::<NodeId>());
    let diagnostics = store.diagnostics();

    assert!(
        store.dispatch_receipt_presentations().is_empty(),
        "目标 node 961 删除后不得残留 Flight 或进入悬空 Impact"
    );
    assert_eq!(
        diagnostics.dispatch_cancelled_effects, 1,
        "删除唯一目标应精确记录一次 dispatch effect 取消"
    );
    assert!(
        !store.activity_summary().canvas_active,
        "目标删除并取消最后一个 effect 后 Canvas 必须恢复 idle"
    );
}
