use super::{app_dialogue_shell_tests, DialogueControlState};
use dialogue::DialoguePhase;
use tempfile::tempdir;

/// DLG-10b：任务终态在 Idle 或合成未完成时保留，成功播报后才从队列确认消费。
///
/// 数据构造：DialogueControlState 初始 Idle，先排入“已完成：创建卡片 #1”。
/// 执行过程：Idle 时尝试开始报告；启动 Listening 后开始合成；模拟监听关闭再开启并重试；
/// 最后模拟成功播报确认消费。
/// 预期结果：Idle、合成中和监听关闭都不丢报告；成功确认后才清空队首。
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

        DialogueControlState::set_mic_permission_check_for_tests(Some(
            app_dialogue_shell_tests::mic_ok,
        ));
        state
            .begin_listening_sync_for_tests(&values, dir.path())
            .expect("测试启动应进入 Listening");

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
