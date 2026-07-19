use super::{MiniMaxConnection, MiniMaxTransport, MiniMaxTtsClient, MiniMaxTtsConfig};
use crate::{
    TtsChunkSink, TtsClient, TtsError, TtsErrorKind, TtsModelId, TtsStreamSummary, TtsVoiceId,
    MINIMAX_SPEECH_TURBO_MODEL_ID,
};
use audio::PcmAudioChunk;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingSink {
    chunks: Mutex<Vec<PcmAudioChunk>>,
    terminal: Mutex<Vec<Result<TtsStreamSummary, TtsError>>>,
    cancelled: AtomicBool,
}

impl TtsChunkSink for RecordingSink {
    fn send(&self, chunk: PcmAudioChunk) -> bool {
        self.chunks
            .lock()
            .expect("recording sink chunk lock should not be poisoned")
            .push(chunk);
        true
    }

    fn finish(&self, result: Result<TtsStreamSummary, TtsError>) {
        self.terminal
            .lock()
            .expect("recording sink terminal lock should not be poisoned")
            .push(result);
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

struct FakeConnection {
    received: VecDeque<Result<String, TtsError>>,
    sent: Arc<Mutex<Vec<String>>>,
}

impl MiniMaxConnection for FakeConnection {
    fn send_text(&mut self, text: String) -> Result<(), TtsError> {
        self.sent
            .lock()
            .expect("fake sent-message lock should not be poisoned")
            .push(text);
        Ok(())
    }

    fn receive_text(&mut self) -> Result<Option<String>, TtsError> {
        self.received
            .pop_front()
            .unwrap_or_else(|| {
                Err(TtsError::new(
                    TtsErrorKind::Protocol,
                    "fake transport ran out of frames",
                ))
            })
            .map(Some)
    }

    fn close(&mut self) {}
}

struct FakeTransport {
    connections: Mutex<VecDeque<Result<FakeConnection, TtsError>>>,
    attempts: AtomicUsize,
}

impl FakeTransport {
    fn new(connections: Vec<Result<FakeConnection, TtsError>>) -> Self {
        Self {
            connections: Mutex::new(connections.into()),
            attempts: AtomicUsize::new(0),
        }
    }
}

impl MiniMaxTransport for FakeTransport {
    fn connect(
        &self,
        _api_key: &str,
        _deadline: std::time::Instant,
    ) -> Result<Box<dyn MiniMaxConnection>, TtsError> {
        self.attempts.fetch_add(1, Ordering::AcqRel);
        self.connections
            .lock()
            .expect("fake connection queue lock should not be poisoned")
            .pop_front()
            .unwrap_or_else(|| {
                Err(TtsError::new(
                    TtsErrorKind::Protocol,
                    "fake transport ran out of connections",
                ))
            })
            .map(|connection| Box::new(connection) as Box<dyn MiniMaxConnection>)
    }
}

fn frame(event: &str, body: &str) -> String {
    format!(
        "{{\"event\":\"{event}\",\"session_id\":\"session-safe\",\"trace_id\":\"trace-safe\",\"base_resp\":{{\"status_code\":0,\"status_msg\":\"success\"}}{body}}}"
    )
}

fn continued_frame(body: &str) -> String {
    format!(
        "{{\"session_id\":\"session-safe\",\"trace_id\":\"trace-safe\",\"base_resp\":{{\"status_code\":0,\"status_msg\":\"success\"}}{body}}}"
    )
}

fn successful_connection(
    sent: Arc<Mutex<Vec<String>>>,
    audio_frames: &[(&str, bool, u64)],
) -> FakeConnection {
    let mut received = VecDeque::from([
        Ok(frame("connected_success", "")),
        Ok(frame("task_started", "")),
    ]);
    for (audio, is_final, usage) in audio_frames {
        received.push_back(Ok(continued_frame(
            &format!(
                ",\"data\":{{\"audio\":\"{audio}\"}},\"is_final\":{is_final},\"extra_info\":{{\"audio_sample_rate\":32000,\"audio_format\":\"pcm\",\"audio_channel\":1,\"usage_characters\":{usage}}}"
            ),
        )));
    }
    received.push_back(Ok(frame("task_finished", "")));
    FakeConnection { received, sent }
}

fn config() -> MiniMaxTtsConfig {
    MiniMaxTtsConfig::new(
        TtsModelId::parse(MINIMAX_SPEECH_TURBO_MODEL_ID).expect("Turbo model fixture should parse"),
        TtsVoiceId::parse("voice-fixture").expect("Voice fixture should parse"),
    )
    .expect("supported MiniMax config should be accepted")
}

/// 两个语义段共享一个 MiniMax task，且 PCM 在 task_finished 前到达 sink。
///
/// 数据构造（含关键数值的推导过程）：
///   connections = 1
///   text segments = first(1) + second(1) = 2
///   PCM chunks = 4 bytes + 4 bytes = 8 bytes（PCM16 偶数字节）
///   sent events = start(1) + continue(2) + finish(1) = 4
///
/// 执行过程：
///   1. 建立流并完成 connected_success → task_started。
///   2. 顺序 push 两个文本段，各接收一个 task_continued PCM。
///   3. 在调用 finish 前观察 sink，再发送 task_finish。
///
/// 预期结果：
///   - 正断言：两段共用一个连接/任务，PCM 有序且在完成前可见。
///   - 负断言：不会为第二段建立新连接，也不会提前发送终态。
#[test]
fn reply_stream_reuses_one_task_and_emits_pcm_before_finish() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let transport = Arc::new(FakeTransport::new(vec![Ok(successful_connection(
        Arc::clone(&sent),
        &[("00010203", true, 5), ("04050607", true, 6)],
    ))]));
    let sink = Arc::new(RecordingSink::default());
    let mut client = MiniMaxTtsClient::new_with_transport(
        "test-api-key".to_string(),
        config(),
        transport.clone(),
    );

    client
        .start_stream(sink.clone())
        .expect("stream setup should succeed");
    client
        .push_text("第一段。")
        .expect("first segment should synthesize");
    client
        .push_text("第二段。")
        .expect("second segment should synthesize on the same task");

    let chunks_before_finish = sink
        .chunks
        .lock()
        .expect("recording sink chunk lock should not be poisoned")
        .clone();
    assert_eq!(
        chunks_before_finish.len(),
        2,
        "task_finish 前应已向播放链路交付两个 PCM chunk"
    );
    assert_eq!(
        chunks_before_finish[0].bytes,
        vec![0x00, 0x01, 0x02, 0x03],
        "第一个 PCM chunk 必须保持服务端到达顺序"
    );
    assert_eq!(
        chunks_before_finish[1].bytes,
        vec![0x04, 0x05, 0x06, 0x07],
        "第二个 PCM chunk 必须跟在第一个之后"
    );
    assert!(
        sink.terminal
            .lock()
            .expect("recording sink terminal lock should not be poisoned")
            .is_empty(),
        "finish_stream 前不得提前发出终态"
    );

    client
        .finish_stream()
        .expect("task_finish should complete the stream");

    assert_eq!(
        transport.attempts.load(Ordering::Acquire),
        1,
        "两个文本段必须只建立一个 WebSocket 连接"
    );
    let sent = sent
        .lock()
        .expect("fake sent-message lock should not be poisoned");
    assert_eq!(sent.len(), 4, "一个回复应发送四个协议事件");
    assert!(
        sent[0].contains("\"event\":\"task_start\""),
        "首个事件必须是 task_start"
    );
    assert!(
        sent[1].contains("第一段。") && sent[2].contains("第二段。"),
        "两个 task_continue 必须保持语义段顺序"
    );
    assert!(
        sent[3].contains("\"event\":\"task_finish\""),
        "末个事件必须是 task_finish"
    );
    let terminal = sink
        .terminal
        .lock()
        .expect("recording sink terminal lock should not be poisoned");
    assert_eq!(terminal.len(), 1, "正常结束必须只发送一次终态");
    assert!(terminal[0].is_ok(), "正常结束的唯一终态必须成功");
}

/// task_started 前的可重试连接错误最多重试一次。
///
/// 数据构造（含关键数值的推导过程）：
///   retry budget = 1
///   attempts = initial(1) + retry(1) = 2
///
/// 执行过程：
///   1. 第一次 connect 成功，但等待 connected_success 时返回普通 Transport 错误。
///   2. 第二次 connect 完成 task_started。
///
/// 预期结果：
///   - 正断言：第二次尝试成功建立流。
///   - 负断言：不会进行第三次连接。
#[test]
fn setup_transport_failure_retries_exactly_once_before_task_started() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let failed_handshake = FakeConnection {
        received: VecDeque::from([Err(TtsError::new(
            TtsErrorKind::Transport,
            "temporary handshake failure",
        ))]),
        sent: Arc::clone(&sent),
    };
    let connection = successful_connection(Arc::clone(&sent), &[]);
    let transport = Arc::new(FakeTransport::new(vec![
        Ok(failed_handshake),
        Ok(connection),
    ]));
    let sink = Arc::new(RecordingSink::default());
    let mut client = MiniMaxTtsClient::new_with_transport(
        "test-api-key".to_string(),
        config(),
        transport.clone(),
    );

