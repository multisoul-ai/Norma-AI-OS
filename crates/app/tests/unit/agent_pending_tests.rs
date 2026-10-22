use super::*;
use agent::AgentDispatchState;
use std::time::{Duration, Instant};

/// ART-READY-1：没有 SessionStart 时，pending 任务必须等待完整启动窗口后才进入兜底提交。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 7，pending tasks = 1；
///   startup grace = 30,000ms（真实 Codex 在 MCP 启动超过 15 秒时仍未接受输入）；
///   early probe = 30,000ms - 1ms = 29,999ms；due probe = 30,000ms。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在固定 Instant 标记 node 7 正在启动并排入 1 项任务；
///   2. 在 29,999ms 查询可兜底提交节点；
///   3. 在 30,000ms 再次查询可兜底提交节点。
///
/// 预期结果：
///   - 正断言：30,000ms 时 node 7 恰好可提交；
///   - 负断言：29,999ms 时结果为空，且不包含无关 node 8。
#[test]
fn pending_submission_fallback_waits_for_full_startup_grace() {
    let started_at = Instant::now();
    let mut pending = PendingAgentSubmissions::default();
    pending.mark_starting_at(7, started_at);
    pending.enqueue(AgentDispatchReceipt {
        task_id: 1,
        task_ref: "test-runtime:1".to_string(),
        root_task_ref: "test-runtime:1".to_string(),
        parent_task_ref: None,
        source_definition_id: None,
        source_node_id: None,
        definition_id: 3,
        name: "Alan".to_string(),
        node_id: 7,
        task: "检查项目".to_string(),
        state: AgentDispatchState::SubmittedManaged,
    });

    let early = pending.due_node_ids(started_at + Duration::from_millis(29_999));
    let due = pending.due_node_ids(started_at + Duration::from_millis(30_000));

    assert!(
        early.is_empty(),
        "启动 29,999ms 时不得提前提交，实际 due nodes：{early:?}"
    );
    assert_eq!(due, vec![7], "启动满 30,000ms 后应只有 node 7 进入兜底提交");
    assert!(
        !due.contains(&8),
        "兜底查询不得产生未登记的 node 8，实际 due nodes：{due:?}"
    );
}

/// ART-READY-2：关闭尚未就绪的 Agent 时，必须同时移除其启动标记和待发送任务。
///
/// 数据构造（含关键数值的推导过程）：
///   node 7 = 即将关闭的 Agent，pending tasks = 1；
///   node 8 = 仍存活的 Agent，pending tasks = 1；
///   due probe = startup grace 30,000ms，两个节点原本都应进入兜底提交。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在固定 Instant 将 node 7、node 8 标记为启动中并分别排入任务；
///   2. 模拟关闭 node 7，清理其 pending 状态；
///   3. 在 30,000ms 查询仍可兜底提交的节点。
///
/// 预期结果：
///   - 正断言：node 8 的任务仍保留并可提交；
///   - 负断言：node 7 不再等待，也不会在关闭后被兜底提交。
#[test]
fn remove_node_discards_only_closed_agent_submission() {
    let started_at = Instant::now();
    let mut pending = PendingAgentSubmissions::default();
    pending.mark_starting_at(7, started_at);
    pending.mark_starting_at(8, started_at);
    for (task_id, definition_id, node_id, name) in [(1, 3, 7, "Alan"), (2, 4, 8, "Codex")] {
        pending.enqueue(AgentDispatchReceipt {
            task_id,
            task_ref: format!("test-runtime:{task_id}"),
            root_task_ref: format!("test-runtime:{task_id}"),
            parent_task_ref: None,
            source_definition_id: None,
            source_node_id: None,
            definition_id,
            name: name.to_string(),
            node_id,
            task: "检查项目".to_string(),
            state: AgentDispatchState::SubmittedManaged,
        });
    }

    pending.remove_node(7);
    let due = pending.due_node_ids(started_at + Duration::from_millis(30_000));

    assert!(
        !pending.is_waiting(7),
        "关闭 node 7 后不得继续保留其启动等待标记"
    );
    assert!(
        !due.contains(&7),
        "关闭 node 7 后不得继续兜底提交其任务，实际 due nodes：{due:?}"
    );
    assert!(
        pending.is_waiting(8),
        "清理 node 7 不得误删仍存活 node 8 的启动等待标记"
    );
    assert_eq!(due, vec![8], "清理 node 7 后应只保留 node 8 的待发送任务");
}

