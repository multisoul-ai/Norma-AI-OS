//! Exercise the exact zero-GPUI native event lease used by the App browser adapter.
#[path = "../../app/src/app/browser/generation.rs"]
mod generation;
use generation::GenerationOwner;

/// 场景：旧native事件已入队，同一节点换host后不能污染新view。
/// 数据构造：同一node=7、旧/新两个真实generation owner和队列；包含旧location/失败事件与新location。
/// 执行过程：先送旧事件→替换并释放旧owner→送新事件→使用生产consume消费。
/// 预期结果：只有新view的location通过，旧失败/旧URL均被拒绝；不能恒定丢弃全部事件。
#[test]
fn queued_old_view_events_are_rejected_at_consumption_after_host_replacement() {
    let (sender, receiver) = std::sync::mpsc::channel();
    let old = GenerationOwner::new();
    let old_lease = old.lease();
    sender
        .send(old_lease.queue((7, "old-url")))
        .expect("入队旧location");
    sender
        .send(old_lease.queue((7, "old-failure")))
        .expect("入队旧失败");
    let current = GenerationOwner::new();
    drop(old);
    assert!(!old_lease.is_current(), "释放旧view必须撤销租约");
    sender
        .send(current.lease().queue((7, "new-url")))
        .expect("入队新location");
    let delivered = receiver
        .try_iter()
        .filter_map(|event| event.consume())
        .collect::<Vec<_>>();
    assert_eq!(
        delivered,
        vec![(7, "new-url")],
        "消费时必须过滤旧代际且保留新代际"
    );
}

/// 场景：窗口关闭时owner对象可能随后台项目保留，但其排队事件也必须被撤销。
/// 数据构造：仍存活的owner、预先入队的事件和租约。
/// 执行过程：显式invalidate后才consume。
/// 预期结果：旧事件失败关闭，owner仍存活也不能授权迟到动作。
#[test]
fn explicit_window_invalidation_rejects_already_queued_events() {
    let owner = GenerationOwner::new();
    let lease = owner.lease();
    let event = lease.queue("late-action");
    assert!(lease.is_current(), "存活窗口初始租约有效");
    owner.invalidate();
    assert!(!lease.is_current(), "关窗必须撤销存活owner的租约");
    assert_eq!(event.consume(), None, "关窗前入队的动作也必须被拒绝");
}
