use super::*;
use runtime::{AgentTaskState, NodeRunState, RuntimeEvent};

fn task_event(node_id: u64, to: AgentTaskState, source_event: &str) -> RuntimeEvent {
    RuntimeEvent::AgentTaskStateChanged {
        node_id,
        from: AgentTaskState::Ready,
        to,
        source_event: source_event.to_string(),
    }
}

/// Agent 正常完成：一次任务轮次只产生一条带呼叫名的 Done 播报。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 7，Agent 呼叫名 = "Raya"
///   task round = 1 次 UserPromptSubmit + 2 次重复 Stop
///   terminal announcement budget = 每轮最多 1 条，因此第二次 Stop 应被去重
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 输入 UserPromptSubmit → 打开 node 7 的新任务轮次，不产生播报。
///   2. 输入第一次 Stop/Done → 接受本轮终态并生成 "Raya, Done"。
///   3. 输入第二次 Stop/Done → 本轮终态已被认领，不再生成播报。
///
/// 预期结果：
///   - 正断言：第一次终态生成 node 7 的 "Raya, Done"。
///   - 负断言：任务开始和重复 Stop 都不生成额外播报。
#[test]
fn test_agent_done_announces_once_per_task_round() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);

    announcements.handle_runtime_event(
        &task_event(7, AgentTaskState::Thinking, "UserPromptSubmit"),
        None,
        eligible,
    );
    let before_done = announcements.take_next_ready();

    announcements.handle_runtime_event(
        &task_event(7, AgentTaskState::Done, "Stop"),
        Some("Raya"),
        eligible,
    );
    let first_done = announcements.take_next_ready();

    announcements.handle_runtime_event(
        &task_event(7, AgentTaskState::Done, "Stop"),
        Some("Raya"),
        eligible,
    );
    let duplicate_done = announcements.take_next_ready();

    assert_eq!(
        before_done, None,
        "UserPromptSubmit 只应打开任务轮次，不应提前生成终态播报"
    );
    assert_eq!(
        first_done,
        Some(AgentAnnouncement::new(7, "Raya, Done")),
        "第一次 Stop/Done 应生成带 Agent 呼叫名的唯一正常完成播报"
    );
    assert_eq!(
        duplicate_done, None,
        "同一任务轮次的重复 Stop 不应生成第二条 Done 播报"
    );
    assert_ne!(
        first_done,
        Some(AgentAnnouncement::new(7, "Codex, Done")),
        "存在 Raya 呼叫名时不应错误回退到 Agent 运行时类型名称"
    );
}

/// Agent 失败分类：工具失败保持非终态，整轮 StopFailure 才播报 Failed。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 11，Agent 呼叫名 = "Raya"
///   round 1 = UserPromptSubmit → PostToolUseFailure → Stop
///   round 2 = UserPromptSubmit → StopFailure
///   每轮终态预算 = 1 条，因此 round 1 只能得到 Done，round 2 只能得到 Failed
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一轮工具调用失败 → 保持轮次可继续，不产生 Failed。
///   2. 第一轮随后 Stop/Done → 生成 "Raya, Done"。
///   3. 第二轮收到 StopFailure → 生成 "Raya, Failed"。
///
/// 预期结果：
///   - 负断言：PostToolUseFailure 不产生任何播报。
///   - 正断言：恢复后的第一轮仍产生 Done。
///   - 正断言：第二轮整轮失败产生 Failed，且不错误产生 Done。
#[test]
fn test_agent_terminal_failure_excludes_recoverable_tool_failure() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);

    announcements.handle_runtime_event(
        &task_event(11, AgentTaskState::Thinking, "UserPromptSubmit"),
        None,
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(
            11,
            AgentTaskState::Failed {
                message: "tool failed".to_string(),
            },
            "PostToolUseFailure",
        ),
        Some("Raya"),
        eligible,
    );
    let after_tool_failure = announcements.take_next_ready();

    announcements.handle_runtime_event(
        &task_event(11, AgentTaskState::Done, "Stop"),
        Some("Raya"),
        eligible,
    );
    let recovered_done = announcements.take_next_ready();

    announcements.handle_runtime_event(
        &task_event(11, AgentTaskState::Thinking, "UserPromptSubmit"),
        None,
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(
            11,
            AgentTaskState::Failed {
                message: "session failed".to_string(),
            },
            "StopFailure",
        ),
        Some("Raya"),
        eligible,
    );
    let terminal_failure = announcements.take_next_ready();

    assert_eq!(
        after_tool_failure, None,
        "PostToolUseFailure 可由 Agent 恢复，不应被朗读为整轮 Failed"
    );
    assert_eq!(
        recovered_done,
        Some(AgentAnnouncement::new(11, "Raya, Done")),
        "工具失败后成功收束的任务仍应播报 Done"
    );
    assert_eq!(
        terminal_failure,
        Some(AgentAnnouncement::new(11, "Raya, Failed")),
        "StopFailure 是整轮失败证据，应播报 Failed"
    );
    assert_ne!(
        terminal_failure,
        Some(AgentAnnouncement::new(11, "Raya, Done")),
        "整轮 StopFailure 不应被错误朗读为 Done"
    );
}

