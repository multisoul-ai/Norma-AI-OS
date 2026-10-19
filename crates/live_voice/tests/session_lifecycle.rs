use audio::{PcmAudioChunk, PcmFormat};
use live_voice::{LiveConfig, LiveError, LiveEvent, LiveSession, LiveUsage};
use serde_json::{json, Value};

/// AC01：必须等待官方 session.started 才能发送麦克风音频。
///
/// 数据构造：16kHz × 20ms × 2字节 = 640字节；会话标识来自假上游。
/// 执行过程：启动会话，先尝试发送PCM，再接收started并再次发送。
/// 预期结果：ready前拒绝；ready后按Live JSON/base64协议发送，不能是Realtime提交。
#[test]
fn microphone_waits_for_started_then_streams_live_audio() {
    let mut session = LiveSession::new(LiveConfig {
        instructions: "用中文与用户交流".into(),
        ..LiveConfig::default()
    });
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 640])
        .expect("20ms mono PCM16应有效");
    let start: Value = serde_json::from_str(&session.start_message().expect("应生成首条启动事件"))
        .expect("启动消息应为JSON");
    assert_eq!(start["type"], "session.start", "首条消息必须启动Live会话");
    assert_eq!(start["session"]["model"], "gpt-live-1", "使用真实Live模型");
    assert_eq!(
        start["session"]["delegation"],
        json!({"type":"client"}),
        "保留本地Agent委派"
    );
    assert_eq!(
        start["session"]["audio"]["format"],
        json!({"type":"audio/pcm","rate":16000}),
        "上下行声明16k PCM"
    );
    assert_eq!(start["session"]["store"], false, "不保存云端会话录音");
    assert_eq!(
        session.audio_message(&pcm),
        Err(LiveError::NotReady),
        "started之前不得发送音频"
    );

    session
        .receive(r#"{"type":"session.started","session":{"id":"live_test","model":"gpt-live-1"}}"#)
        .expect("应接受官方ready事件");
    let message: Value =
        serde_json::from_str(&session.audio_message(&pcm).expect("ready后必须能发送PCM"))
            .expect("音频消息必须为JSON文本");
    assert_eq!(
        message["type"], "session.input_audio.append",
        "音频走Live事件"
    );
    assert!(
        message["audio"].as_str().is_some_and(|s| !s.is_empty()),
        "音频须编码为base64"
    );
    assert!(message.get("commit").is_none(), "持续音频不使用逐轮commit");
}

/// AC02：接收模型音频不能使持续收音进入互斥的Speaking状态。
///
/// 数据构造：AQACAA==是两个PCM16LE样本1、2，输出仍声明16kHz。
/// 执行过程：ready后交错接收输出、发送输入，再接收第二段输出。
/// 预期结果：两次输出解码一致；中间输入仍可发送，没有等音频done的要求。
#[test]
fn output_audio_and_microphone_continue_independently() {
    let mut session = LiveSession::new(LiveConfig::default());
    session.start_message().expect("应启动会话");
    session
        .receive(
            r#"{"type":"session.started","session":{"id":"live_duplex","model":"gpt-live-1"}}"#,
        )
        .expect("应进入ready");
    let output = r#"{"type":"session.output_audio.delta","delta":"AQACAA=="}"#;
    let expected = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![1, 0, 2, 0])
        .expect("两个16bit样本应有效");
    assert_eq!(
        session.receive(output),
        Ok(LiveEvent::OutputAudio(expected.clone())),
        "收到的音频必须解码为可播放的PCM"
    );
    let microphone: Value = serde_json::from_str(
        &session
            .audio_message(&expected)
            .expect("播放期间必须继续收音"),
    )
    .expect("输入消息应有效");
    assert_eq!(microphone["audio"], "AQACAA==", "上传原始样本且不添加WAV头");
    assert_eq!(
        session.receive(output),
        Ok(LiveEvent::OutputAudio(expected)),
        "无需等待音频done也能处理下一段输出"
    );
}