/// ART-READY-3：启动宽限已过且没有待发送任务时，后续派发应把 Agent 视为已就绪。
///
/// 数据构造（含关键数值的推导过程）：
///   startup grace = 30,000ms；early probe = 30,000ms - 1ms = 29,999ms；
///   expiry probe = 30,000ms；pending tasks = 0。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在固定 Instant 标记 node 7 正在启动，但不排入任务；
///   2. 在 29,999ms 查询是否仍需等待；
///   3. 在 30,000ms 再查询，并检查过期标记是否被清除。
///
/// 预期结果：
///   - 正断言：29,999ms 时仍需等待；
///   - 负断言：30,000ms 后不再等待，也不残留启动标记。
#[test]
fn expired_startup_marker_does_not_delay_later_dispatch() {
    let started_at = Instant::now();
    let mut pending = PendingAgentSubmissions::default();
    pending.mark_starting_at(7, started_at);

    let waits_early = pending.should_wait_at(7, started_at + Duration::from_millis(29_999));
    let waits_after_expiry = pending.should_wait_at(7, started_at + Duration::from_millis(30_000));

    assert!(waits_early, "启动 29,999ms 时 node 7 仍应等待 CLI 就绪");
    assert!(
        !waits_after_expiry,
        "启动满 30,000ms 且无 pending task 时，node 7 应立即视为可提交"
    );
    assert!(
        !pending.is_waiting(7),
        "启动标记过期后必须清除，避免后续派发错误返回 Queued"
    );
    assert!(
        !pending.is_waiting(8),
        "过期检查不得产生未登记的 node 8 启动标记"
    );
}

/// ART-READY-4：可安全重试的提交失败必须等待完整 500ms 后重新进入可提交集合。
///
/// 数据构造（含关键数值的推导过程）：
///   retry delay = 500ms；early probe = 500ms - 1ms = 499ms；
///   due probe = 500ms；pending tasks = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在固定失败时刻重新排入 node 7 的任务；
///   2. 在 499ms 查询可重试节点；
///   3. 在 500ms 再次查询可重试节点。
///
/// 预期结果：
///   - 正断言：500ms 时 node 7 恰好可重试；
///   - 负断言：499ms 时不得提前重试，也不得产生 node 8。
#[test]
fn retry_safe_failure_waits_for_full_retry_delay() {
    let failed_at = Instant::now();
    let mut pending = PendingAgentSubmissions::default();
    pending.requeue_at(
        AgentDispatchReceipt {
            task_id: 1,
            task_ref: "test-runtime:1".to_string(),
            root_task_ref: "test-runtime:1".to_string(),
            parent_task_ref: None,
            source_definition_id: None,
            source_node_id: None,
            definition_id: 3,
            name: "Alan".to_string(),
            node_id: 7,
            task: "检查项目".to_string(),
            state: AgentDispatchState::SubmittedManaged,
        },
        failed_at,
    );

    let early = pending.due_node_ids(failed_at + Duration::from_millis(499));
    let due = pending.due_node_ids(failed_at + Duration::from_millis(500));

    assert!(
        early.is_empty(),
        "提交失败 499ms 时不得提前重试，实际 due nodes：{early:?}"
    );
    assert_eq!(due, vec![7], "提交失败满 500ms 后应只有 node 7 可安全重试");
    assert!(
        !due.contains(&8),
        "重试查询不得产生未登记的 node 8，实际 due nodes：{due:?}"
    );
}

