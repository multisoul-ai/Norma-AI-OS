use agent::{
    AgentAssignment, AgentDispatchState, AgentRosterEntry, AgentTaskCoordinator, AgentTrackingMode,
};
use norma::protocol::AgentProfile;

fn roster_entry(
    definition_id: i64,
    name: &str,
    profile: AgentProfile,
    busy: bool,
) -> AgentRosterEntry {
    AgentRosterEntry {
        definition_id,
        name: name.to_string(),
        speech_alias: None,
        profile,
        enabled: true,
        active_node_id: Some(definition_id as u64 + 100),
        tracking: AgentTrackingMode::Managed,
        busy,
    }
}

/// AC-1：批量委派应整体预检，并让忙碌 Agent 排队、空闲 Agent 立即并行提交。
///
/// 数据构造（含关键数值的推导过程）：
///   roster = 2 个 Agent：Alan(id=1, busy=true)、Bella(id=2, busy=false)
///   assignments = 2 项：Alan 修后端、Bella 写测试
///   每个 Agent 同时运行上限 = 1，因此 Alan queue=1，Bella running=1
///   invalid batch = 2 项：Alan 第二项 + Missing 文档；任一无效则新增副作用数 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. dispatch(valid batch) → 先解析全部目标，再分别计算 Alan queued、Bella submitted
///   2. 读取 Alan 等待数与 Bella 忙碌状态
///   3. dispatch(invalid batch) → Missing 解析失败，整批拒绝
///   4. 再次读取 Alan 等待数，确认无部分入队
///
/// 预期结果：
///   - 正断言：Alan 回执为 Queued，Bella 回执为 SubmittedManaged
///   - 正断言：Alan 只有 1 项等待，Bella 已进入忙碌
///   - 负断言：无效批次不能把 Alan 的等待数增加到 2
#[test]
fn coordinator_atomically_dispatches_across_managed_agent_queues() {
    let mut coordinator = AgentTaskCoordinator::new(vec![
        roster_entry(1, "Alan", AgentProfile::Codex, true),
        roster_entry(2, "Bella", AgentProfile::Claude, false),
    ])
    .expect("两个名称唯一的合法 Agent 应能建立协调器");

    let receipts = coordinator
        .dispatch(
            vec![
                AgentAssignment::new("Alan", "修后端"),
                AgentAssignment::new("Bella", "写测试"),
            ],
            false,
        )
        .expect("明确分工且所有目标有效时应整体接受");

    assert_eq!(
        receipts[0].state,
        AgentDispatchState::Queued,
        "Alan 已忙碌，其新任务必须进入 FIFO 等待队列"
    );
    assert_eq!(
        receipts[1].state,
        AgentDispatchState::SubmittedManaged,
        "Bella 空闲且 Hook 可跟踪，应立即以 managed 状态提交"
    );
    assert_eq!(
        coordinator.queued_task_count(1),
        1,
        "Alan 当前忙碌后只能新增 1 项等待任务"
    );
    assert!(
        coordinator.is_busy(2),
        "Bella 的任务已提交，协调器必须把 Bella 标记为忙碌"
    );

    let invalid = coordinator.dispatch(
        vec![
            AgentAssignment::new("Alan", "再修接口"),
            AgentAssignment::new("Missing", "写文档"),
        ],
        false,
    );

    assert!(invalid.is_err(), "包含未知 Missing 的批次必须整体拒绝");
    assert_eq!(
        coordinator.queued_task_count(1),
        1,
        "无效批次不得把 Alan 的第二项任务部分写入队列"
    );
    assert_ne!(
        coordinator.queued_task_count(1),
        2,
        "原子预检失败后 Alan 的等待数不能增加到 2"
    );
}

/// AC-2：同一 managed Agent 的等待任务应在终态事件后严格按 FIFO 顺序逐项提交。
///
/// 数据构造（含关键数值的推导过程）：
///   Alan(id=1, busy=true) 已有运行中任务 = 1 项
///   新增等待任务 = 2 项，文本依次为 "第二项"、"第三项"
///   每次 finish_current 最多释放 1 项，因此 queue 数量应从 2 → 1 → 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 连续 dispatch 两项到忙碌 Alan，均进入等待队列
///   2. 第一次 finish_current(1) 取出队首
///   3. 第二次 finish_current(1) 取出下一项，第三次调用把 busy 置为 false
///
/// 预期结果：
///   - 正断言：两次释放文本依次为第二项、第三项，task_id 递增
///   - 正断言：清空后 queue=0 且 busy=false
///   - 负断言：第三次释放不应再次返回已消费任务
#[test]
fn coordinator_releases_managed_queue_in_fifo_order() {
    let mut coordinator =
        AgentTaskCoordinator::new(vec![roster_entry(1, "Alan", AgentProfile::Codex, true)])
            .expect("合法 Alan roster 应能建立协调器");
    let receipts = coordinator
        .dispatch(
            vec![
                AgentAssignment::new("Alan", "第二项"),
                AgentAssignment::new("Alan", "第三项"),
            ],
            false,
        )
        .expect("同一 managed Agent 的两项任务应进入 FIFO 队列");

    let second = coordinator
        .finish_current(1)
        .expect("第一个终态事件应释放第二项");
    let third = coordinator
        .finish_current(1)
        .expect("第二个终态事件应释放第三项");
    let none_left = coordinator.finish_current(1);

    assert_eq!(
        receipts[0].state,
        AgentDispatchState::Queued,
        "忙碌 Alan 收到的第二项必须先排队"
    );
    assert_eq!(
        receipts[1].state,
        AgentDispatchState::Queued,
        "忙碌 Alan 收到的第三项也必须排队"
    );
    assert_eq!(second.task, "第二项", "第一次终态后必须先提交队首第二项");
    assert_eq!(third.task, "第三项", "第二次终态后才提交第三项");
    assert!(
        second.task_id < third.task_id,
        "FIFO 释放顺序必须保持递增 task_id"
    );
    assert_eq!(
        coordinator.queued_task_count(1),
        0,
        "两项均释放后 Alan 等待队列必须为空"
    );
    assert!(
        !coordinator.is_busy(1),
        "第三次 finish_current 确认无等待任务后应把 Alan 标为空闲"
    );
    assert!(none_left.is_none(), "空队列不得再次返回已消费的 Agent 任务");
}

