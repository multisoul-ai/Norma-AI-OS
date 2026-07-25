use agent::{
    AgentAssignment, AgentCoordinationError, AgentDispatchSource, AgentRosterEntry,
    AgentTaskCancellation, AgentTaskCoordinator, AgentTrackingMode,
};
use norma::protocol::AgentProfile;

fn roster_entry(definition_id: i64, name: &str, node_id: u64) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id,
        name: name.to_string(),
        speech_alias: None,
        profile: AgentProfile::Codex,
        enabled: true,
        active_node_id: Some(node_id),
        tracking: AgentTrackingMode::Managed,
        busy: false,
    }
}

fn coordinator() -> AgentTaskCoordinator {
    AgentTaskCoordinator::new_with_runtime_id(
        "runtime-test",
        vec![
            roster_entry(1, "Alan", 101),
            roster_entry(2, "Bella", 102),
            roster_entry(3, "Cora", 103),
        ],
    )
    .expect("合法的三 Agent roster 应建立确定性运行实例协调器")
}

/// 子委派身份校验：来源 Agent 不得把当前任务再次派给自己。
///
/// 数据构造（含关键数值的推导过程）：
///   runtime_instance_id = "runtime-test"
///   顶层任务数 = 1：Norma → Alan，runtime_task_id=1
///   子任务尝试数 = 1：Alan(definition_id=1, node_id=101) → Alan
///   未完成后代预算 = 0 / 32，不构成预算拒绝
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Norma 向 Alan 派发顶层任务 → Alan 当前任务成为父任务。
///   2. 服务端来源上下文绑定 Alan 的 definition_id 与 node_id。
///   3. Alan 尝试向 Alan 派发 → 在写入队列前命中自派发校验。
///
/// 预期结果：
///   - 正断言：返回 SelfDispatchForbidden，且携带 Alan 的 definition_id。
///   - 负断言：协调器任务总数仍为 1，不得创建第二条自派发任务。
#[test]
fn delegation_rejects_self_dispatch_before_queue_mutation() {
    let mut coordinator = coordinator();
    coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "根任务")], false)
        .expect("Norma 顶层任务应成功派给 Alan");

    let error = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Alan", "错误的自派发")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect_err("Alan 向自己派发必须被拒绝");

    assert_eq!(
        error,
        AgentCoordinationError::SelfDispatchForbidden { definition_id: 1 },
        "自派发必须返回稳定的 SelfDispatchForbidden 领域错误"
    );
    assert_eq!(
        coordinator.tasks().len(),
        1,
        "自派发被拒绝后不得向现有队列插入第二条任务"
    );
    assert!(
        coordinator
            .tasks()
            .iter()
            .all(|task| task.task != "错误的自派发"),
        "被拒绝的自派发任务正文不得出现在协调器任务列表"
    );
}

/// 活跃祖先循环：A → B → A 必须拒绝，但已存在的 A → B 关系保持不变。
///
/// 数据构造（含关键数值的推导过程）：
///   顶层任务：Norma → Alan，task_id=1，root=runtime-test:1
///   第一代子任务：Alan → Bella，task_id=2，parent=runtime-test:1
///   循环尝试：Bella → Alan；Alan 位于 task_id=2 的活跃祖先链
///   未完成后代 = 1 < 32，因此拒绝原因只能是祖先循环
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan 根任务，再以 Alan 来源创建 Bella 子任务。
///   2. 服务端从 Bella 当前任务解析 parent/root。
///   3. 沿 parent 链检查来源与目标定义，发现 Alan 已是活跃祖先。
///
/// 预期结果：
///   - 正断言：返回 DelegationCycle，target_definition_id=1。
///   - 正断言：Bella 子任务仍保存 Alan 父任务和同一 root 引用。
///   - 负断言：不存在第三条 Bella → Alan 任务。
#[test]
fn delegation_rejects_active_ancestor_cycle_and_preserves_existing_child() {
    let mut coordinator = coordinator();
    let root = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "根任务")], false)
        .expect("Norma 顶层任务应成功派给 Alan");
    let child = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Bella", "第一代子任务")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect("Alan 应能把子任务派给非祖先 Bella");

    let error = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Alan", "形成 A-B-A 循环")],
            false,
            AgentDispatchSource::agent(2, 102),
        )
        .expect_err("Bella 不得把当前任务派回活跃祖先 Alan");

    assert_eq!(
        error,
        AgentCoordinationError::DelegationCycle {
            target_definition_id: 1,
        },
        "A → B → A 必须返回稳定的 DelegationCycle 领域错误"
    );
    assert_eq!(
        child[0].parent_task_ref.as_deref(),
        Some(root[0].task_ref.as_str()),
        "Bella 子任务必须保存 Alan 根任务作为 parent"
    );
    assert_eq!(
        child[0].root_task_ref, root[0].task_ref,
        "Bella 子任务必须继承 Alan 顶层任务的 root 引用"
    );
    assert_eq!(
        coordinator.tasks().len(),
        2,
        "循环拒绝后只能保留根任务与第一代子任务两条记录"
    );
    assert!(
        coordinator
            .tasks()
            .iter()
            .all(|task| task.task != "形成 A-B-A 循环"),
        "被拒绝的循环任务不得进入任一 Agent 队列"
    );
}