/// AC03：声明16k的会话必须拒绝其他采样率、损坏样本和过量单包。
///
/// 数据构造：24k有效PCM与奇数3字节分别违反采样率、16bit对齐；
/// 1秒16k单声道PCM为32000字节，32002字节超过单包硬上限。
/// 执行过程：ready后逐一提交这些数据，再验证合法20ms输入仍可发送。
/// 预期结果：无效输入全部拒绝，不进入编码；合法输入保持可用。
#[test]
fn microphone_rejects_mismatched_or_unbounded_pcm() {
    let mut session = LiveSession::new(LiveConfig::default());
    session.start_message().expect("应启动");
    session
        .receive(
            r#"{"type":"session.started","session":{"id":"live_format","model":"gpt-live-1"}}"#,
        )
        .expect("应ready");
    let mut pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 24_000, 1, vec![0; 640])
        .expect("24k PCM本身有效但不匹配会话");
    assert!(
        session.audio_message(&pcm).is_err(),
        "不能把24k样本标为16k发送"
    );
    pcm.sample_rate_hz = 16_000;
    pcm.bytes = vec![0; 3];
    assert!(
        session.audio_message(&pcm).is_err(),
        "公开PCM字段损坏后仍须在协议边界拒绝"
    );
    pcm.bytes = vec![0; 32_002];
    assert!(
        session.audio_message(&pcm).is_err(),
        "单包不能超过1秒输入预算"
    );
    pcm.bytes = vec![0; 640];
    assert!(
        session.audio_message(&pcm).is_ok(),
        "合法输入不能因前一次无效包而失效"
    );
    assert!(
        session
            .receive(r#"{"type":"session.output_audio.delta","delta":"???"}"#)
            .is_err(),
        "损坏base64不得交给播放器"
    );
    assert!(
        session
            .receive(r#"{"type":"session.output_audio.delta","delta":"AQ=="}"#)
            .is_err(),
        "输出也必须是完整16bit样本"
    );
}

/// AC11：用量是包含小数秒的累计快照，只有closed确认最终用量。
///
/// 数据构造：先12.5秒、再15.25秒，最终16秒；不是把三次相加。
/// 执行过程：ready后接收两个用量事件，发起close，再接收closed及迟到ready。
/// 预期结果：closing拒绝新音频；closed前不确认最终值；closed后不复活。
#[test]
fn close_stops_new_audio_and_only_closed_finalizes_cumulative_usage() {
    let mut session = LiveSession::new(LiveConfig::default());
    session.start_message().expect("应启动");
    session
        .receive(r#"{"type":"session.started","session":{"id":"live_close","model":"gpt-live-1"}}"#)
        .expect("应ready");
    session
        .receive(r#"{"type":"session.usage.updated","usage":{"seconds":12.5}}"#)
        .expect("应接受小数秒用量");
    session
        .receive(r#"{"type":"session.usage.updated","usage":{"seconds":15.25}}"#)
        .expect("应替换累计快照");
    assert_eq!(
        session.usage(),
        LiveUsage {
            seconds: 15.25,
            finalized: false
        },
        "累计快照不能求和或假报最终值"
    );
    let closing: Value = serde_json::from_str(&session.close_message().expect("应发起优雅关闭"))
        .expect("关闭为JSON");
    assert_eq!(closing["type"], "session.close", "必须使用Live关闭事件");
    let pcm = PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 640]).expect("PCM有效");
    assert_eq!(
        session.audio_message(&pcm),
        Err(LiveError::NotReady),
        "closing不得提交新音频"
    );
    assert!(
        !session.usage().finalized,
        "close命令不能代表服务器finalized"
    );
    assert_eq!(
        session.receive(r#"{"type":"session.output_audio.delta","delta":"AQACAA=="}"#),
        Ok(LiveEvent::Ignored),
        "关闭时仍须消费在途音频以等到closed，但不能重新播放"
    );
    session.receive(r#"{"type":"session.closed","reason":"close_requested","session":{"id":"live_close","model":"gpt-live-1"},"usage":{"seconds":16.0}}"#)
        .expect("应接受最终closed");
    assert_eq!(
        session.usage(),
        LiveUsage {
            seconds: 16.0,
            finalized: true
        },
        "只取最终16秒快照"
    );
    assert!(
        session
            .receive(
                r#"{"type":"session.started","session":{"id":"live_late","model":"gpt-live-1"}}"#
            )
            .is_err(),
        "迟到ready不能复活旧会话"
    );
    assert!(session.audio_message(&pcm).is_err(), "closed后不能再次发送");
}