    client
        .start_stream(sink)
        .expect("single retry should recover setup failure");

    assert_eq!(
        transport.attempts.load(Ordering::Acquire),
        2,
        "setup failure should consume exactly initial attempt plus one retry"
    );
    assert_ne!(
        transport.attempts.load(Ordering::Acquire),
        3,
        "retry budget is one, so a third connection is forbidden"
    );
}

/// task_started 后的连接错误不重连、不重放并产生一次失败终态。
///
/// 数据构造（含关键数值的推导过程）：
///   accepted tasks = 1
///   received PCM = 0 bytes
///   permitted retries after acceptance = 0
///
/// 执行过程：
///   1. 完成 connected_success 与 task_started。
///   2. 首个 task_continue 等待响应时发生 Transport 错误。
///
/// 预期结果：
///   - 正断言：错误标记 task_accepted，并向 sink 发送一次失败终态。
///   - 负断言：连接次数仍为一，不重建 task。
#[test]
fn accepted_task_disconnect_never_retries_or_replays() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let connection = FakeConnection {
        received: VecDeque::from([
            Ok(frame("connected_success", "")),
            Ok(frame("task_started", "")),
            Err(TtsError::new(TtsErrorKind::Transport, "connection lost")),
        ]),
        sent,
    };
    let transport = Arc::new(FakeTransport::new(vec![Ok(connection)]));
    let sink = Arc::new(RecordingSink::default());
    let mut client = MiniMaxTtsClient::new_with_transport(
        "test-api-key".to_string(),
        config(),
        transport.clone(),
    );
    client
        .start_stream(sink.clone())
        .expect("task should reach accepted state");

    let error = client
        .push_text("不能重放。")
        .expect_err("accepted-task disconnect must fail the stream");

    assert!(
        error.task_accepted(),
        "task_started 后的错误必须携带 accepted-task 边界"
    );
    assert!(!error.retryable(), "accepted-task 错误不得标记为可重试");
    assert_eq!(
        transport.attempts.load(Ordering::Acquire),
        1,
        "task_started 后断连不得建立第二个连接"
    );
    let terminal = sink
        .terminal
        .lock()
        .expect("recording sink terminal lock should not be poisoned");
    assert_eq!(terminal.len(), 1, "accepted-task 失败应只发送一次终态");
    assert!(terminal[0].is_err(), "accepted-task 失败终态不能伪装成成功");
}

