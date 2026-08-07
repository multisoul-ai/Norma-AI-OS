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

/// TTS 配置切换：共享 runtime 必须释放旧模型/Voice client，下一轮重新创建。
///
/// 数据构造（含关键数值的推导过程）：
///   cached clients = 1 个 FakeTtsClient
///   configuration changes = 1 次
///   expected cached clients after invalidation = 1 - 1 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 向共享 runtime 注入一个代表旧模型/Voice 的缓存 client。
///   2. 执行 Dialogue 配置切换失效操作并替换 resident runtime。
///   3. 检查新 runtime 为空，旧 runtime handle 仍持有 client、可整体移到后台释放。
///
/// 预期结果：
///   - 正断言：失效前共享 runtime 恰好有一个 client。
///   - 正断言：失效操作返回旧 runtime handle，供调用方移到后台线程释放。
///   - 负断言：失效后共享 runtime 不得继续缓存旧模型/Voice client。
#[test]
fn test_tts_configuration_change_invalidates_shared_client() {
    let mut dialogue = crate::app::dialogue::DialogueControlState::default();
    dialogue
        .shared_tts_runtime
        .with_client(
            None,
            || Ok::<_, String>(crate::app::dialogue::fake_tts_client_for_tests()),
            |_tts| Ok::<_, String>(()),
        )
        .expect("Fake TTS runtime 初始化不应失败");

    assert_eq!(
        dialogue
            .shared_tts_runtime
            .resident_client_count_for_tests(),
        1,
        "配置切换前共享 runtime 必须恰好缓存一个旧 TTS client"
    );
    let stale_runtime = dialogue.cancel_tts_for_configuration_change();
    assert_eq!(
        stale_runtime.resident_client_count_for_tests(),
        1,
        "配置切换必须返回持有旧 client 的 runtime，供调用方在后台释放"
    );
    assert_eq!(
        dialogue
            .shared_tts_runtime
            .resident_client_count_for_tests(),
        0,
        "配置切换后共享 runtime 不得继续缓存旧模型或旧 Voice client"
    );
}