/// 根任务后代预算：第 32 个未完成后代允许，第 33 个必须拒绝。
///
/// 数据构造（含关键数值的推导过程）：
///   根任务 = 1 条：Norma → Alan
///   允许后代 = 32 条：Alan → Bella，task_id=2..33
///   拒绝尝试 = 第 33 个后代：task_id 不应分配
///   root 未完成后代计数 = 32 / 32
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 Alan 根任务。
///   2. 连续创建 32 条以该根任务为 parent 的 Bella 子任务。
///   3. 再提交第 33 条，协调器读取有界 root 计数并拒绝。
///
/// 预期结果：
///   - 正断言：前 32 条全部存在，最后一条 task_id=33。
///   - 正断言：第 33 次返回 DelegationBudgetExceeded(limit=32)。
///   - 负断言：任务总数不得从 33（根 1 + 后代 32）增长到 34。
#[test]
fn delegation_allows_thirty_second_descendant_and_rejects_thirty_third() {
    let mut coordinator = coordinator();
    let root = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "预算根任务")], false)
        .expect("Norma 顶层任务应成功派给 Alan");

    let mut last_child_task_id = 0;
    for descendant_index in 1..=32 {
        let receipt = coordinator
            .dispatch_from(
                vec![AgentAssignment::new(
                    "Bella",
                    format!("后代-{descendant_index}"),
                )],
                false,
                AgentDispatchSource::agent(1, 101),
            )
            .expect("第 1 到第 32 个未完成后代都应被允许");
        last_child_task_id = receipt[0].task_id;
    }

    let error = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Bella", "后代-33")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect_err("第 33 个未完成后代必须被预算拒绝");

    assert_eq!(
        last_child_task_id, 33,
        "根任务占用 task_id=1，因此第 32 个后代应获得 task_id=33"
    );
    assert_eq!(
        error,
        AgentCoordinationError::DelegationBudgetExceeded {
            root_task_ref: root[0].task_ref.clone(),
            limit: 32,
        },
        "第 33 个未完成后代必须返回 limit=32 的稳定预算错误"
    );
    assert_eq!(
        coordinator.tasks().len(),
        33,
        "预算拒绝后应保留根任务 1 条与后代 32 条，共 33 条"
    );
    assert!(
        coordinator
            .tasks()
            .iter()
            .all(|task| task.task != "后代-33"),
        "被预算拒绝的第 33 个后代不得进入任务列表"
    );
}

/// 后代预算释放：一个后代进入终态后，应允许补入一个新后代。
///
/// 数据构造（含关键数值的推导过程）：
///   初始未完成后代 = 32 / 32
///   Bella 当前后代进入 done → 未完成后代 = 31 / 32
///   新增“替补后代”1 条 → 未完成后代恢复为 32 / 32
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建根任务及 32 条 Bella 后代，填满预算。
///   2. 完成 Bella 当前运行任务，协调器推进 FIFO 下一条。
///   3. Alan 再派发一条替补后代。
///
/// 预期结果：
///   - 正断言：替补后代成功获得新 task_id=34。
///   - 正断言：原第一条后代状态为 done。
///   - 负断言：替补派发不得继续返回 DelegationBudgetExceeded。
#[test]
fn terminal_descendant_releases_one_root_budget_slot() {
    let mut coordinator = coordinator();
    coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "预算根任务")], false)
        .expect("Norma 顶层任务应成功派给 Alan");
    for descendant_index in 1..=32 {
        coordinator
            .dispatch_from(
                vec![AgentAssignment::new(
                    "Bella",
                    format!("后代-{descendant_index}"),
                )],
                false,
                AgentDispatchSource::agent(1, 101),
            )
            .expect("填满预算前每个后代都应被允许");
    }

    coordinator.finish_current_with_state(2, "done");
    let replacement = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Bella", "替补后代")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect("一个后代进入终态后必须释放一个预算槽");

    assert_eq!(
        replacement[0].task_id, 34,
        "task_id 单调递增，替补后代应获得 34 而不是复用已完成的 2"
    );
    assert_eq!(
        coordinator
            .tasks()
            .iter()
            .find(|task| task.task_id == 2)
            .map(|task| task.state),
        Some("done"),
        "第一条 Bella 后代进入终态后应保留 done 审计状态"
    );
    assert!(
        coordinator
            .tasks()
            .iter()
            .any(|task| task.task == "替补后代"),
        "释放预算后新后代必须真实进入 Bella 队列"
    );
    assert_ne!(
        replacement[0].task_id, 2,
        "新后代不得复用已完成后代的 task_id"
    );
}

