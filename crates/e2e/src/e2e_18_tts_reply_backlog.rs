use crate::{echo_factory, make_app};
use gpui::TestAppContext;

static REPLY_TTS_E2E_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// E2E-TTS-BACKLOG-1：慢速 TTS Provider 必须按序消费四十段长回复且不受历史十五字边界影响。
///
/// 数据构造（含关键数值的推导过程）：
///   speech segments       = 40
///   characters/segment    = 60 > historical 15-character boundary
///   excess per segment    = 60 - 15 = 45 characters
///   active characters     = 40 × 60 = 2,400
///   active capacity       = 10,000
///   remaining capacity    = 10,000 - 2,400 = 7,600
///   Provider permits      = 40（每段 push_text 消费一个 permit）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动真实 headless GPUI App，注入实例隔离的 gated TTS 与即时完成播放 sink。
///   2. 一次提交四十个 60 字完整语音段并结束模型流；Provider 暂无 permit，backlog 保留 2,400 字。
///   3. 释放四十个 permit，推进 background executor、TTS worker、PCM playback 到稳定状态。
///
/// 预期结果：
///   - 正断言：每个超过十五字的首段、中段、尾段严格按序完整 push，且只 start/finish 一次。
///   - 正断言：四十块 PCM 全部进入播放 sink，完整显示文本保留并回到 Listening。
///   - 负断言：不得产生十五字省略号截断、cancel、错误标签或丢失第 40 段。
#[test]
fn slow_tts_provider_drains_complete_reply_through_headless_app() {
    let _guard = REPLY_TTS_E2E_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let mut segments = vec!["乙".repeat(60); 40];
    segments[0] = "甲".repeat(60);
    segments[19] = "中".repeat(60);
    segments[39] = "尾".repeat(60);
    let visible_reply = segments.concat();

    let probe = app.update(cx, |app, cx| {
        let probe = app.install_gated_reply_tts_for_e2e(cx);
        app.begin_static_reply_for_e2e(visible_reply.clone(), cx);
        probe
    });
    let blocked = app.read_with(cx, |app, _cx| app.reply_tts_e2e_snapshot());

    assert_eq!(
        blocked.active_speech_chars, 2_400,
        "Provider 停滞时 active backlog 必须保留 40 × 60 = 2,400 字"
    );
    assert_eq!(
        blocked.visible_reply.as_deref(),
        Some(visible_reply.as_str()),
        "TTS 尚未消费时完整四十段显示文本也必须立即保留"
    );

    probe.release_segments(40);
    cx.run_until_parked();

    let calls = probe.snapshot();
    let completed = app.read_with(cx, |app, _cx| app.reply_tts_e2e_snapshot());
    assert_eq!(calls.starts, 1, "一个模型回复必须只启动一个 TTS stream");
    assert_eq!(calls.pushes.len(), 40, "四十段文本必须恰好各 push 一次");
    assert_eq!(calls.pushes[0], segments[0], "第一段必须保持原始顺序");
    assert_eq!(
        calls.pushes[0].chars().count(),
        60,
        "首个 TTS push 必须保留完整 60 字，不能回退到历史十五字边界"
    );
    assert!(
        !calls.pushes[0].ends_with('…'),
        "首个长语音段不得被改写为十五字加省略号的截断内容"
    );
    assert_eq!(calls.pushes[19], segments[19], "中间段必须保持原始顺序");
    assert_eq!(calls.pushes[39], segments[39], "最后一段不能被背压丢失");
    assert_eq!(calls.finishes, 1, "全部四十段消费后必须恰好 finish 一次");
    assert_eq!(calls.cancels, 0, "正常慢消费不得触发 cancel");
    assert_eq!(
        calls.pcm_writes.len(),
        40,
        "每个语音段产生的一块 PCM 都必须进入播放 sink"
    );
    assert_eq!(
        completed.visible_reply.as_deref(),
        Some(visible_reply.as_str()),
        "播放完成后显示文本仍必须保留完整四十段"
    );
    assert_eq!(
        completed.phase, "listening",
        "完整合成和播放收束后 Continuous Listening 必须回到 Listening"
    );
    assert_eq!(
        completed.voice_label.as_deref(),
        Some("Continuous Listening"),
        "正常完成不得留下 Speech error 或容量错误标签"
    );
    assert!(
        completed
            .visible_reply
            .as_deref()
            .is_some_and(|reply| reply.ends_with(&segments[39])),
        "完整回复必须明确以第 40 段结尾，防止只验证前缀"
    );

    app.update(cx, |app, cx| app.remove_reply_tts_for_e2e(cx));
}

