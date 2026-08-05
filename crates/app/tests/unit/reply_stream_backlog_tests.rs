use super::reply_stream_test_support::{echo_pty_factory, GatedTtsCall, GatedTtsClient};
use super::SharedTtsRuntime;
use dialogue::DialoguePhase;
use gpui::TestAppContext;
use std::sync::{mpsc, Arc, Mutex};

/// STR-BACKLOG-1：单个模型 delta 产生九个完整语音段时不得触发旧 staging queue 错误。
///
/// 数据构造（含关键数值的推导过程）：
///   complete sentences  = 9（每句不少于 8 字，均可立即形成独立 segment）
///   old staging slots   = 8
///   overflow            = 9 - 8 = 1 segment
///   active characters   < 9 × 60 = 540 < 10,000
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离 App 实例中配置 Fake TTS 与 Recording playback，进入 Thinking。
///   2. 开始 reply stream，并一次注入含九个完整句子的模型 delta。
///   3. 结束模型文本流并推进前台/后台任务直到稳定，再读取显示文本和错误状态。
///
/// 预期结果：
///   - 正断言：完整九句显示文本必须保留，并最终完成 reply 生命周期。
///   - 正断言：九个 segment 均处于 10,000 active-character 容量范围内。
///   - 负断言：不得出现旧的 bounded reply queue 错误或丢失第九句。
#[test]
fn test_one_delta_with_nine_segments_does_not_fail_bounded_reply_queue() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let mut gpui = TestAppContext::single();
        let (app, cx) = gpui.add_window_view(|_window, cx| {
            crate::app::MultisoulApp::new_with_pty_factory(cx, echo_pty_factory())
        });
        let sentences = [
            "第一句话应该完整播报。",
            "第二句话应该完整播报。",
            "第三句话应该完整播报。",
            "第四句话应该完整播报。",
            "第五句话应该完整播报。",
            "第六句话应该完整播报。",
            "第七句话应该完整播报。",
            "第八句话应该完整播报。",
            "第九句话结论不能丢失。",
        ];
        let reply = sentences.concat();
        app.update(cx, |app, cx| {
            app.dialogue.prepare_reply_test_fixtures();
            app.dialogue.reply_tts_selection_override =
                Some(super::reply::SelectedTtsModel::local(
                    std::path::PathBuf::new(),
                    tts::TtsModelId::parse("test-backlog-model")
                        .expect("测试 TTS model ID 必须合法"),
                    tts::TtsVoiceId::parse("test-backlog-voice")
                        .expect("测试 TTS Voice ID 必须合法"),
                ));
            app.dialogue.session.on_listening_started();
            app.dialogue
                .session
                .on_utterance_final()
                .expect("Listening 收到 final 后必须进入 Thinking");
            app.dialogue.begin_reply_stream();
            app.dialogue.on_agent_text_delta(&reply, cx);
            app.dialogue.finish_agent_text_stream(cx);
        });
        cx.run_until_parked();

        let (visible_reply, error_message, phase) = app.read_with(cx, |app, _cx| {
            (
                app.dialogue.last_reply_text.clone(),
                app.dialogue.error_message.clone(),
                app.dialogue.phase(),
            )
        });
        assert_eq!(
            visible_reply.as_deref(),
            Some(reply.as_str()),
            "一次 delta 的完整九句显示文本必须保留，包括第九句结论"
        );
        assert!(
            error_message.is_none(),
            "九个 segment 小于 active 容量，不得产生 TTS 错误；实际：{error_message:?}"
        );
        assert_eq!(
            phase,
            DialoguePhase::Listening,
            "九段语音合成和播放完成后 Continuous Listening 必须回到 Listening"
        );
        assert!(
            visible_reply
                .as_deref()
                .is_some_and(|text| text.ends_with(sentences[8])),
            "完整回复必须明确保留第九句，不能只通过前八句存在而通过"
        );
    });
}