/// ART-READY-5：正文写入后必须等待完整 100ms，才能发送 Enter。
///
/// 数据构造（含关键数值的推导过程）：
///   task bytes = 12（“检查项目”的 UTF-8 为 4 × 3 bytes）；
///   enter delay = 100ms；early probe = 100ms - 1ms = 99ms；
///   due probe = 100ms；pending stage = SubmitEnter。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在固定正文写入时刻把 node 7 切换到 SubmitEnter 阶段；
///   2. 在 99ms 查询可发送 Enter 的节点；
///   3. 在 100ms 再次查询。
///
/// 预期结果：
///   - 正断言：100ms 时只有 node 7 可发送 Enter；
///   - 负断言：99ms 时不得提前发送，也不得产生 node 8。
#[test]
fn enter_submission_waits_for_paste_batch_to_settle() {
    let typed_at = Instant::now();
    let mut pending = PendingAgentSubmissions::default();
    pending.schedule_enter_at(
        AgentDispatchReceipt {
            task_id: 1,
            task_ref: "test-runtime:1".to_string(),
            root_task_ref: "test-runtime:1".to_string(),
            parent_task_ref: None,
            source_definition_id: None,
            source_node_id: None,
            definition_id: 3,
            name: "Alan".to_string(),
            node_id: 7,
            task: "检查项目".to_string(),
            state: AgentDispatchState::SubmittedManaged,
        },
        typed_at,
    );

    let early = pending.due_node_ids(typed_at + Duration::from_millis(99));
    let due = pending.due_node_ids(typed_at + Duration::from_millis(100));

    assert!(
        early.is_empty(),
        "正文写入 99ms 时不得提前发送 Enter，实际 due nodes：{early:?}"
    );
    assert_eq!(
        due,
        vec![7],
        "正文写入满 100ms 后应只有 node 7 可发送 Enter"
    );
    assert!(
        !due.contains(&8),
        "Enter 查询不得产生未登记的 node 8，实际 due nodes：{due:?}"
    );
}

/// ART-READY-6：取消尚未发送 Enter 的任务必须按 task id 移除，且不能误删相邻节点任务。
///
/// 数据构造（含关键数值的推导过程）：
///   pending tasks = 2（task_id=7 已写正文，task_id=8 尚未写正文）
///   removed tasks = 1（只取消 task_id=7）
///   due probe = 30,000ms（node 8 的启动兜底到期）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将 task 7 放入 SubmitEnter 阶段，将 task 8 放入 TypeTask 阶段
///   2. 按 task_id=7 移除待提交项
///   3. 查询 30,000ms 时仍可提交的节点
///
/// 预期结果：
///   - 正断言：返回 task 7 且标记正文已写入，node 8 仍可提交
///   - 负断言：node 7 不再到期提交，task 8 没有被误删
#[test]
fn remove_task_prevents_cancelled_pending_submission_from_later_enter() {
    let started_at = Instant::now();
    let mut pending = PendingAgentSubmissions::default();
    pending.schedule_enter_at(
        AgentDispatchReceipt {
            task_id: 7,
            task_ref: "test-runtime:7".to_string(),
            root_task_ref: "test-runtime:7".to_string(),
            parent_task_ref: None,
            source_definition_id: None,
            source_node_id: None,
            definition_id: 3,
            name: "Alan".to_string(),
            node_id: 17,
            task: "检查项目".to_string(),
            state: AgentDispatchState::SubmittedManaged,
        },
        started_at,
    );
    pending.mark_starting_at(18, started_at);
    pending.enqueue(AgentDispatchReceipt {
        task_id: 8,
        task_ref: "test-runtime:8".to_string(),
        root_task_ref: "test-runtime:8".to_string(),
        parent_task_ref: None,
        source_definition_id: None,
        source_node_id: None,
        definition_id: 4,
        name: "Codex".to_string(),
        node_id: 18,
        task: "运行测试".to_string(),
        state: AgentDispatchState::SubmittedManaged,
    });

    let stage = pending.task_stage(7).expect("task 7 应存在于待提交集合");
    let removed = pending.remove_task(7).expect("task 7 应存在于待提交集合");
    let due = pending.due_node_ids(started_at + Duration::from_millis(30_000));

    assert_eq!(removed.receipt.task_id, 7, "移除结果必须精确返回 task_id=7");
    assert!(
        stage.1,
        "SubmitEnter 阶段表示 task 7 正文已写入，应要求清理输入区"
    );
    assert!(!due.contains(&17), "被取消的 node 17 不得在之后发送 Enter");
    assert_eq!(due, vec![18], "取消 task 7 后应只保留 node 18 的 task 8");
    assert!(
        pending.remove_task(8).is_some(),
        "未被误删的 task 8 应仍可按 id 找到"
    );
    assert!(
        pending.remove_task(7).is_none(),
        "task 7 已移除，不得残留第二份待提交记录"
    );
}