/// 鉴权失败属于明确的非重试错误。
///
/// 数据构造（含关键数值的推导过程）：
///   provider status_code = 1004（MiniMax 鉴权失败）
///   retry budget consumed = 0
///
/// 执行过程：
///   1. 连接返回带 status_code=1004 的 connected_success 帧。
///   2. start_stream 通过公共错误类型返回失败。
///
/// 预期结果：
///   - 正断言：错误分类为 Authentication。
///   - 负断言：不发送 task_start，也不建立第二个连接。
#[test]
fn authentication_status_is_not_retried() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let auth_frame = "{\"event\":\"connected_success\",\"base_resp\":{\"status_code\":1004,\"status_msg\":\"invalid token\"}}";
    let connection = FakeConnection {
        received: VecDeque::from([Ok(auth_frame.to_string())]),
        sent: Arc::clone(&sent),
    };
    let transport = Arc::new(FakeTransport::new(vec![Ok(connection)]));
    let sink = Arc::new(RecordingSink::default());
    let mut client = MiniMaxTtsClient::new_with_transport(
        "test-api-key".to_string(),
        config(),
        transport.clone(),
    );

    let error = client
        .start_stream(sink)
        .expect_err("provider status 1004 must reject setup");

    assert_eq!(
        error.kind(),
        TtsErrorKind::Authentication,
        "MiniMax status 1004 必须映射为 Authentication"
    );
    assert!(!error.retryable(), "鉴权错误不得标记为可重试");
    assert_eq!(
        transport.attempts.load(Ordering::Acquire),
        1,
        "鉴权错误不得建立第二个连接"
    );
    assert!(
        sent.lock()
            .expect("fake sent-message lock should not be poisoned")
            .is_empty(),
        "鉴权帧失败后不得继续发送 task_start"
    );
}

