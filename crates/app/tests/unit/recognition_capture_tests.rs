use super::AsrRecordingSession;
use asr::{
    AsrError, AsrProviderCapabilities, RecognitionCancellation, Recognizer, RecognizerFactory,
};
use audio::{PcmAudioChunk, PcmFormat};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

static FIXTURE_LOCK: Mutex<()> = Mutex::new(());
struct Factory(Arc<AtomicUsize>, Option<u64>);
struct TruncatedRecognizer(Arc<AtomicUsize>);

impl RecognizerFactory for Factory {
    fn capabilities(&self) -> AsrProviderCapabilities {
        AsrProviderCapabilities::new(true, self.1)
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
    let _fixture = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let finished = Arc::new(AtomicUsize::new(0));
    voice_input::install_test_pcm_stream(vec![PcmAudioChunk::new(
        PcmFormat::Pcm16,
        16_000,
        1,
        vec![0; 3_200],
    )
    .expect("100ms录音夹具必须合法")]);
    let (session, partials) = AsrRecordingSession::start(
        None,
        &Factory(Arc::clone(&finished), None),
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
    wait_until(|| partials.is_closed());
    assert!(
        partials.is_closed(),
        "调用finish前就必须发布终态并关闭partial事件，不能继续假装正在录音"
    );
    assert!(
        session.input_has_ended(),
        "App的输入结束监控必须能看到意外断流"
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

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !predicate() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn pcm(frames: usize) -> PcmAudioChunk {
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; frames * 2])
        .expect("16kHz单声道每帧两字节的夹具应合法")
}

/// 场景：用户主动停止保活的输入，不能被EOF处理误判为设备断流。
/// 构造：1600帧/16kHz=100ms，夹具发送后继续保活；先等到真实partial见证。
/// 执行：调用生产finish；预期正常文本、识别器finish恰好一次、采集生命周期已关闭。
#[test]
fn a_user_stop_preserves_normal_finalization_and_closes_capture() {
    let _fixture = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let finished = Arc::new(AtomicUsize::new(0));
    let lifecycle = voice_input::install_test_duplex_pcm_stream(vec![pcm(1_600)]);
    let (session, partials) = AsrRecordingSession::start(
        None,
        &Factory(Arc::clone(&finished), None),
        RecognitionCancellation::new(),
    )
    .expect("保活输入应成功启动");
    lifecycle
        .try_recv()
        .expect("必须见证生产入口已经取得采集夹具");
    wait_until(|| !partials.is_empty());
    partials
        .try_recv()
        .expect("主动停止前必须有输入到达真实识别器的partial见证");
    assert!(
        !session.input_has_ended(),
        "保活输入在用户停止前不能提前进入终态"
    );
    assert_eq!(
        session.finish(),
        Ok("truncated final must not succeed".into()),
        "正常用户停止必须保留识别器给出的完整final"
    );
    assert_eq!(
        finished.load(Ordering::SeqCst),
        1,
        "用户停止只应调用一次正常finish"
    );
    assert!(lifecycle.is_closed(), "finish返回前必须收口PCM采集生命周期");
}

/// 场景：达到提供商时长上限后输入EOF，不能覆盖已排队的正常DurationLimit完成。
/// 构造：上限1秒，16000帧×2字节正好为16kHz下一秒输入，发送后关闭源。
/// 执行：等待真实限时状态和事件收口，再finish；预期正常final且只finish一次。
#[test]
fn a_duration_limit_is_not_cancelled_by_the_following_pcm_eof() {
    let _fixture = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let finished = Arc::new(AtomicUsize::new(0));
    voice_input::install_test_pcm_stream(vec![pcm(16_000)]);
    let (session, partials) = AsrRecordingSession::start(
        None,
        &Factory(Arc::clone(&finished), Some(1)),
        RecognitionCancellation::new(),
    )
    .expect("一秒限时夹具应启动");
    wait_until(|| session.reached_duration_limit() && partials.is_closed());
    assert!(
        session.reached_duration_limit(),
        "16000帧必须实际触发一秒限时"
    );
    assert!(session.input_has_ended(), "App监控应看到正常限时终态");
    assert_eq!(
        session.finish(),
        Ok("truncated final must not succeed".into()),
        "后续EOF不能取消正常限时final"
    );
    assert_eq!(
        finished.load(Ordering::SeqCst),
        1,
        "DurationLimit只应调用一次正常finish"
    );
}

/// 场景：主动取消必须关闭输入与事件，不得被EOF重新finalize。
/// 构造：100ms保活输入，先等首个partial；执行生产cancel并等待后台收口。
/// 预期：PCM和partial生命周期均关闭，识别器正常finish调用数为零。
#[test]
fn cancelling_capture_does_not_finalize_partial_audio() {
    let _fixture = FIXTURE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let finished = Arc::new(AtomicUsize::new(0));
    let lifecycle = voice_input::install_test_duplex_pcm_stream(vec![pcm(1_600)]);
    let (session, partials) = AsrRecordingSession::start(
        None,
        &Factory(Arc::clone(&finished), None),
        RecognitionCancellation::new(),
    )
    .expect("取消夹具应启动");
    lifecycle.try_recv().expect("取消前必须实际取得输入夹具");
    wait_until(|| !partials.is_empty());
    partials.try_recv().expect("取消前必须有真实PCM处理见证");
    session.cancel();
    wait_until(|| lifecycle.is_closed() && partials.is_closed());
    assert!(lifecycle.is_closed(), "取消必须释放保活的采集资源");
    assert!(partials.is_closed(), "取消必须发布终态并关闭事件流");
    assert_eq!(
        finished.load(Ordering::SeqCst),
        0,
        "取消不能对截断输入调用正常finish"
    );
}