/// Agent 进程失败兜底：只为仍活动且未认领终态的任务播报 Failed。
///
/// 数据构造（含关键数值的推导过程）：
///   active node_id = 21：1 次 UserPromptSubmit 后收到进程 Error
///   idle node_id = 22：0 次 UserPromptSubmit，直接收到进程 Error
///   done node_id = 23：UserPromptSubmit → Stop/Done → 进程 Error
///   每轮终态预算 = 1 条，因此 node 23 只能保留 Done，不能再追加 Failed
///
/// 执行过程（逐步说明系统如何处理）：
///   1. node 21 打开任务轮次后进程非零退出 → 使用 Failed 兜底。
///   2. node 22 没有任务轮次直接退出 → 忽略。
///   3. node 23 先正常完成、再收到进程退出 → 已认领终态，忽略兜底。
///
/// 预期结果：
///   - 正断言：node 21 生成 "Raya, Failed"。
///   - 负断言：node 22 不生成空闲失败播报。
///   - 正断言：node 23 只生成 "Mira, Done"。
///   - 负断言：node 23 不再生成 "Mira, Failed"。
#[test]
fn test_agent_process_error_falls_back_only_for_unclaimed_active_task() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);
    let process_error = |node_id| RuntimeEvent::NodeRunStateChanged {
        node_id,
        from: NodeRunState::Running,
        to: NodeRunState::Error {
            message: "process exited with status 1".to_string(),
        },
    };

    announcements.handle_runtime_event(
        &task_event(21, AgentTaskState::Thinking, "UserPromptSubmit"),
        None,
        eligible,
    );
    announcements.handle_runtime_event(&process_error(21), Some("Raya"), eligible);
    let active_failure = announcements.take_next_ready();

    announcements.handle_runtime_event(&process_error(22), Some("Codex"), eligible);
    let idle_failure = announcements.take_next_ready();

    announcements.handle_runtime_event(
        &task_event(23, AgentTaskState::Thinking, "UserPromptSubmit"),
        None,
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(23, AgentTaskState::Done, "Stop"),
        Some("Mira"),
        eligible,
    );
    announcements.handle_runtime_event(&process_error(23), Some("Mira"), eligible);
    let completed = announcements.take_next_ready();
    let duplicate_failure = announcements.take_next_ready();

    assert_eq!(
        active_failure,
        Some(AgentAnnouncement::new(21, "Raya, Failed")),
        "活动任务缺少 provider 失败 hook 时，PTY Error 应提供 Failed 兜底"
    );
    assert_eq!(
        idle_failure, None,
        "没有 UserPromptSubmit 的空闲 Agent 进程退出不应产生任务失败播报"
    );
    assert_eq!(
        completed,
        Some(AgentAnnouncement::new(23, "Mira, Done")),
        "已经正常完成的任务应保留唯一 Done 播报"
    );
    assert_eq!(
        duplicate_failure, None,
        "正常终态认领后到达的 PTY Error 不应追加第二条 Failed 播报"
    );
}

