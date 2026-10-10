use agent::card_instance::{CardInstanceCoordinator, CardInstanceStage, CardInstanceTracking};
#[path = "support/card_instance_assignment.rs"]
mod support;
use support::assignment;

/// 无可靠 Hook 的卡片可能仍忙时必须由用户明确要求继续发送，模型不能自行 force。
/// 数据构造：同一卡片 untracked 首任务已 Submitted；第二项无 force，第三项带明确 force。
/// 执行过程：接收并提交首项，尝试默认第二项，再接收有授权的第三项。
/// 预期结果：默认第二项拒绝且不入队；明确 force 的项可接收；首项不被虚构为完成/中断。
#[test]
fn untracked_busy_card_requires_explicit_force_without_losing_original_task() {
    use agent::card_instance::CardInstanceError;
    let mut coordinator = CardInstanceCoordinator::default();
    let mut first = assignment("first", 11, "card", None);
    first.tracking = CardInstanceTracking::Untracked;
    coordinator.accept(first).expect("首项可发送");
    coordinator.submitted("first").expect("已 Enter");
    let mut second = assignment("second", 11, "card", None);
    second.tracking = CardInstanceTracking::Untracked;
    assert_eq!(
        coordinator.accept(second.clone()),
        Err(CardInstanceError::UntrackedRequiresForce),
        "无跟踪且可能忙不能自作主张派第二项"
    );
    second.force_untracked = true;
    coordinator
        .accept(second)
        .expect("用户明确继续发送后可接收");
    assert_eq!(
        coordinator.receipt("first").expect("原项").stage,
        CardInstanceStage::Submitted,
        "明确发送下一项不能编造首项终态"
    );
    assert_eq!(
        coordinator.pending_assignments().len(),
        1,
        "拒绝项不入队，有授权的同项仅入队一次"
    );
}
