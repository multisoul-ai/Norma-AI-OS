use super::*;

/// Workspace 切换：旧工作区的活动轮次与待播回执必须一次性清空。
///
/// 数据构造（含关键数值的推导过程）：
///   node-1 = 已排队 `Raya, Done`
///   node-2 = 已打开但尚未终结的活动轮次
///   reset count = 1 次 workspace 成功切换
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建一条旧工作区待播回执，并保留另一个活动轮次。
///   2. 调用 workspace reset → 同时清空 pending 与 rounds。
///   3. 注入复用 node-2 的进程 Error → 不得继承旧工作区活动资格。
///
/// 预期结果：
///   - 负断言：旧工作区 `Raya, Done` 不再可取。
///   - 负断言：复用 node id 的进程 Error 不生成 `Codex, Failed`。
#[test]
fn test_workspace_reset_clears_pending_and_active_rounds() {
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

    announcements.reset();
    announcements.handle_runtime_event(
        &RuntimeEvent::NodeRunStateChanged {
            node_id: 2,
            from: NodeRunState::Running,
            to: NodeRunState::Error {
                message: "new workspace process error".to_string(),
            },
        },
        Some("Codex"),
        eligible,
    );

    assert_eq!(
        announcements.take_next_ready(),
        None,
        "workspace reset 后旧 pending 与旧 active round 都必须消失"
    );
    assert_eq!(
        announcements.take_next_ready(),
        None,
        "复用旧 node id 不得从前一 workspace 继承失败播报资格"
    );
}

/// TTS 配置切换：Agent 播报必须释放旧模型/Voice client，下一轮重新创建。
///
/// 数据构造（含关键数值的推导过程）：
///   cached clients = 1 个 FakeTtsClient
///   configuration changes = 1 次
///   expected cached clients after invalidation = 1 - 1 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向 Agent 播放状态注入一个代表旧模型/Voice 的缓存 client。
///   2. 执行配置切换专用失效操作并取出旧 client。
///   3. 检查播放状态内不再保留旧 client。
///
/// 预期结果：
///   - 正断言：失效操作返回旧 client，供调用方后台释放。
///   - 负断言：播放状态不得继续缓存旧模型/Voice client。
#[test]
fn test_tts_configuration_change_invalidates_cached_agent_client() {
    let mut playback = AgentAnnouncementPlayback {
        tts_client: Some(crate::app::dialogue::fake_tts_client_for_tests()),
        ..AgentAnnouncementPlayback::default()
    };

    let stale_client = playback.take_tts_client_for_configuration_change();

    assert!(
        stale_client.is_some(),
        "配置切换必须取出旧 Agent TTS client，供后台线程释放"
    );
    assert!(
        playback.tts_client.is_none(),
        "配置切换后播放状态不得继续缓存旧模型或旧 Voice client"
    );
}
