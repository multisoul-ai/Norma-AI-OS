use live_voice::{LiveConfig, LiveEvent, LiveSession, Speaker, TranscriptFragment};

/// AC04：用户与助手的重叠字幕保持独立来源、空格和近似时轴。
///
/// 数据构造：用户1000.5–1200.5ms、助手1100–1150ms，最后用户片段迟到。
/// 执行过程：ready后按到达顺序交错接收三个事件，分别观察公开事件。
/// 预期结果：助手附和不会吞掉用户后续输入，文本、说话方和小数时戳不变。
#[test]
fn interleaved_transcript_fragments_preserve_each_speaker_and_timing() {
    let mut session = LiveSession::new(LiveConfig::default());
    session.start_message().expect("应启动");
    session
        .receive(
            r#"{"type":"session.started","session":{"id":"live_captions","model":"gpt-live-1"}}"#,
        )
        .expect("应ready");
    let user = session.receive(r#"{"type":"session.input_transcript.delta","event_id":"u1","delta":"让 Simon","start_ms":1000.5,"end_ms":1200.5}"#).expect("用户字幕有效");
    assert_eq!(
        user,
        LiveEvent::Transcript(TranscriptFragment {
            event_id: "u1".into(),
            speaker: Speaker::User,
            delta: "让 Simon".into(),
            start_ms: 1000.5,
            end_ms: 1200.5
        }),
        "用户转写必须保留来源和近似时轴"
    );
    let assistant = session.receive(r#"{"type":"session.output_transcript.delta","event_id":"a1","delta":"好，","start_ms":1100,"end_ms":1150}"#).expect("助手字幕有效");
    assert!(
        matches!(
            assistant,
            LiveEvent::Transcript(TranscriptFragment {
                speaker: Speaker::Assistant,
                ..
            })
        ),
        "同时说话不能混淆说话方"
    );
    let late = session.receive(r#"{"type":"session.input_transcript.delta","event_id":"u2","delta":" 看登录模块","start_ms":1200.5,"end_ms":1500}"#).expect("迟到用户字幕有效");
    assert_eq!(
        late,
        LiveEvent::Transcript(TranscriptFragment {
            event_id: "u2".into(),
            speaker: Speaker::User,
            delta: " 看登录模块".into(),
            start_ms: 1200.5,
            end_ms: 1500.0
        }),
        "助手附和后仍保留用户字幕及前导空格"
    );
    assert!(session.receive(r#"{"type":"session.input_transcript.delta","event_id":"bad","delta":"x","start_ms":2000,"end_ms":1000}"#).is_err(), "倒置时轴不能进入上下文");
}

/// AC12：命令拒绝与断线分开，服务端原始message不得进入可展示事件。
///
/// 数据构造：错误message含测试canary，code及client_event_id是可关联的机器字段。
/// 执行过程：ready后收错误，再发送一包合法PCM。
/// 预期结果：返回拒绝事件并保留关联ID；不传播原始message，也不擅自结束会话。
#[test]
fn rejected_command_keeps_correlation_without_exposing_raw_message() {
    let mut session = LiveSession::new(LiveConfig::default());
    session.start_message().expect("应启动");
    session
        .receive(r#"{"type":"session.started","session":{"id":"live_error","model":"gpt-live-1"}}"#)
        .expect("应ready");
    let event = session.receive(r#"{"type":"error","error":{"code":"immutable_field_update","client_event_id":"change_1","message":"private-canary"}}"#).expect("命令错误不是无效JSON");
    assert_eq!(
        event,
        LiveEvent::Rejected {
            code: Some("immutable_field_update".into()),
            client_event_id: Some("change_1".into())
        },
        "只暴露稳定机器字段及关联ID"
    );
    assert!(
        !format!("{event:?}").contains("private-canary"),
        "原始上游message不能漏进诊断事件"
    );
    let pcm = audio::PcmAudioChunk::new(audio::PcmFormat::Pcm16, 16_000, 1, vec![0; 640])
        .expect("PCM有效");
    assert!(
        session.audio_message(&pcm).is_ok(),
        "单个命令拒绝不能伪装成会话已断开"
    );
}
