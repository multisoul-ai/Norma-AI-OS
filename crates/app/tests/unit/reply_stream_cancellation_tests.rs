use super::reply_stream_test_support::{echo_pty_factory, GatedTtsCall, GatedTtsClient};
use super::{DialogueControlState, SharedTtsRuntime};
use dialogue::DialoguePhase;
use gpui::TestAppContext;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, Mutex};

/// STR-CANCEL-1：用户打断时必须同时丢弃未提交语音和已经排队的旧 PCM。
///
/// 数据构造（含关键数值的推导过程）：
///   speech segments       = 2
///   characters/segment    = 60
///   active characters     = 2 × 60 = 120 < 10,000
///   submitted characters  = 60（第一段已提交但未 consume）
///   pending characters    = 60（命名尾段尚未提交）
///   queued PCM             = 1（旧 generation 已交给 playback，尚未完成）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过真实 SpeechBacklog 接口构造一段 submitted、一段 pending，并建立一个旧 PCM pending。
///   2. Speaking 阶段执行确认用户打断 → generation 递增、backlog/PCM 状态清空、sink stop。
///   3. 通过实际 playback completion handler 投递旧 generation completion → 校验拒绝修改新状态。
///
/// 预期结果：
///   - 正断言：打断被接受，旧 generation 的 backlog 与 PCM pending 均归零且 sink 已停止。
///   - 正断言：命名尾段从未提交，旧 completion 之后仍保持 Listening 和新 generation。
///   - 负断言：旧回复不得 finish 成功，也不得被迟到事件恢复为 Speaking。
#[test]
fn test_user_interruption_discards_unsent_backlog_and_queued_pcm_together() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let mut state = DialogueControlState::default();
        state.prepare_reply_test_fixtures();
        state.session.on_listening_started();
        state
            .session
            .on_utterance_final()
            .expect("Listening 收到 final 后必须进入 Thinking");
        state
            .session
            .on_reply_ready()
            .expect("首个 PCM ready 后必须进入 Speaking");
        let first_segment = "首".repeat(60);
        let named_tail = "尾".repeat(60);
        state
            .speech_backlog
            .accept_batch(vec![first_segment, named_tail.clone()])
            .expect("2 × 60 = 120 active characters 必须进入 backlog");
        let first_receipt = state
            .speech_backlog
            .next_submission()
            .expect("第一段必须可提交")
            .receipt();
        state
            .speech_backlog
            .mark_submitted(first_receipt)
            .expect("第一段必须进入 submitted-but-unconsumed 状态");
        state.playback_chunks_pending = 1;
        state.playback_started = true;
        state.speech_synthesis_active = true;
        state.last_reply_text = Some("首尾".to_string());
        state.reply_generation = 41;
        let sink = state.assistant_audio_sink_for_tests();
        let old_generation = state.reply_generation;

        assert_eq!(
            state.speech_backlog.active_chars(),
            120,
            "submitted 60 + pending 60 必须共同占用 120 active characters"
        );
        assert_eq!(
            state
                .speech_backlog
                .next_submission()
                .map(|submission| submission.text()),
            Some(named_tail.as_str()),
            "命名尾段必须明确处于 pending，不能用泛化 backlog 非空代替"
        );
        assert_eq!(state.playback_chunks_pending, 1, "必须有一个旧 PCM pending");

        let interrupted = state.interrupt_assistant_reply_for_user_action();
        let new_generation = state.reply_generation;
        assert!(interrupted, "Speaking 中的确认用户打断必须返回 true");
        assert_eq!(
            new_generation,
            old_generation.wrapping_add(1),
            "打断必须精确递增 generation，使全部旧事件失效"
        );
        assert_eq!(
            state.phase(),
            DialoguePhase::Listening,
            "Continuous Listening 的确认打断必须回到 Listening"
        );
        assert_eq!(
            state.speech_backlog.active_chars(),
            0,
            "打断必须清空 active speech"
        );
        assert_eq!(
            state.playback_chunks_pending, 0,
            "打断必须清空旧 generation 的 PCM pending 计数"
        );
        assert!(
            state.tts_cancelled.load(Ordering::Acquire),
            "打断必须设置共享 TTS cancellation 标记"
        );
        assert!(
            state.last_reply_interrupted,
            "用户确认打断必须保留 interrupted 可见状态"
        );
        assert!(sink.stopped(), "打断必须立即 stop 实际 playback sink");
        assert!(
            state.speech_backlog.next_submission().is_none(),
            "取消后命名尾段不得仍可提交给 Provider"
        );
        assert!(
            !state.speech_finish_queued,
            "被打断的旧回复不得把 stream 终态误报为成功 finish"
        );

        state.on_playback_chunk_finished(old_generation, 7, 3, 99);
        assert_eq!(
            state.phase(),
            DialoguePhase::Listening,
            "迟到的旧 PCM completion 不得把新 generation 恢复为 Speaking"
        );
        assert_eq!(
            state.reply_generation, new_generation,
            "迟到旧事件不得再次改变已经递增的 reply generation"
        );
        assert_eq!(
            state.playback_chunks_pending, 0,
            "迟到旧 completion 不得重建 PCM pending 状态"
        );
    });
}

