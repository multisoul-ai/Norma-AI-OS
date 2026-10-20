use norma::mobile::ActivityRefreshGate;

/// 场景：同一合并窗口里的连续变化只安排一次重算；快照之后的变化结束后只补一次。
///
/// 数据构造：空门闩。前三次 mark 发生在 begin 之前，表示快照还没取。
///   begin 表示后台任务开始时读取事实。其后的 mark 表示重算进行中又有变化。
/// 执行过程：三次 mark → begin → finish；再 mark → begin → mark → finish → begin → finish。
/// 预期结果：
///   - 正断言：每个窗口只有第一次 mark 安排重算；快照之后的变化让 finish 再安排一次。
///   - 负断言：begin 前的重复 mark，以及没有新变化的 finish，都不安排重算。
#[test]
fn consecutive_activity_marks_schedule_one_recompute() {
    let mut gate = ActivityRefreshGate::default();
    assert!(gate.mark(), "第一次变化必须安排一次重算");
    assert!(!gate.mark(), "快照前的第二次变化必须并入这一次");
    assert!(!gate.mark(), "快照前的第三次变化仍然不能再安排");
    gate.begin();
    assert!(!gate.finish(), "快照已经包含这三次变化，结束后不能再安排");
    assert!(gate.mark(), "下一轮变化可以重新安排");
    gate.begin();
    assert!(!gate.mark(), "快照之后的变化不能并行再开一次");
    assert!(gate.finish(), "进行中到来的变化在结束后补一次");
    gate.begin();
    assert!(!gate.finish(), "补算的快照之后没有新变化");
}
