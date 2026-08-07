use super::*;
use runtime::{AgentTaskState, NodeRunState, RuntimeEvent};

/// 缺失开始事件的可信终态：Stop/Done 恢复任务轮次并且仍只播报一次。
///
/// 数据构造（含关键数值的推导过程）：
///   node-41 = 0 次 UserPromptSubmit + 2 次重复 Stop/Done
///   node-42 = 0 次 UserPromptSubmit + 1 次进程 Error
///   terminal announcement budget = 每个可信任务轮次最多 1 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. node-41 未收到 UserPromptSubmit，直接收到可信 Stop/Done → 从终态恢复轮次并入队。
///   2. node-41 再次收到重复 Stop/Done → 已认领终态，不得重复入队。
///   3. node-42 没有任务轮次且只收到进程 Error → 仍按空闲进程失败处理，不得恢复任务。
///
/// 预期结果：
///   - 正断言：node-41 产生一次 `Henry, Done`。
///   - 负断言：node-41 的重复 Stop 不产生第二次播报。
///   - 负断言：node-42 的空闲进程 Error 不产生 `Codex, Failed`。
#[test]
fn test_trusted_terminal_recovers_missing_round_without_repeating() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);

    announcements.handle_runtime_event(
        &task_event(41, AgentTaskState::Done, "Stop"),
        Some("Henry"),
        eligible,
    );
    let recovered_done = announcements.take_next_ready();

    announcements.handle_runtime_event(
        &task_event(41, AgentTaskState::Done, "Stop"),
        Some("Henry"),
        eligible,
    );
    let duplicate_done = announcements.take_next_ready();

    announcements.handle_runtime_event(
        &RuntimeEvent::NodeRunStateChanged {
            node_id: 42,
            from: NodeRunState::Running,
            to: NodeRunState::Error {
                message: "process exited with status 1".to_string(),
            },
        },
        Some("Codex"),
        eligible,
    );
    let idle_process_failure = announcements.take_next_ready();

    assert_eq!(
        recovered_done,
        Some(AgentAnnouncement::new(41, "Henry, Done")),
        "可信 Stop/Done 即使缺少 UserPromptSubmit 也必须恢复轮次并播报 `Henry, Done`"
    );
    assert_eq!(
        duplicate_done, None,
        "恢复轮次后的重复 Stop/Done 不得生成第二条 `Henry, Done`"
    );
    assert_eq!(
        idle_process_failure, None,
        "没有任务轮次的进程 Error 仍必须保持静默，不能误报 `Codex, Failed`"
    );
}