/// STR-CANCEL-2：真实 Provider 失败必须覆盖先前的容量告警并执行标准清理。
///
/// 数据构造（含关键数值的推导过程）：
///   accepted characters  = 166 × 60 + 40 = 10,000
///   overflow characters  = 10 → attempted active = 10,010 > 10,000
///   Provider permits     = 0；关闭 sender 后首个 push 返回 Transport failure
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 填满 active speech 容量并注入 10 字 overflow，先产生容量告警。
///   2. 关闭 gated Provider permit channel，令正在等待的真实 TtsClient::push_text 失败。
///   3. worker 上报失败 → generic recovery 中止 generation、清空 backlog 并覆盖告警。
///
/// 预期结果：
///   - 正断言：最终错误是 Provider transport failure，backlog 清空且回到 Listening。
///   - 负断言：容量提示不得遮蔽 Provider 失败，也不得出现成功 finish。
#[test]
fn test_provider_failure_overrides_capacity_warning_and_cleans_up() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let mut gpui = TestAppContext::single();
        let (app, cx) = gpui.add_window_view(|_window, cx| {
            crate::app::MultisoulApp::new_with_pty_factory(cx, echo_pty_factory())
        });
        let selection = super::reply::SelectedTtsModel::local(
            std::path::PathBuf::new(),
            tts::TtsModelId::parse("test-provider-failure-model")
                .expect("测试 Provider failure model ID 必须合法"),
            tts::TtsVoiceId::parse("test-provider-failure-voice")
                .expect("测试 Provider failure Voice ID 必须合法"),
        );
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (permit_tx, permit_rx) = mpsc::channel();
        let client = GatedTtsClient::new(Arc::clone(&calls), permit_rx);
        let full_segments = vec!["甲".repeat(60); 166];
        let accepted_tail = "尾".repeat(40);
        let overflow = "溢".repeat(10);

        app.update(cx, |app, cx| {
            app.dialogue.prepare_reply_test_fixtures();
            app.dialogue.shared_tts_runtime =
                SharedTtsRuntime::from_client_for_tests(Some(selection.clone()), Box::new(client));
            app.dialogue.reply_tts_selection_override = Some(selection);
            app.dialogue.session.on_listening_started();
            app.dialogue
                .session
                .on_utterance_final()
                .expect("Listening 收到 final 后必须进入 Thinking");
            app.dialogue.begin_reply_stream();
            for segment in &full_segments {
                app.dialogue.on_agent_text_delta(segment, cx);
            }
            app.dialogue.on_agent_text_delta(&accepted_tail, cx);
            app.dialogue.on_agent_text_delta(&overflow, cx);
            app.dialogue.finish_agent_text_stream(cx);
        });
        let capacity_message = app.read_with(cx, |app, _cx| app.dialogue.error_message.clone());
        assert!(
            capacity_message
                .as_deref()
                .is_some_and(|message| message.contains("10000")),
            "Provider 失败前必须先建立待覆盖的 10,000 capacity 告警；实际 {capacity_message:?}"
        );

        drop(permit_tx);
        cx.run_until_parked();

        let (final_error, final_phase, active, synthesis_active) = app.read_with(cx, |app, _cx| {
            (
                app.dialogue.error_message.clone(),
                app.dialogue.phase(),
                app.dialogue.speech_backlog.active_chars(),
                app.dialogue.speech_synthesis_active,
            )
        });
        let recorded = calls
            .lock()
            .expect("Provider failure call log lock should not be poisoned")
            .clone();
        assert!(
            final_error
                .as_deref()
                .is_some_and(|message| message.contains("gated TTS test permit timed out")),
            "最终错误必须暴露 Provider transport failure；实际 {final_error:?}"
        );
        assert_ne!(
            final_error, capacity_message,
            "真实 Provider failure 必须覆盖先前容量告警，不能继续显示 capacity"
        );
        assert_eq!(
            final_phase,
            DialoguePhase::Listening,
            "Provider failure 的标准 recovery 必须回到 Continuous Listening"
        );
        assert_eq!(
            active, 0,
            "Provider failure recovery 必须清空全部 active speech"
        );
        assert!(
            !synthesis_active,
            "Provider failure recovery 不得留下 synthesis active 卡死状态"
        );
        assert!(
            !recorded.contains(&GatedTtsCall::Finish),
            "Provider push 失败后不得继续报告成功 finish"
        );
    });
}