/// 精确取消非级联：取消父任务只影响父任务，子任务继续运行。
///
/// 数据构造（含关键数值的推导过程）：
///   父任务 = Norma → Alan，task_id=1，状态 running
///   子任务 = Alan → Bella，task_id=2，状态 running
///   精确取消目标 = task_ref(runtime-test:1)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建父任务和子任务。
///   2. 按父任务不透明 task_ref 请求取消。
///   3. 协调器只把父任务转为 cancel_requested，并重新查询子任务。
///
/// 预期结果：
///   - 正断言：取消结果为 CancelRequested，父任务状态同步更新。
///   - 正断言：子任务仍为 running，父/根引用保持不变。
///   - 负断言：子任务不得进入 cancelled 或 cancel_requested。
#[test]
fn cancelling_parent_by_task_ref_does_not_cascade_to_child() {
    let mut coordinator = coordinator();
    let parent = coordinator
        .dispatch(vec![AgentAssignment::new("Alan", "父任务")], false)
        .expect("Norma 顶层父任务应派发成功");
    let child = coordinator
        .dispatch_from(
            vec![AgentAssignment::new("Bella", "独立继续的子任务")],
            false,
            AgentDispatchSource::agent(1, 101),
        )
        .expect("Alan 子任务应派发成功");

    let cancellation = coordinator
        .cancel_task_ref(&parent[0].task_ref)
        .expect("当前运行实例的父任务引用应可解析");
    let parent_status = coordinator
        .task_by_ref(&parent[0].task_ref)
        .expect("取消后父任务引用仍应可查询");
    let child_status = coordinator
        .task_by_ref(&child[0].task_ref)
        .expect("取消父任务后子任务引用仍应可查询");

    assert_eq!(
        cancellation,
        AgentTaskCancellation::CancelRequested,
        "运行中父任务的精确取消必须先返回 CancelRequested"
    );
    assert_eq!(
        parent_status.state, "cancel_requested",
        "父任务取消请求后状态必须真实更新为 cancel_requested"
    );
    assert_eq!(
        child_status.state, "running",
        "取消父任务不得级联改变运行中子任务状态"
    );
    assert_eq!(
        child_status.parent_task_ref.as_deref(),
        Some(parent[0].task_ref.as_str()),
        "非级联取消后子任务仍应保留原 parent 引用"
    );
    assert_ne!(
        child_status.state, "cancel_requested",
        "子任务不得继承父任务的取消请求"
    );
    assert_ne!(
        child_status.state, "cancelled",
        "子任务不得因父任务取消而进入 cancelled"
    );
}

/// 可信中断确认：目标会话结束时，运行任务与同卡队列都应保留为 interrupted 终态。
///
/// 数据构造（含关键数值的推导过程）：
///   Alan 当前任务 = 1 条：task_id=1，状态 running。
///   Alan FIFO 等待 = 1 条：task_id=2，状态 queued。
///   会话结束影响数 = running(1) + queued(1) = 2。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向空闲 Alan 连续派发两项任务，形成一个运行槽和一个 FIFO 等待项。
///   2. 模拟 Hook 确认 Agent 会话已结束，调用 finish_all_with_state(interrupted)。
///   3. 按两个不透明 task_ref 分别查询终态和队列占用。
///
/// 预期结果：
///   - 正断言：方法报告原运行槽存在且清理一条等待任务。
///   - 正断言：task_id=1 与 task_id=2 都保留为 interrupted，可继续查询。
///   - 负断言：Alan 不再 busy，队列不得残留，也不得把运行任务伪装为 cancelled。
#[test]
fn confirmed_session_end_retains_running_and_queued_tasks_as_interrupted() {
    let mut coordinator = coordinator();
    let receipts = coordinator
        .dispatch(
            vec![
                AgentAssignment::new("Alan", "运行任务"),
                AgentAssignment::new("Alan", "等待任务"),
            ],
            false,
        )
        .expect("同一 Alan 的两项任务应形成运行槽与 FIFO 等待项");

    let finished = coordinator
        .finish_all_with_state(1, "interrupted")
        .expect("Alan definition_id=1 应存在并可收口全部任务");
    let running = coordinator
        .task_by_ref(&receipts[0].task_ref)
        .expect("被中断的运行任务应保留可查询终态");
    let queued = coordinator
        .task_by_ref(&receipts[1].task_ref)
        .expect("被中断的等待任务应保留可查询终态");

    assert_eq!(
        finished,
        (true, 1),
        "会话结束前应存在一个运行槽和恰好一条 FIFO 等待任务"
    );
    assert_eq!(
        running.state, "interrupted",
        "可信会话结束确认后运行任务必须进入 interrupted"
    );
    assert_eq!(
        queued.state, "interrupted",
        "同一卡片无法继续执行的等待任务也必须进入 interrupted"
    );
    assert!(
        !coordinator.is_busy(1),
        "全部任务收口后 Alan 不得继续标记 busy"
    );
    assert_eq!(
        coordinator.queued_task_count(1),
        0,
        "全部任务收口后 FIFO 队列不得残留任何任务"
    );
    assert_ne!(
        running.state, "cancelled",
        "运行任务的可信中断确认不得伪装成尚未提交任务的 cancelled"
    );
}