/// AC-3：无 Hook 跟踪且可能仍忙碌的 Agent 必须显式 force 才能继续发送。
///
/// 数据构造（含关键数值的推导过程）：
///   Robin(id=3, tracking=Untracked, busy=true)
///   assignment = 1 项 "检查日志"
///   force=false 时允许提交数 = 0；force=true 时允许提交数 = 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 使用 force=false dispatch，预检检测到无法确认 Robin 是否空闲
///   2. 使用同一 assignment 与 force=true 再次 dispatch
///   3. 检查回执状态与 task 文本
///
/// 预期结果：
///   - 正断言：未 force 返回 UntrackedRequiresForce
///   - 正断言：force 后返回 SubmittedUntracked 且文本不变
///   - 负断言：未 force 的失败调用不应生成回执或等待队列
#[test]
fn coordinator_requires_force_for_busy_untracked_agent() {
    let mut robin = roster_entry(3, "Robin", AgentProfile::OpenCode, true);
    robin.tracking = AgentTrackingMode::Untracked;
    let mut coordinator =
        AgentTaskCoordinator::new(vec![robin]).expect("合法 Robin roster 应能建立协调器");

    let rejected = coordinator.dispatch(vec![AgentAssignment::new("Robin", "检查日志")], false);
    let accepted = coordinator
        .dispatch(vec![AgentAssignment::new("Robin", "检查日志")], true)
        .expect("明确 force 后应允许向 untracked Robin 发送");

    assert!(
        matches!(
            rejected,
            Err(agent::AgentCoordinationError::UntrackedRequiresForce(ref name)) if name == "Robin"
        ),
        "未 force 时必须返回 Robin 的 UntrackedRequiresForce 错误"
    );
    assert_eq!(
        coordinator.queued_task_count(3),
        0,
        "untracked Agent 不得伪造可跟踪等待队列"
    );
    assert_eq!(accepted.len(), 1, "force 后应只生成当前 1 项任务的回执");
    assert_eq!(
        accepted[0].state,
        AgentDispatchState::SubmittedUntracked,
        "force 后回执必须明确标记 SubmittedUntracked"
    );
    assert_eq!(
        accepted[0].task, "检查日志",
        "force 只改变许可状态，不得篡改任务文本"
    );
}

/// AC-4：确认关闭忙碌 Agent 时应同时中断当前占用并取消全部等待任务。
///
/// 数据构造（含关键数值的推导过程）：
///   Alan(id=1, busy=true)，新增 queued tasks=2
///   cancel_all 后 busy=false、queued=0，取消等待数=2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向忙碌 Alan 委派两项，均进入 FIFO
///   2. 调用 cancel_all(1) 模拟用户二次确认关闭卡片
///   3. 读取返回的当前占用、取消数、busy 与 queue
///
/// 预期结果：
///   - 正断言：返回 was_busy=true、cancelled_queued=2
///   - 正断言：取消后 busy=false 且 queue=0
///   - 负断言：等待队列中不得残留任一任务
#[test]
fn coordinator_cancel_all_clears_running_slot_and_waiting_queue() {
    let mut coordinator =
        AgentTaskCoordinator::new(vec![roster_entry(1, "Alan", AgentProfile::Codex, true)])
            .expect("合法 Alan roster 应能建立协调器");
    coordinator
        .dispatch(
            vec![
                AgentAssignment::new("Alan", "第二项"),
                AgentAssignment::new("Alan", "第三项"),
            ],
            false,
        )
        .expect("两项任务应先进入 Alan 等待队列");

    let cancelled = coordinator
        .cancel_all(1)
        .expect("definition_id=1 应存在并可取消");

    assert_eq!(cancelled.0, true, "取消前 Alan 应占用一个运行槽");
    assert_eq!(cancelled.1, 2, "确认关闭应取消两项 FIFO 等待任务");
    assert!(
        !coordinator.is_busy(1),
        "确认关闭后 Alan 不应继续标记为忙碌"
    );
    assert_eq!(
        coordinator.queued_task_count(1),
        0,
        "确认关闭后等待队列不得残留任何任务"
    );
    assert_ne!(
        coordinator.queued_task_count(1),
        2,
        "取消后等待数不得保持原来的 2"
    );
}
