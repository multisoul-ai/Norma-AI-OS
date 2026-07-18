use super::{app_dialogue_shell_tests, DialogueControlState};
use dialogue::DialoguePhase;
use tempfile::tempdir;

/// DLG-10b：任务终态在 Idle 或合成未完成时保留，成功播报后才从队列确认消费。
///
/// 数据构造（含关键数值的推导过程）：
///   initial phase = Idle
///   queued reports = 1 条“已完成：创建卡片 #1”
///   successful consumptions = 1 次，因此最终 reports = 1 - 1 = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Idle 时尝试开始报告 → 保留报告，但不占用其他独立 TTS 的优先级。
///   2. 启动 Listening → 报告变为可消费并取得语音优先级。
///   3. 模拟监听关闭再开启 → 同一报告仍可重试。
///   4. 模拟成功播报确认 → 从队列移除唯一报告。
///
/// 预期结果：
///   - 正断言：Listening 阶段的待播报告会阻止其他语音抢占。
///   - 负断言：Idle 阶段无法消费的报告不阻塞 Agent 终态播报，也不会丢失。
///   - 正断言：只有成功确认后报告队列才清空。
#[test]
fn test_agent_terminal_report_waits_for_listening_then_enters_playback_flow() {
    app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let dir = tempdir().expect("temp model dir should be creatable");
        let values =
            app_dialogue_shell_tests::continuous_listening_values(Some(dir.path().to_path_buf()));
        let mut state = DialogueControlState::default();
        state.queue_agent_report("已完成：创建卡片 #1".to_string());

        assert_eq!(
            state.begin_next_agent_report(),
            None,
            "Idle 时必须保留报告，等待 Continuous Listening 恢复"
        );
        assert_eq!(
            state.queued_template_reports_for_tests(),
            &["已完成：创建卡片 #1".to_string()],
            "Idle 尝试不得丢失报告"
        );
        assert!(
            !state.has_pending_spoken_reply(),
            "Idle 阶段无法消费的 Continuous Listening 报告不得阻塞其他独立 TTS 播报"
        );

        DialogueControlState::set_mic_permission_check_for_tests(Some(
            app_dialogue_shell_tests::mic_ok,
        ));
        state
            .begin_listening_sync_for_tests(&values, dir.path())
            .expect("测试启动应进入 Listening");
        assert!(
            state.has_pending_spoken_reply(),
            "进入 Listening 后可消费的模板报告必须恢复语音优先级"
        );

        assert_eq!(
            state.begin_next_agent_report(),
            Some("已完成：创建卡片 #1".to_string()),
            "Listening 后应按 FIFO 开始合成真实终态报告"
        );
        assert_eq!(
            state.phase(),
            DialoguePhase::Thinking,
            "取得报告后应进入后台 TTS 合成阶段"
        );
        assert_eq!(
            state.queued_template_reports_for_tests(),
            &["已完成：创建卡片 #1".to_string()],
            "TTS 成功前必须保留报告，避免合成失败或监听关闭时永久丢失"
        );

        state.stop_shell();
        assert_eq!(
            state.queued_template_reports_for_tests(),
            &["已完成：创建卡片 #1".to_string()],
            "关闭 Continuous Listening 不得清除尚未播报的终态"
        );

        DialogueControlState::set_mic_permission_check_for_tests(Some(
            app_dialogue_shell_tests::mic_ok,
        ));
        state
            .begin_listening_sync_for_tests(&values, dir.path())
            .expect("重新启动 Listening 应成功");
        assert_eq!(
            state.begin_next_agent_report(),
            Some("已完成：创建卡片 #1".to_string()),
            "重新开启后必须再次取得同一条未确认报告"
        );
        state.complete_current_agent_report();
        assert!(
            state.queued_template_reports_for_tests().is_empty(),
            "只有成功播报确认后才能移除队首报告"
        );
    });
}