/// STR-BACKLOG-2：慢速 Provider 面对四十个 60 字 segment 时必须完整消费 backlog 后再 finish。
///
/// 数据构造（含关键数值的推导过程）：
///   speech segments       = 40
///   characters/segment    = 60
///   active characters     = 40 × 60 = 2,400
///   old pending envelope  = 16 × 60 = 960
///   new remaining budget  = 10,000 - 2,400 = 7,600
///   Provider permits      = 40（每个 push_text 恰好消费 1 个）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 给单个 App runtime 注入可控 GatedTtsClient，并让第一个 push 等待 permit。
///   2. 连续注入四十个 60 字 delta 后结束模型流，使 command channel 和 backlog 同时积压。
///   3. 读取 active backlog，随后释放四十个 permit 并推进任务直到播放完成。
///
/// 预期结果：
///   - 正断言：active backlog 在 Provider 停滞时保留全部 2,400 字。
///   - 正断言：首段、中段和尾段严格按序各 push 一次，尾段之后恰好 finish 一次。
///   - 负断言：不得出现旧 bounded queue 错误、提前 finish、cancel 或第 40 段丢失。
#[test]
fn test_slow_provider_drains_forty_segments_before_finish_without_truncation() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let mut gpui = TestAppContext::single();
        let (app, cx) = gpui.add_window_view(|_window, cx| {
            crate::app::MultisoulApp::new_with_pty_factory(cx, echo_pty_factory())
        });
        let selection = super::reply::SelectedTtsModel::local(
            std::path::PathBuf::new(),
            tts::TtsModelId::parse("test-slow-backlog-model")
                .expect("测试 slow TTS model ID 必须合法"),
            tts::TtsVoiceId::parse("test-slow-backlog-voice")
                .expect("测试 slow TTS Voice ID 必须合法"),
        );
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (permit_tx, permit_rx) = mpsc::channel();
        let client = GatedTtsClient::new(Arc::clone(&calls), permit_rx);
        let mut segments = vec!["乙".repeat(60); 40];
        segments[0] = "甲".repeat(60);
        segments[19] = "中".repeat(60);
        segments[39] = "尾".repeat(60);
        let visible_reply = segments.concat();

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
            for segment in &segments {
                app.dialogue.on_agent_text_delta(segment, cx);
            }
            app.dialogue.finish_agent_text_stream(cx);
        });

        let (active_before_release, pending_before_release, error_before_release) =
            app.read_with(cx, |app, _cx| {
                (
                    app.dialogue.speech_backlog.active_chars(),
                    app.dialogue.speech_backlog.pending_len(),
                    app.dialogue.error_message.clone(),
                )
            });
        assert_eq!(
            active_before_release, 2_400,
            "Provider 停滞时 pending 加 submitted 必须保留 40 × 60 = 2,400 字"
        );
        assert!(
            pending_before_release >= 31,
            "command channel 最多容纳 8 段且 1 段正在执行，至少 31 段必须留在 backlog；实际 {pending_before_release}"
        );
        assert!(
            error_before_release.is_none(),
            "2,400 active characters 小于容量，不得误报错误；实际 {error_before_release:?}"
        );

        for _ in 0..40 {
            permit_tx
                .send(())
                .expect("四十个 gated TTS permit 必须全部送达同一 worker");
        }
        cx.run_until_parked();

        let recorded = calls
            .lock()
            .expect("gated TTS call log lock should not be poisoned")
            .clone();
        let pushes = recorded
            .iter()
            .filter_map(|call| match call {
                GatedTtsCall::Push(text) => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            recorded.first(),
            Some(&GatedTtsCall::Start),
            "完整回复必须先且只先 start TTS stream"
        );
        assert_eq!(
            pushes.len(),
            40,
            "四十个 accepted segment 必须各 push 一次，不能遗漏或重复"
        );
        assert_eq!(
            pushes.first().map(String::as_str),
            Some(segments[0].as_str()),
            "第一段必须保持原始顺序"
        );
        assert_eq!(
            pushes.get(19).map(String::as_str),
            Some(segments[19].as_str()),
            "第 20 段中间标记必须保持原始顺序"
        );
        assert_eq!(
            pushes.get(39).map(String::as_str),
            Some(segments[39].as_str()),
            "第 40 段尾部标记必须存在，不能被 backlog 截断"
        );
        assert_eq!(
            recorded.last(),
            Some(&GatedTtsCall::Finish),
            "finish 必须恰好位于第 40 段之后"
        );
        assert_eq!(
            recorded
                .iter()
                .filter(|call| matches!(call, GatedTtsCall::Finish))
                .count(),
            1,
            "完整 drain 后必须恰好 finish 一次"
        );
        assert!(
            !recorded
                .iter()
                .any(|call| matches!(call, GatedTtsCall::Cancel)),
            "健康慢速 Provider 不得被误取消"
        );
        let (final_reply, final_error, final_phase, written_chunks) =
            app.read_with(cx, |app, _cx| {
                (
                    app.dialogue.last_reply_text.clone(),
                    app.dialogue.error_message.clone(),
                    app.dialogue.phase(),
                    app.dialogue
                        .assistant_audio_sink_for_tests()
                        .chunks_written(),
                )
            });
        assert_eq!(
            final_reply.as_deref(),
            Some(visible_reply.as_str()),
            "显示文本必须保留全部四十段，包括尾段"
        );
        assert!(
            final_error.is_none(),
            "完整 drain 后不得残留 bounded queue 或 Provider 错误；实际 {final_error:?}"
        );
        assert_eq!(
            final_phase,
            DialoguePhase::Listening,
            "四十段实际播放完成后必须回到 Listening"
        );
        assert_eq!(
            written_chunks, 40,
            "每个 120ms PCM segment 必须恰好调度一次播放"
        );
    });
}

