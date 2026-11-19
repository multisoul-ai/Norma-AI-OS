use super::*;

/// 场景：排队计数随入队和开始增减，过期的开始事件不改计数。
/// 数据构造：先入队两次，再依次收到轮次 5、5（重复）、3（更旧）的开始事件。
/// 预期结果：入队后 queued=2；轮次 5 开始后 queued=1；重复的 5 与更旧的 3 不再减少；interrupt 清零。
#[test]
fn queue_counts_follow_queued_and_started_events() {
    let mut activity = TaskActivity::default();
    activity.apply(0, TaskUpdate::Queued);
    activity.apply(0, TaskUpdate::Queued);
    assert_eq!(activity.queued, 2, "两次入队");
    activity.apply(5, TaskUpdate::Started);
    assert_eq!(activity.queued, 1, "轮次 5 开始，队列减一");
    activity.apply(5, TaskUpdate::Started);
    activity.apply(3, TaskUpdate::Started);
    assert_eq!(activity.queued, 1, "重复或更旧的开始事件不改变排队数");
    activity.interrupt();
    assert_eq!(activity.queued, 0, "服务释放后排队清零");
}
