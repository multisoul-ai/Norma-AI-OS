use super::{MiniMaxConnection, MiniMaxTransport, MiniMaxTtsClient, MiniMaxTtsConfig};
use crate::{
    TtsChunkSink, TtsClient, TtsError, TtsErrorKind, TtsModelId, TtsStreamSummary, TtsVoiceId,
    MINIMAX_SPEECH_TURBO_MODEL_ID,
};
use audio::PcmAudioChunk;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct FailureSink {
    chunks: Mutex<Vec<PcmAudioChunk>>,
    terminal: Mutex<Vec<Result<TtsStreamSummary, TtsError>>>,
}

impl TtsChunkSink for FailureSink {
    fn send(&self, chunk: PcmAudioChunk) -> bool {
        self.chunks
            .lock()
            .expect("chunk lock should be valid")
            .push(chunk);
        true
    }

    fn finish(&self, result: Result<TtsStreamSummary, TtsError>) {
        self.terminal
            .lock()
            .expect("terminal lock should be valid")
            .push(result);
    }

    fn is_cancelled(&self) -> bool {
        false
    }
}

struct FailureConnection {
    frames: VecDeque<String>,
}

impl MiniMaxConnection for FailureConnection {
    fn send_text(&mut self, _text: String) -> Result<(), TtsError> {
        Ok(())
    }

    fn receive_text(&mut self) -> Result<Option<String>, TtsError> {
        self.frames.pop_front().map(Some).ok_or_else(|| {
            TtsError::new(TtsErrorKind::Protocol, "failure fixture exhausted frames")
        })
    }

    fn close(&mut self) {}
}

struct FailureTransport(Mutex<Option<FailureConnection>>);

impl MiniMaxTransport for FailureTransport {
    fn connect(
        &self,
        _api_key: &str,
        _deadline: std::time::Instant,
    ) -> Result<Box<dyn MiniMaxConnection>, TtsError> {
        self.0
            .lock()
            .expect("transport lock should be valid")
            .take()
            .map(|connection| Box::new(connection) as Box<dyn MiniMaxConnection>)
            .ok_or_else(|| TtsError::new(TtsErrorKind::Protocol, "unexpected second connection"))
    }
}

fn client_with_frames(frames: Vec<String>) -> MiniMaxTtsClient {
    let config = MiniMaxTtsConfig::new(
        TtsModelId::parse(MINIMAX_SPEECH_TURBO_MODEL_ID).expect("model fixture should parse"),
        TtsVoiceId::parse("voice-fixture").expect("Voice fixture should parse"),
    )
    .expect("MiniMax fixture config should be supported");
    MiniMaxTtsClient::new_with_transport(
        "test-key".to_string(),
        config,
        Arc::new(FailureTransport(Mutex::new(Some(FailureConnection {
            frames: frames.into(),
        })))),
    )
}

fn success_event(event: &str) -> String {
    format!("{{\"event\":\"{event}\",\"base_resp\":{{\"status_code\":0}}}}")
}

/// 空音频任务不能被当作一次成功的流式合成。
///
/// 数据构造（含关键数值的推导过程）：
///   text segments = 1
///   playable PCM chunks = 0
///   terminal frames = continued(1) + finished(1) = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完成 connected_success 与 task_started。
///   2. 提交一个文本段，只收到无 audio 的 final continue。
///   3. 收到 task_finished 后结束流。
///
/// 预期结果：
///   - 正断言：结束返回 AudioDecode，sink 收到一次失败终态。
///   - 负断言：sink 不产生 PCM，任务不能伪装为成功。
#[test]
fn task_finished_without_pcm_fails_safely() {
    let frames = vec![
        success_event("connected_success"),
        success_event("task_started"),
        "{\"data\":null,\"is_final\":true,\"base_resp\":{\"status_code\":0}}".to_string(),
        success_event("task_finished"),
    ];
    let sink = Arc::new(FailureSink::default());
    let mut client = client_with_frames(frames);
    client
        .start_stream(sink.clone())
        .expect("setup should succeed");
    client
        .push_text("必须有音频。")
        .expect("empty continue should complete the segment");

    let error = client
        .finish_stream()
        .expect_err("zero-PCM task must fail at finish");

    assert_eq!(
        error.kind(),
        TtsErrorKind::AudioDecode,
        "零 PCM 的完成任务必须映射为 AudioDecode"
    );
    assert!(
        sink.chunks
            .lock()
            .expect("chunk lock should be valid")
            .is_empty(),
        "空音频响应不得创建播放 chunk"
    );
    let terminal = sink.terminal.lock().expect("terminal lock should be valid");
    assert_eq!(terminal.len(), 1, "空音频任务必须精确发送一次失败终态");
    assert!(terminal[0].is_err(), "空音频任务的终态不得伪装为成功");
}

