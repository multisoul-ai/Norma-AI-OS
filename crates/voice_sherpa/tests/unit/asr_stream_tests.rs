use super::PipelineAsrStreamClient;
use asr::{AsrError, AsrProviderCapabilities, Recognizer, RecognizerFactory};
use audio::{PcmAudioChunk, PcmFormat};
use dialogue::{AsrClient, AsrEvent, AsrInputEndReason};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct FakeFactory {
    capabilities: AsrProviderCapabilities,
    final_result: Result<Option<String>, AsrError>,
    creations: Arc<AtomicUsize>,
}

impl RecognizerFactory for FakeFactory {
    fn capabilities(&self) -> AsrProviderCapabilities {
        self.capabilities
    }

    fn create(&self) -> Result<Box<dyn Recognizer>, AsrError> {
        self.creations.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(FakeRecognizer {
            final_result: self.final_result.clone(),
        }))
    }
}

struct FakeRecognizer {
    final_result: Result<Option<String>, AsrError>,
}

struct CancellationCapturingFactory {
    cancellations: Arc<Mutex<Vec<asr::RecognitionCancellation>>>,
}

impl RecognizerFactory for CancellationCapturingFactory {
    fn capabilities(&self) -> AsrProviderCapabilities {
        AsrProviderCapabilities::new(false, None)
    }

    fn create(&self) -> Result<Box<dyn Recognizer>, AsrError> {
        Ok(Box::new(CancellationCapturingRecognizer {
            cancellations: self.cancellations.clone(),
        }))
    }
}

struct CancellationCapturingRecognizer {
    cancellations: Arc<Mutex<Vec<asr::RecognitionCancellation>>>,
}

impl Recognizer for CancellationCapturingRecognizer {
    fn accept_pcm(&mut self, _chunk: &PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        Ok(Vec::new())
    }

    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        Ok(None)
    }

    fn set_cancellation_token(&mut self, cancellation: asr::RecognitionCancellation) {
        self.cancellations
            .lock()
            .expect("cancellation capture 锁不应中毒")
            .push(cancellation);
    }
}

impl Recognizer for FakeRecognizer {
    fn accept_pcm(&mut self, _chunk: &PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        Ok(Vec::new())
    }

    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        self.final_result.clone()
    }
}

fn pcm_for_seconds(seconds: usize) -> PcmAudioChunk {
    // bytes = 16,000 samples/s × 2 bytes/sample × seconds
    let bytes = 16_000 * 2 * seconds;
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; bytes])
        .expect("16kHz mono PCM16 应构造成功")
}

/// PASR-1：Continuous 适配器必须通过公共 RecognitionPipeline 执行时长上限。
///
/// 数据构造（含关键数值的推导过程）：
///   max duration = 1 s
///   input PCM = 2 s = 16,000 × 2 × 2 = 64,000 bytes
///   accepted PCM = 1 s = 32,000 bytes（其余由 pipeline 截断）
///   fake final = “统一管线结果”
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用只声明 1 秒上限的 fake factory 构造 continuous adapter
///   2. 一次推送 2 秒 PCM，触发公共 pipeline 自动 Finish(DurationLimit)
///   3. 再推送一秒 PCM，验证当前 utterance 已关闭
///
/// 预期结果：
///   - 正断言：首次事件含 DurationLimit 与唯一 Final
///   - 负断言：上限后的 PCM 不产生第二个 Final，factory 也不被隐式重建
#[test]
fn continuous_adapter_uses_shared_pipeline_duration_limit() {
    let creations = Arc::new(AtomicUsize::new(0));
    let factory = FakeFactory {
        capabilities: AsrProviderCapabilities::new(false, Some(1)),
        final_result: Ok(Some("统一管线结果".to_string())),
        creations: creations.clone(),
    };
    let mut adapter = PipelineAsrStreamClient::new(Box::new(factory))
        .expect("fake factory 应创建 continuous adapter");

    let events = adapter
        .push_pcm(&pcm_for_seconds(2))
        .expect("跨越上限的 PCM 应由 pipeline 截断并自动提交");
    let after_limit = adapter
        .push_pcm(&pcm_for_seconds(1))
        .expect("上限后的 PCM 应被安全忽略而不是污染下一轮");

    assert_eq!(
        events,
        vec![
            AsrEvent::InputEnded(AsrInputEndReason::DurationLimit),
            AsrEvent::Final("统一管线结果".to_string()),
        ],
        "continuous adapter 必须透传公共 pipeline 的上限结束与唯一 Final"
    );
    assert!(after_limit.is_empty(), "上限后的 PCM 不得产生第二个终态");
    assert_eq!(
        creations.load(Ordering::SeqCst),
        1,
        "同一 utterance 达到上限后不得隐式创建第二个 recognizer"
    );
}