/// 终态后的 Ready 生命周期：迟到的重复 Stop 仍属于上一轮，不能被恢复成新任务。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 43
///   task events = 1 次 UserPromptSubmit + 1 次 Stop/Done + 1 次 Ready + 1 次迟到 Stop/Done
///   terminal announcement budget = 同一任务轮次最多 1 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 打开 node-43 的任务轮次并接收首个 Stop/Done → 生成 `Henry, Done`。
///   2. 接收 Ready 生命周期事件 → 已认领终态应保留去重证据，等待下一次真实任务开始。
///   3. 接收迟到的重复 Stop/Done → 识别为上一轮重复终态，不得恢复新轮次。
///
/// 预期结果：
///   - 正断言：首个 Stop/Done 恰好生成一次 `Henry, Done`。
///   - 负断言：Ready 后迟到的重复 Stop/Done 不生成第二条播报。
///   - 负断言：队列中不存在任何额外终态回执。
#[test]
fn test_ready_after_terminal_preserves_deduplication_for_late_stop() {
    let mut announcements = AgentAnnouncements::default();
    let eligible = AnnouncementEligibility::new(true, true);

    announcements.handle_runtime_event(
        &task_event(43, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Henry"),
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(43, AgentTaskState::Done, "Stop"),
        Some("Henry"),
        eligible,
    );
    let first_done = announcements.take_next_ready();

    announcements.handle_runtime_event(
        &task_event(43, AgentTaskState::Ready, "SessionReady"),
        Some("Henry"),
        eligible,
    );
    announcements.handle_runtime_event(
        &task_event(43, AgentTaskState::Done, "Stop"),
        Some("Henry"),
        eligible,
    );
    let late_duplicate = announcements.take_next_ready();

    assert_eq!(
        first_done,
        Some(AgentAnnouncement::new(43, "Henry, Done")),
        "首个 Stop/Done 必须生成唯一的 `Henry, Done`"
    );
    assert_eq!(
        late_duplicate, None,
        "Ready 生命周期事件不得清除终态去重证据，迟到 Stop 不能生成第二条播报"
    );
    assert_eq!(
        announcements.take_next_ready(),
        None,
        "迟到 Stop 被拒绝后队列中不得残留任何额外终态回执"
    );
}

/// TTS 短暂未就绪：终态等待后台校验完成后只释放一次。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 51
///   task round = 1 次 UserPromptSubmit + 1 次 Stop/Done
///   TTS 状态序列 = TemporarilyUnavailable → Ready → Ready
///   terminal announcement budget = 1 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 TTS 后台校验中的状态打开任务轮次并收到 Stop/Done → 保存等待项但不允许取出。
///   2. TTS 第一次进入 Ready → 释放该等待项。
///   3. 再次报告 Ready → 不得重新释放已经领取的终态。
///
/// 预期结果：
///   - 负断言：TTS 未 Ready 时不能提前取得 `Henry, Done`。
///   - 正断言：第一次 Ready 恰好释放 1 条 `Henry, Done`。
///   - 负断言：重复 Ready 后队列为空，不得再次播报。
#[test]
fn test_temporarily_unavailable_tts_releases_terminal_once_when_ready() {
    let mut announcements = AgentAnnouncements::default();
    let waiting_for_tts = AnnouncementEligibility::waiting_for_tts(true);

    announcements.handle_runtime_event(
        &task_event(51, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Henry"),
        waiting_for_tts,
    );
    announcements.handle_runtime_event(
        &task_event(51, AgentTaskState::Done, "Stop"),
        Some("Henry"),
        waiting_for_tts,
    );

    assert_eq!(
        announcements.take_next_ready(),
        None,
        "TTS 仍在后台校验时不得提前取出 `Henry, Done`"
    );

    let first_release =
        announcements.update_tts_availability(AgentAnnouncementTtsAvailability::Ready);
    let released_done = announcements.take_next_ready();
    let duplicate_release =
        announcements.update_tts_availability(AgentAnnouncementTtsAvailability::Ready);

    assert_eq!(
        first_release.released, 1,
        "TTS 第一次 Ready 必须恰好释放 1 条等待中的终态播报"
    );
    assert_eq!(
        released_done,
        Some(AgentAnnouncement::new(51, "Henry, Done")),
        "TTS Ready 后必须释放此前保留的 `Henry, Done`"
    );
    assert_eq!(
        duplicate_release.released, 0,
        "重复 Ready 不得再次释放已经领取的终态播报"
    );
    assert_eq!(
        announcements.take_next_ready(),
        None,
        "释放一次后队列必须为空，不能残留第二条 `Henry, Done`"
    );
}

/// TTS 永久不可用：未配置的终态保持跳过且以后 Ready 也不补播。
///
/// 数据构造（含关键数值的推导过程）：
///   node_id = 52
///   task round = 1 次 UserPromptSubmit + 1 次 Stop/Done
///   TTS 状态序列 = Unavailable → Ready
///   queued announcements = 0 条
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在没有可用 TTS 配置时打开任务轮次并收到 Stop/Done → 消费终态但不入队。
///   2. 后续 TTS 进入 Ready → 队列中没有等待项可释放。
///
/// 预期结果：
///   - 负断言：未配置时不生成 `Henry, Done`。
///   - 正断言：后续 Ready 的释放数量为 0。
///   - 负断言：Ready 后仍不存在迟到补播。
#[test]
fn test_permanently_unavailable_tts_does_not_replay_skipped_terminal() {
    let mut announcements = AgentAnnouncements::default();
    let unavailable = AnnouncementEligibility::new(true, false);

    announcements.handle_runtime_event(
        &task_event(52, AgentTaskState::Thinking, "UserPromptSubmit"),
        Some("Henry"),
        unavailable,
    );
    announcements.handle_runtime_event(
        &task_event(52, AgentTaskState::Done, "Stop"),
        Some("Henry"),
        unavailable,
    );
    let skipped = announcements.take_next_ready();
    let later_ready =
        announcements.update_tts_availability(AgentAnnouncementTtsAvailability::Ready);
    let replayed = announcements.take_next_ready();

    assert_eq!(skipped, None, "TTS 永久不可用时不得生成 `Henry, Done`");
    assert_eq!(
        later_ready.released, 0,
        "未配置 TTS 时跳过的终态不在等待队列中，后续 Ready 必须释放 0 条"
    );
    assert_eq!(
        replayed, None,
        "TTS 后续 Ready 也不得补播此前因永久不可用而跳过的终态"
    );
}