/// E2E-TTS-BACKLOG-2：快速本地消费累计 12,000 字时必须复用 active 容量而非整轮截断。
///
/// 数据构造（含关键数值的推导过程）：
///   speech segments       = 200
///   characters/segment    = 60
///   cumulative characters = 200 × 60 = 12,000
///   active per step       ≤ 60 < 10,000（每段后推进 worker 并收到 consumed ack）
///   Provider permits      = 200（测试开始前全部释放，模拟无网络等待的本地 Provider）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动一轮 reply stream，预先释放 200 个 Provider permit。
///   2. 逐段提交 200 个 60 字 delta；每段后推进 executor，使 active 容量释放后复用。
///   3. 结束模型流并推进合成、PCM 播放和 Dialogue 生命周期到稳定状态。
///
/// 预期结果：
///   - 正断言：累计 12,000 字全部按序 push，且只 start/finish 一次。
///   - 正断言：首、中、尾 PCM 标记按 push 顺序进入播放 sink，格式保持 PCM16/16k/mono。
///   - 负断言：不得触发 10,000 active 容量消息、不得选择第二客户端或 cancel。
#[test]
fn fast_local_tts_reuses_capacity_beyond_ten_thousand_cumulative_characters() {
    let _guard = REPLY_TTS_E2E_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut gpui = TestAppContext::single();
    let (app, cx) = make_app(&mut gpui, echo_factory());
    let mut segments = vec!["快".repeat(60); 200];
    segments[0] = "首".repeat(60);
    segments[99] = "中".repeat(60);
    segments[199] = "尾".repeat(60);
    let visible_reply = segments.concat();

    let probe = app.update(cx, |app, cx| {
        let probe = app.install_gated_reply_tts_for_e2e(cx);
        probe.release_segments(200);
        app.begin_reply_stream_for_e2e(cx);
        probe
    });
    for segment in &segments {
        app.update(cx, |app, cx| app.push_reply_delta_for_e2e(segment, cx));
        cx.run_until_parked();
    }
    app.update(cx, |app, cx| app.finish_reply_stream_for_e2e(cx));
    cx.run_until_parked();

    let calls = probe.snapshot();
    let completed = app.read_with(cx, |app, _cx| app.reply_tts_e2e_snapshot());
    assert_eq!(
        calls.starts, 1,
        "12,000 字仍必须复用同一个 TTS client/stream"
    );
    assert_eq!(calls.pushes.len(), 200, "200 段累计 12,000 字必须全部 push");
    assert_eq!(calls.pushes[0], segments[0], "第 1 段必须保持原始顺序");
    assert_eq!(calls.pushes[99], segments[99], "第 100 段必须保持原始顺序");
    assert_eq!(
        calls.pushes[199], segments[199],
        "第 200 段尾部不能被 10,000 截断"
    );
    assert_eq!(calls.finishes, 1, "第 200 段消费后必须恰好 finish 一次");
    assert_eq!(calls.cancels, 0, "快速本地消费不得 cancel 或切换第二客户端");
    assert_eq!(
        calls.pcm_writes.len(),
        200,
        "200 块 PCM 必须全部进入播放 sink"
    );
    assert_eq!(
        calls.pcm_writes[0].sequence_marker, 1,
        "第一块 PCM 的顺序标记必须保持为 1"
    );
    assert_eq!(
        calls.pcm_writes[99].sequence_marker, 100,
        "中间 PCM 的顺序标记必须保持为 100"
    );
    assert_eq!(
        calls.pcm_writes[199].sequence_marker, 200,
        "尾部 PCM 的顺序标记必须保持为 200"
    );
    assert!(
        calls.pcm_writes.iter().all(|pcm| {
            pcm.format == "pcm16"
                && pcm.sample_rate_hz == 16_000
                && pcm.channels == 1
                && pcm.byte_count == 3_840
        }),
        "全部本地非回归 PCM 必须保持 PCM16、16kHz、mono、3,840 bytes"
    );
    assert_eq!(
        completed.visible_reply.as_deref(),
        Some(visible_reply.as_str()),
        "累计 12,000 字的显示文本必须完整保留"
    );
    assert_eq!(
        completed.active_speech_chars, 0,
        "完成后 active backlog 必须释放为 0"
    );
    assert_eq!(
        completed.voice_label.as_deref(),
        Some("Continuous Listening"),
        "累计超过 10,000 但 active 未超限时不得显示容量错误"
    );

    app.update(cx, |app, cx| app.remove_reply_tts_for_e2e(cx));
}
