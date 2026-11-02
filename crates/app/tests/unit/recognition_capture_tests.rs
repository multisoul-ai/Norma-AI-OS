use super::AsrRecordingSession;
use asr::{
    AsrError, AsrProviderCapabilities, RecognitionCancellation, Recognizer, RecognizerFactory,
};
use audio::{PcmAudioChunk, PcmFormat};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

struct Factory(Arc<AtomicUsize>);
struct TruncatedRecognizer(Arc<AtomicUsize>);

impl RecognizerFactory for Factory {
    fn capabilities(&self) -> AsrProviderCapabilities {
        AsrProviderCapabilities::new(true, None)
    }
    fn create(&self) -> Result<Box<dyn Recognizer>, AsrError> {
        Ok(Box::new(TruncatedRecognizer(Arc::clone(&self.0))))
    }
}

impl Recognizer for TruncatedRecognizer {
    fn accept_pcm(&mut self, _: &PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        Ok(vec!["unfinished utterance".into()])
    }
    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(Some("truncated final must not succeed".into()))
    }
}

/// 场景：用户未停止时PCM源已断流，不能把截断录音按User正常finalize。
/// 构造：生产PCM测试入口发送一块100ms音频后关闭，识别器若finalize会返回非空截断文本。
/// 执行：真实AsrRecordingSession启动，等待PCM转发线程退出，再调用finish收取终态。
/// 预期：明确报输入断流错误，不调用识别器正常finish，不能返回截断文本成功。
#[test]
fn an_unexpected_capture_eof_cannot_finalize_a_truncated_transcript() {
    let finished = Arc::new(AtomicUsize::new(0));
    voice_input::install_test_pcm_stream(vec![PcmAudioChunk::new(
        PcmFormat::Pcm16,
        16_000,
        1,
        vec![0; 3_200],
    )
    .expect("100ms录音夹具必须合法")]);
    let (session, _partials) = AsrRecordingSession::start(
        None,
        &Factory(Arc::clone(&finished)),
        RecognitionCancellation::new(),
    )
    .expect("受控PCM输入应能启动真实ASR会话");
    let forward = session
        .forward_thread
        .as_ref()
        .expect("生产ASR必须有PCM转发线程");
    let deadline = Instant::now() + Duration::from_secs(3);
    while !forward.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(
        forward.is_finished(),
        "受控PCM EOF必须到达真实转发线程，不能挂起"
    );
    assert_eq!(
        session.finish(),
        Err("microphone PCM stream ended unexpectedly".into()),
        "意外输入结束必须向调用方失败，不能返回截断文本"
    );
    assert_eq!(
        finished.load(Ordering::SeqCst),
        0,
        "意外断流不能走用户正常完成识别器路径"
    );
}