/// Agent 生命周期静默收束：手动中断或 SessionEnd 后的进程错误不得误报任务失败。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 24
///   round 1 = UserPromptSubmit + ManualInterrupt/Ended + process Error
///   round 2 = UserPromptSubmit + process Error
///   Total   = 2 轮任务，其中仅第 2 轮仍有活动终态资格
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一轮进入 Thinking 后收到 ManualInterrupt/Ended → 静默关闭活动轮次。
///   2. 第一轮随后进程 Error → 因轮次已关闭而忽略。
///   3. 第二轮重新 UserPromptSubmit 后直接进程 Error → 作为活动任务失败兜底。
///
/// 预期结果：
///   - 负断言：ManualInterrupt 本身及其后的进程 Error 都不生成播报。
///   - 正断言：新一轮活动任务的进程 Error 仍生成 `Raya, Failed`。
#[test]
fn test_agent_ended_lifecycle_closes_round_before_process_error_fallback() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);
    let process_error = RuntimeEvent::NodeRunStateChanged {
        node_id: 24,
        from: NodeRunState::Running,
        to: NodeRunState::Error {
            message: "process exited with status 1".to_string(),
        },
    };

    announcements.handle_runtime_event(
        &task_event(24, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Raya"),
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(24, AgentTaskState::Ended, "ManualInterrupt"),
        Some("Raya"),
        eligible,
    );
    announcements.handle_runtime_event(&process_error, Some("Raya"), eligible);

    assert_eq!(
        announcements.take_next_ready(),
        None,
        "ManualInterrupt/Ended 必须关闭第一轮，后续进程 Error 不得误报 Failed"
    );

    announcements.handle_runtime_event(
        &task_event(24, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Raya"),
        eligible,
    );
    announcements.handle_runtime_event(&process_error, Some("Raya"), eligible);

    assert_eq!(
        announcements.take_next_ready(),
        Some(AgentAnnouncement::new(24, "Raya, Failed")),
        "新的活动轮次仍应允许非零进程退出生成 Failed 兜底"
    );
    assert_eq!(
        announcements.take_next_ready(),
        None,
        "两轮生命周期中不得残留 ManualInterrupt 对应的额外播报"
    );
}

/// Agent 播报队列上限：第 33 条到达时丢弃最旧待播项并保留最新 32 条。
///
/// 数据构造（含关键数值的推导过程）：
///   pending capacity = 32 条（SPEC 固定上限）
///   terminal events = 33 条，每个 node 各 1 次 UserPromptSubmit + 1 次 Stop
///   overflow = 33 - 32 = 1 条，因此只应丢弃最旧 node 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 按 node 1..=33 的顺序分别打开并正常完成一轮任务。
///   2. 依次取出所有待播项，观察 FIFO 的首项、末项和总数。
///
/// 预期结果：
///   - 正断言：最终恰好保留 32 条。
///   - 负断言：node 1 已因溢出被丢弃。
///   - 正断言：首项是 node 2，证明丢的是最旧项。
///   - 正断言：末项是 node 33，证明最新结果被保留。
#[test]
fn test_agent_announcement_queue_drops_oldest_after_thirty_two_pending() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);

    for node_id in 1..=33 {
        announcements.handle_runtime_event(
            &task_event(node_id, AgentTaskState::Thinking, "UserPromptSubmit"),
            None,
            eligible,
        );
        announcements.handle_runtime_event(
            &task_event(node_id, AgentTaskState::Done, "Stop"),
            Some("Raya"),
            eligible,
        );
    }

    let queued = std::iter::from_fn(|| announcements.take_next_ready()).collect::<Vec<_>>();

    assert_eq!(
        queued.len(),
        32,
        "33 条终态进入容量 32 的队列后必须只保留 32 条待播项"
    );
    assert!(
        !queued.iter().any(|item| item.node_id == 1),
        "溢出时最旧的 node 1 必须被丢弃，不能仍留在待播队列"
    );
    assert_eq!(
        queued.first(),
        Some(&AgentAnnouncement::new(2, "Raya, Done")),
        "溢出后 FIFO 首项必须是次旧的 node 2"
    );
    assert_eq!(
        queued.last(),
        Some(&AgentAnnouncement::new(33, "Raya, Done")),
        "最新到达的 node 33 必须作为 FIFO 末项保留"
    );
}