/// 官方错误码必须映射为可操作的稳定错误类别。
///
/// 数据构造（含关键数值的推导过程）：
///   status codes = auth(2049) + balance(1008) + invalid Voice(20132) + rate limit(2056) + server(1024)
///   expected classes = 5
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别解析五个只有 base_resp 的响应帧。
///   2. 读取每个公共 TtsErrorKind。
///   3. 读取余额不足错误的用户可见消息与重试标记。
///
/// 预期结果：
///   - 正断言：五个错误精确映射到 Authentication、QuotaExceeded、InvalidParameter、RateLimit、Transport。
///   - 正断言：余额不足消息明确建议充值或切换本地音色。
///   - 负断言：无效 API Key 和余额不足不得落为 Protocol；余额不足不得自动重试。
#[test]
fn official_status_codes_map_to_actionable_error_kinds() {
    fn kind(code: i64) -> TtsErrorKind {
        super::protocol::parse_incoming(&format!("{{\"base_resp\":{{\"status_code\":{code}}}}}"))
            .expect_err("non-zero provider status must fail")
            .kind()
    }

    assert_eq!(
        kind(2049),
        TtsErrorKind::Authentication,
        "2049 必须表示无效 API Key"
    );
    let balance_error = super::protocol::parse_incoming(
        "{\"base_resp\":{\"status_code\":1008,\"status_msg\":\"insufficient balance\"}}",
    )
    .expect_err("1008 余额不足必须终止当前 MiniMax 请求");
    assert_eq!(
        balance_error.kind(),
        TtsErrorKind::QuotaExceeded,
        "1008 必须表示账户额度不足"
    );
    assert_eq!(
        balance_error.to_string(),
        "MiniMax account balance is insufficient. Add balance or switch to an On-device voice.",
        "1008 必须给出充值或切换本地音色的可操作提示"
    );
    assert!(
        !balance_error.retryable(),
        "余额不足不会因立即重试恢复，任务不得自动重试"
    );
    assert_eq!(
        kind(20132),
        TtsErrorKind::InvalidParameter,
        "20132 必须表示无效 Voice ID"
    );
    assert_eq!(
        kind(2056),
        TtsErrorKind::RateLimit,
        "2056 必须表示资源或速率限制"
    );
    assert_eq!(
        kind(1024),
        TtsErrorKind::Transport,
        "1024 服务错误必须允许接收前重试"
    );
    assert_ne!(
        kind(2049),
        TtsErrorKind::Protocol,
        "无效 API Key 不得显示为临时协议错误"
    );
    assert_ne!(
        balance_error.kind(),
        TtsErrorKind::Protocol,
        "余额不足不得显示为协议错误"
    );
}

struct FinishSetupFailureConnection {
    frames: VecDeque<String>,
}

impl MiniMaxConnection for FinishSetupFailureConnection {
    fn finish_setup(&mut self) -> Result<(), TtsError> {
        Err(TtsError::new(
            TtsErrorKind::Transport,
            "could not restore runtime socket timeout",
        ))
    }

    fn send_text(&mut self, _text: String) -> Result<(), TtsError> {
        Ok(())
    }

    fn receive_text(&mut self) -> Result<Option<String>, TtsError> {
        self.frames
            .pop_front()
            .map(Some)
            .ok_or_else(|| TtsError::new(TtsErrorKind::Protocol, "fixture exhausted frames"))
    }

    fn close(&mut self) {}
}

struct FinishSetupFailureTransport {
    attempts: AtomicUsize,
}

impl MiniMaxTransport for FinishSetupFailureTransport {
    fn connect(
        &self,
        _api_key: &str,
        _deadline: std::time::Instant,
    ) -> Result<Box<dyn MiniMaxConnection>, TtsError> {
        self.attempts.fetch_add(1, Ordering::AcqRel);
        Ok(Box::new(FinishSetupFailureConnection {
            frames: VecDeque::from([
                success_event("connected_success"),
                success_event("task_started"),
            ]),
        }))
    }
}

/// task_started 后恢复运行期 timeout 失败也不得重建或重放任务。
///
/// 数据构造（含关键数值的推导过程）：
///   accepted tasks = 1
///   finish_setup failures = 1
///   permitted retries after acceptance = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 完成 connected_success，发送 task_start 并收到 task_started。
///   2. 模拟恢复正常 socket timeout 时发生 Transport 错误。
///   3. 读取公共错误边界与连接次数。
///
/// 预期结果：
///   - 正断言：错误标记 task_accepted，连接次数保持为一。
///   - 负断言：不得消费 setup retry 建立第二个已计费 task。
#[test]
fn finish_setup_failure_after_task_started_never_retries() {
    let transport = Arc::new(FinishSetupFailureTransport {
        attempts: AtomicUsize::new(0),
    });
    let sink = Arc::new(FailureSink::default());
    let config = MiniMaxTtsConfig::new(
        TtsModelId::parse(MINIMAX_SPEECH_TURBO_MODEL_ID).expect("model fixture should parse"),
        TtsVoiceId::parse("voice-fixture").expect("Voice fixture should parse"),
    )
    .expect("MiniMax fixture config should be supported");
    let mut client =
        MiniMaxTtsClient::new_with_transport("test-key".to_string(), config, transport.clone());

    let error = client
        .start_stream(sink)
        .expect_err("post-acceptance timeout configuration failure must end setup");

    assert!(
        error.task_accepted(),
        "task_started 后的错误必须标记 accepted 边界"
    );
    assert!(!error.retryable(), "accepted-task 错误不得继续标记为可重试");
    assert_eq!(
        transport.attempts.load(Ordering::Acquire),
        1,
        "task_started 后恢复 timeout 失败不得建立第二个连接"
    );
    assert_ne!(
        transport.attempts.load(Ordering::Acquire),
        2,
        "接受任务后的失败不得消费 setup retry"
    );
}