/// 单个 task_continue 的多个音频帧必须全部交付，`data: null` 不产生空 chunk。
///
/// 数据构造（含关键数值的推导过程）：
///   protocol frames = null-data(1) + metadata-free partial-audio(1) + final-audio(1) = 3
///   playable chunks = 0 + 2 bytes + 2 bytes = 2 chunks / 4 bytes
///
/// 执行过程：
///   1. 完成 task_started 后发送一个文本段。
///   2. 依次接收无音频帧、`is_final=false` 音频和 `is_final=true` 音频。
///   3. 在 push_text 返回后检查 sink。
///
/// 预期结果：
///   - 正断言：两个有效 PCM chunk 均按顺序到达。
///   - 负断言：`data: null` 不会创建第三个空 chunk，也不会提前结束读取。
#[test]
fn one_continue_delivers_all_audio_frames_and_ignores_null_data() {
    let sent = Arc::new(Mutex::new(Vec::new()));
    let null_data = frame(
        "task_continued",
        ",\"data\":null,\"is_final\":false,\"extra_info\":{\"usage_characters\":0}",
    );
    let partial_audio = continued_frame(",\"data\":{\"audio\":\"0001\"},\"is_final\":false");
    let final_audio = frame(
        "task_continued",
        ",\"data\":{\"audio\":\"0203\"},\"is_final\":true,\"extra_info\":{\"audio_sample_rate\":32000,\"audio_format\":\"pcm\",\"audio_channel\":1,\"usage_characters\":2}",
    );
    let connection = FakeConnection {
        received: VecDeque::from([
            Ok(frame("connected_success", "")),
            Ok(frame("task_started", "")),
            Ok(null_data),
            Ok(partial_audio),
            Ok(final_audio),
        ]),
        sent,
    };
    let transport = Arc::new(FakeTransport::new(vec![Ok(connection)]));
    let sink = Arc::new(RecordingSink::default());
    let mut client =
        MiniMaxTtsClient::new_with_transport("test-api-key".to_string(), config(), transport);
    client
        .start_stream(sink.clone())
        .expect("task setup should succeed");

    client
        .push_text("一个分段。")
        .expect("all frames for one task_continue should be consumed");

    let chunks = sink
        .chunks
        .lock()
        .expect("recording sink chunk lock should not be poisoned");
    assert_eq!(
        chunks.len(),
        2,
        "null data 应被忽略且两个有效 chunk 都应交付"
    );
    assert_eq!(
        chunks[0].bytes,
        vec![0x00, 0x01],
        "partial 音频必须作为第一个 chunk 到达"
    );
    assert_eq!(
        chunks[1].bytes,
        vec![0x02, 0x03],
        "final 音频必须作为第二个 chunk 到达"
    );
    assert!(
        chunks.iter().all(|chunk| !chunk.bytes.is_empty()),
        "data: null 不得产生空 PCM chunk"
    );
}
