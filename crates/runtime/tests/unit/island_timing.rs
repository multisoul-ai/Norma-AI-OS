use super::{IslandEvent as E, IslandMode as M, IslandState};

// 场景：悬停展开并在离开后收回，截止时间不能提前触发。
// 构造：100ms 时移入；执行：259/260ms tick，再在 300ms 移出并在 719/720ms tick。
// 预期：100+160=260ms 展开，300+420=720ms 收回；每个边界前保持原形态。
#[test]
fn hover_and_leave_obey_exact_deadlines() {
    let mut island = IslandState::default();
    assert_eq!(island.mode(), M::Compact, "默认收起");
    assert_eq!(island.deadline_ms(), None, "空闲时不需定时刷新");
    island.apply(E::PointerEntered, 100);
    assert_eq!(island.deadline_ms(), Some(260), "悬停截止为 160ms 后");
    island.apply(E::Tick, 259);
    assert_eq!(island.mode(), M::Compact, "不能提前展开");
    island.apply(E::Tick, 260);
    assert_eq!(island.mode(), M::Expanded, "到截止时展开");
    island.apply(E::PointerLeft, 300);
    assert_eq!(island.deadline_ms(), Some(720), "离开截止为 420ms 后");
    island.apply(E::Tick, 719);
    assert_eq!(island.mode(), M::Expanded, "不能提前收回");
    island.apply(E::Tick, 720);
    assert_eq!(island.mode(), M::Compact, "截止后收起");
    assert_eq!(island.deadline_ms(), None, "收起后没有无用定时器");
}

// 场景：完成通知停留三秒，不被鼠标进出延长或提前清除。
// 构造：100ms 收到完成；执行：指针进出后在 3099/3100ms tick。
// 预期：100+3000=3100ms 前保持 Activity，到时恢复 Compact，无剩余截止时间。
#[test]
fn completion_returns_after_three_seconds_even_with_pointer_events() {
    let mut island = IslandState::default();
    island.apply(E::Completed("done-1".into()), 100);
    assert_eq!(island.mode(), M::Activity, "完成应短暂展示");
    assert_eq!(island.deadline_ms(), Some(3100), "完成停留 3000ms");
    island.apply(E::PointerEntered, 200);
    island.apply(E::PointerLeft, 300);
    island.apply(E::Tick, 3099);
    assert_eq!(island.mode(), M::Activity, "指针进出不能清除通知期限");
    island.apply(E::Tick, 3100);
    assert_eq!(island.mode(), M::Compact, "完成通知到时收回");
    assert_eq!(island.deadline_ms(), None, "结束后不轮询");
}

// 场景：原生 tracking area 重复发入/出事件，不能不断延后截止时间。
// 构造：100ms 入岛，120ms 重复；执行：260ms 展开，300/320ms 连续离开。
// 预期：首次悬停仍在 260ms 到期，首次离开仍在 720ms 到期；重复事件不产生状态变更。
#[test]
fn repeated_pointer_events_do_not_postpone_deadlines() {
    let mut island = IslandState::default();
    island.apply(E::PointerEntered, 100);
    assert!(!island.apply(E::PointerEntered, 120), "重复移入是 no-op");
    assert_eq!(island.deadline_ms(), Some(260), "不延后悬停截止");
    island.apply(E::Tick, 260);
    island.apply(E::PointerLeft, 300);
    assert!(!island.apply(E::PointerLeft, 320), "重复移出是 no-op");
    assert_eq!(island.deadline_ms(), Some(720), "不延后收回截止");
}

// 场景：指针在展开延时内离开，旧 timer 即使迟到也不可重新展开。
// 构造：10ms 进入，100ms 离开；执行：旧 170ms timer 回调。
// 预期：截止清除，迟到 Tick 不改变 Compact；u64 上界计算不溢出。
#[test]
fn stale_tick_after_cancel_is_harmless() {
    let mut island = IslandState::default();
    island.apply(E::PointerEntered, 10);
    island.apply(E::PointerLeft, 100);
    assert_eq!(island.deadline_ms(), None, "离开取消悬停");
    assert!(!island.apply(E::Tick, 170), "迟到 timer 无副作用");
    assert_eq!(island.mode(), M::Compact, "保持收起");
    island.apply(E::PointerEntered, u64::MAX - 10);
    assert_eq!(island.deadline_ms(), Some(u64::MAX), "截止时间饱和而不溢出");
}