/// 想法流星的暂缓：到点之前不可提交，到点后恰好可提交；更晚的暂缓不会被更早的覆盖。
///
/// 数据构造：node 7 在固定时刻 t0 暂缓 860ms 后再被要求暂缓 300ms；node 7 另排入 1 项任务。
/// 执行过程：在 t0+300ms、t0+859ms、t0+860ms 查询到点节点。
/// 预期结果：
///   - 正断言：t0+860ms 时 node 7 到点。
///   - 负断言：t0+300ms（较短的暂缓不能缩短已有的）和 t0+859ms 时没有到点节点。
#[test]
fn hold_until_keeps_the_later_deadline() {
    let t0 = Instant::now();
    let mut pending = PendingAgentSubmissions::default();
    pending.hold_until(7, t0 + Duration::from_millis(860));
    pending.hold_until(7, t0 + Duration::from_millis(300));
    pending.enqueue(AgentDispatchReceipt {
        task_id: 1,
        task_ref: "test-runtime:1".to_string(),
        root_task_ref: "test-runtime:1".to_string(),
        parent_task_ref: None,
        source_definition_id: None,
        source_node_id: None,
        definition_id: 1,
        name: "Alan".to_string(),
        node_id: 7,
        task: "任务".to_string(),
        state: AgentDispatchState::SubmittedUntracked,
    });
    assert!(pending
        .due_node_ids(t0 + Duration::from_millis(300))
        .is_empty());
    assert!(pending
        .due_node_ids(t0 + Duration::from_millis(859))
        .is_empty());
    assert_eq!(
        pending.due_node_ids(t0 + Duration::from_millis(860)),
        vec![7]
    );
}

/// 派发失败撤销暂缓：不留下永远不会被消费的等待记录；但已有待输入任务或被别人改过的等待时间不能被误清。
///
/// 数据构造：node 7 暂缓后撤销；node 8 暂缓后排入任务再撤销；node 9 暂缓后被更晚的时间覆盖再用旧时间撤销。
/// 预期结果：
///   - 正断言：node 7 撤销后没有待处理记录。
///   - 负断言：node 8（已有任务）与 node 9（等待时间已变）撤销后仍保留记录。
#[test]
fn release_hold_only_clears_its_own_idle_hold() {
    let t0 = Instant::now();
    let early = t0 + Duration::from_millis(860);
    let late = t0 + Duration::from_millis(5_000);
    let mut pending = PendingAgentSubmissions::default();
    pending.hold_until(7, early);
    pending.release_hold(7, early);
    assert!(!pending.has_pending_for_node(7), "空闲暂缓应被撤销");

    pending.hold_until(8, early);
    pending.enqueue(AgentDispatchReceipt {
        task_id: 2,
        task_ref: "test-runtime:2".to_string(),
        root_task_ref: "test-runtime:2".to_string(),
        parent_task_ref: None,
        source_definition_id: None,
        source_node_id: None,
        definition_id: 1,
        name: "Alan".to_string(),
        node_id: 8,
        task: "任务".to_string(),
        state: AgentDispatchState::SubmittedUntracked,
    });
    pending.release_hold(8, early);
    assert!(pending.has_pending_for_node(8), "已有待输入任务时不能清除");

    pending.hold_until(9, early);
    pending.hold_until(9, late);
    pending.release_hold(9, early);
    assert!(pending.has_pending_for_node(9), "等待时间被改过时不能清除");
}