/// 关闭播报时终态被消费且不会在重新开启后补播。
///
/// 数据构造（含关键数值的推导过程）：
///   round-1 = UserPromptSubmit + Stop，事件时 enabled=false
///   round-2 = UserPromptSubmit + Stop，事件时 enabled=true
///   Total   = 2 个任务轮次、1 条可播回执
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 第一轮终态在关闭状态到达 → 轮次被终态占用但不入队。
///   2. 开启设置但不创建新轮次 → 第一轮不得迟到补播。
///   3. 第二轮正常完成 → 新建一条 `Raya, Done`。
///
/// 预期结果：
///   - 第一轮关闭时队列为空。
///   - 重新开启本身仍无回执。
///   - 第二轮恰好产生一条回执。
#[test]
fn test_disabled_agent_announcement_is_not_replayed_after_reenable() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);
    let disabled = AnnouncementEligibility::new(false, true);
    announcements.handle_runtime_event(
        &task_event(7, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Raya"),
        disabled,
    );
    announcements.handle_runtime_event(
        &task_event(7, AgentTaskState::Done, "Stop"),
        Some("Raya"),
        disabled,
    );

    assert_eq!(
        announcements.take_next_ready(),
        None,
        "关闭播报时到达的第一轮终态不应进入待播队列"
    );
    assert_eq!(
        announcements.take_next_ready(),
        None,
        "重新开启设置本身不应补播第一轮已跳过的终态"
    );

    announcements.handle_runtime_event(
        &task_event(7, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Raya"),
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(7, AgentTaskState::Done, "Stop"),
        Some("Raya"),
        eligible,
    );
    assert_eq!(
        announcements.take_next_ready(),
        Some(AgentAnnouncement::new(7, "Raya, Done")),
        "重新开启后只有新任务轮次应生成 `Raya, Done`"
    );
    assert_eq!(
        announcements.take_next_ready(),
        None,
        "第二轮之外不应残留第一轮迟到补播或重复回执"
    );
}

/// 删除 Agent 卡片时移除其待播回执且不影响其他卡片。
///
/// 数据构造（含关键数值的推导过程）：
///   node-1 = `Raya, Done`
///   node-2 = `Codex, Failed`
///   Total  = 2 条待播回执，删除其中 1 个来源节点后剩 1 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 两个 Agent 分别开启轮次并到达终态 → FIFO 中有两条回执。
///   2. 收到 node-1 的 NodeRemoved → 清除 node-1 轮次和待播项。
///   3. 继续取队列 → 只得到 node-2 的失败回执。
///
/// 预期结果：
///   - 已删除 node-1 的 `Raya, Done` 不存在。
///   - 未删除 node-2 的 `Codex, Failed` 仍存在。
///   - 队列没有额外条目。
#[test]
fn test_removing_agent_card_clears_only_its_pending_announcements() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);
    announcements.handle_runtime_event(
        &task_event(1, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Raya"),
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(1, AgentTaskState::Done, "Stop"),
        Some("Raya"),
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(2, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Codex"),
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(
            2,
            AgentTaskState::Failed {
                message: "provider stopped".to_string(),
            },
            "StopFailure",
        ),
        Some("Codex"),
        eligible,
    );

    announcements.handle_runtime_event(
        &RuntimeEvent::NodeRemoved {
            node_id: 1,
            previous_state: NodeRunState::Running,
        },
        None,
        eligible,
    );
    assert_eq!(
        announcements.take_next_ready(),
        Some(AgentAnnouncement::new(2, "Codex, Failed")),
        "删除 Raya 卡片后必须保留未删除的 `Codex, Failed` 回执"
    );
    assert_eq!(
        announcements.take_next_ready(),
        None,
        "已删除的 Raya 回执不得在未删除 Agent 回执之后继续出现"
    );
}

#[path = "agent_announcements_workspace_tests.rs"]
mod workspace_tests;