/// STR-BACKLOG-3：active speech 超过 10,000 字时必须保留完整显示文本并排空已接受前缀。
///
/// 数据构造（含关键数值的推导过程）：
///   full segments        = 166 × 60 + 40 = 10,000 active characters
///   overflow sentence    = 10 characters，令 attempted active = 10,010
///   accepted pushes      = 166 + 1 tail = 167
///   rejected pushes      = 1 overflow sentence + 1 later visible sentence
///   Provider permits     = 167（只对应已接受前缀）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 gated Provider 尚未消费时填满恰好 10,000 active characters。
///   2. 注入 overflow 与后续文字 → 继续扩展显示文本，但关闭本轮 speech input。
///   3. 释放 167 个 permit → 已接受前缀完整 push、finish 和播放，拒绝段不进入 TTS。
///
/// 预期结果：
///   - 正断言：容量消息立即可见，完整文字包含 overflow 后的最终句。
///   - 正断言：已接受的首段和第 167 段尾部均播报，随后恰好 finish 一次。
///   - 负断言：overflow 与后续句不得部分进入 TTS，也不得调用 generic abort/cancel。
#[test]
fn test_active_capacity_exhaustion_preserves_visible_reply_and_drains_accepted_prefix() {
    super::app_dialogue_shell_tests::with_dialogue_shell_test_guard(|| {
        let mut gpui = TestAppContext::single();
        let (app, cx) = gpui.add_window_view(|_window, cx| {
            crate::app::MultisoulApp::new_with_pty_factory(cx, echo_pty_factory())
        });
        let selection = super::reply::SelectedTtsModel::local(
            std::path::PathBuf::new(),
            tts::TtsModelId::parse("test-capacity-model")
                .expect("测试 capacity TTS model ID 必须合法"),
            tts::TtsVoiceId::parse("test-capacity-voice")
                .expect("测试 capacity TTS Voice ID 必须合法"),
        );
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (permit_tx, permit_rx) = mpsc::channel();
        let client = GatedTtsClient::new(Arc::clone(&calls), permit_rx);
        let full_segments = vec!["甲".repeat(60); 166];
        let accepted_tail = format!("{}。", "尾".repeat(39));
        let overflow = "这一段容量明确溢出。";
        let visible_after_overflow = "但完整文字仍然必须显示。";
        let expected_visible = format!(
            "{}{}{}{}",
            full_segments.concat(),
            accepted_tail,
            overflow,
            visible_after_overflow
        );

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
            app.dialogue.on_agent_text_delta(overflow, cx);
            app.dialogue.on_agent_text_delta(visible_after_overflow, cx);
            app.dialogue.finish_agent_text_stream(cx);
        });

        let (active, capacity_exceeded, visible_before_drain, error_before_drain) =
            app.read_with(cx, |app, _cx| {
                (
                    app.dialogue.speech_backlog.active_chars(),
                    app.dialogue.speech_backlog.capacity_exceeded(),
                    app.dialogue.last_reply_text.clone(),
                    app.dialogue.error_message.clone(),
                )
            });
        assert_eq!(
            active, 10_000,
            "166 × 60 + 40 accepted characters 必须恰好占用 10,000 active 容量"
        );
        assert!(
            capacity_exceeded,
            "再到达 10 字 overflow sentence 后必须记录 capacity exceeded"
        );
        assert_eq!(
            visible_before_drain.as_deref(),
            Some(expected_visible.as_str()),
            "容量超限不得阻止 overflow 和后续句继续进入完整显示文本"
        );
        assert!(
            error_before_drain.as_deref().is_some_and(|message| {
                message.contains("10000") && message.contains("complete reply remains visible")
            }),
            "容量消息必须说明 10,000 边界和完整文字仍可见；实际 {error_before_drain:?}"
        );

        for _ in 0..167 {
            permit_tx
                .send(())
                .expect("167 个 accepted segment 必须各获得一个 gated permit");
        }
        cx.run_until_parked();

        let recorded = calls
            .lock()
            .expect("capacity TTS call log lock should not be poisoned")
            .clone();
        let pushes = recorded
            .iter()
            .filter_map(|call| match call {
                GatedTtsCall::Push(text) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            pushes.len(),
            167,
            "只允许 166 个满段和一个 40 字尾段进入 TTS，共 167 次 push"
        );
        assert_eq!(
            pushes.first().copied(),
            Some(full_segments[0].as_str()),
            "capacity drain 必须保留第一个 accepted segment"
        );
        assert_eq!(
            pushes.get(166).copied(),
            Some(accepted_tail.as_str()),
            "capacity drain 必须保留第 167 个 accepted tail"
        );
        assert!(
            !pushes.contains(&overflow),
            "触发 overflow 的 batch 必须原子拒绝，不能部分进入 TTS"
        );
        assert!(
            !pushes.contains(&visible_after_overflow),
            "speech input 关闭后后续可见句不得重新进入 TTS"
        );
        assert_eq!(
            recorded.last(),
            Some(&GatedTtsCall::Finish),
            "已接受前缀 drain 后必须以唯一 finish 结束"
        );
        assert!(
            !recorded
                .iter()
                .any(|call| matches!(call, GatedTtsCall::Cancel)),
            "capacity exhaustion 是受控 speech terminal，不得调用 generic cancel"
        );
        let (final_visible, final_error, final_phase, written_chunks) =
            app.read_with(cx, |app, _cx| {
                (
                    app.dialogue.last_reply_text.clone(),
                    app.dialogue.error_message.clone(),
                    app.dialogue.phase(),
                    app.dialogue
                        .assistant_audio_sink_for_tests()
                        .chunks_written(),
                )
            });
        assert_eq!(
            final_visible.as_deref(),
            Some(expected_visible.as_str()),
            "TTS terminal 完成后完整显示文本仍不得被 accepted prefix 覆盖"
        );
        assert_eq!(
            final_error, error_before_drain,
            "正常排空 accepted prefix 后必须保留 capacity 诊断，不能伪装成成功"
        );
        assert_eq!(
            final_phase,
            DialoguePhase::Listening,
            "模型、TTS 和实际播放均完成后必须回到 Listening"
        );
        assert_eq!(
            written_chunks, 167,
            "167 个 accepted segment 必须各播放一次且不包含 rejected speech"
        );
    });
}