/// PASR-2：Continuous 供应商错误必须从公共 pipeline 返回给上层，而不是只写日志。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM = 1 s = 32,000 bytes
///   finish reason = Vad
///   provider result = AsrError::Provider("fake auth failure")
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造无时长上限但 finish 必然失败的 fake recognizer
///   2. 推送 1 秒 PCM
///   3. 模拟 VAD SpeechEnd 调用 end_utterance
///
/// 预期结果：
///   - 正断言：错误文本明确包含 fake auth failure
///   - 负断言：失败不得被映射为空 transcript 或成功 Final
#[test]
fn continuous_adapter_returns_provider_failure_to_event_loop() {
    let creations = Arc::new(AtomicUsize::new(0));
    let factory = FakeFactory {
        capabilities: AsrProviderCapabilities::new(false, None),
        final_result: Err(AsrError::Provider("fake auth failure".to_string())),
        creations,
    };
    let mut adapter = PipelineAsrStreamClient::new(Box::new(factory))
        .expect("fake factory 应创建 continuous adapter");

    let partials = adapter
        .push_pcm(&pcm_for_seconds(1))
        .expect("供应商应先接受 utterance PCM");
    let error = adapter
        .end_utterance()
        .expect_err("供应商 finish 失败必须返回上层")
        .to_string();

    assert!(
        partials.is_empty(),
        "不支持 partial 的 fake provider 不得伪造文本"
    );
    assert!(
        error.contains("fake auth failure"),
        "continuous 错误必须保留可向用户展示的供应商失败原因"
    );
    assert!(
        !error.trim().is_empty(),
        "供应商失败不得退化为空 transcript"
    );
}

/// PASR-3：Continuous 重置分段后，外部稳定取消句柄仍必须取消当前 recognizer。
///
/// 数据构造（含关键数值的推导过程）：
///   initial sessions = 1
///   reset count = 1
///   total recognizer tokens = 1 + 1 = 2
///   external cancellation handles = 1（整个 Continuous worker 生命周期稳定复用）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 adapter，捕获第一段 recognizer 的 child token
///   2. 保存 worker 对外暴露的稳定父 cancellation
///   3. reset 创建第二段 recognizer，并捕获新的 child token
///   4. 调用旧有的外部句柄 cancel，父 token 级联取消当前 child token
///
/// 预期结果：
///   - 正断言：reset 后确实创建第二个、不同的当前分段 token
///   - 正断言：外部 cancel 后当前第二段 token 进入 cancelled
///   - 负断言：当前第二段 token 不得因 reset 与外部句柄失联
#[test]
fn continuous_adapter_cancels_current_recognizer_after_reset() {
    let cancellations = Arc::new(Mutex::new(Vec::new()));
    let factory = CancellationCapturingFactory {
        cancellations: cancellations.clone(),
    };
    let mut adapter = PipelineAsrStreamClient::new(Box::new(factory))
        .expect("捕获取消 token 的 factory 应创建 continuous adapter");
    let worker_cancellation = adapter.cancellation();

    adapter.reset();

    let captured = cancellations
        .lock()
        .expect("cancellation capture 锁不应中毒")
        .clone();
    assert_eq!(
        captured.len(),
        2,
        "初始分段加一次 reset 应恰好创建两个 recognizer token"
    );
    assert!(
        !captured[1].is_cancelled(),
        "调用外部 cancel 前，当前第二段 recognizer 不应提前取消"
    );

    worker_cancellation.cancel();

    assert!(
        captured[1].is_cancelled(),
        "reset 后的当前 recognizer 必须仍受 worker 稳定取消句柄控制"
    );
}
