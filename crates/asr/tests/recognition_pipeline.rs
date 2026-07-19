use asr::{
    AsrError, AsrProviderCapabilities, InputEndReason, RecognitionEvent, RecognitionPipeline,
    Recognizer,
};
use audio::{PcmAudioChunk, PcmFormat};
use std::sync::Mutex;

struct ScriptedRecognizer {
    accepted_bytes: usize,
    partial: Option<String>,
    final_text: Option<String>,
}

impl Recognizer for ScriptedRecognizer {
    fn accept_pcm(&mut self, chunk: &PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        self.accepted_bytes += chunk.bytes.len();
        Ok(self.partial.take().into_iter().collect())
    }

    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        Ok(self.final_text.take())
    }
}

fn pcm_with_duration_ms(duration_ms: usize) -> PcmAudioChunk {
    // bytes = 16,000 samples/s × 2 bytes/sample × duration_ms / 1,000 ms/s
    let byte_count = 16_000 * 2 * duration_ms / 1_000;
    PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; byte_count])
        .expect("16 kHz 单声道偶数字节 PCM 应构造成功")
}

/// RP-1：支持 partial 的供应商应在用户结束输入后产生恰好一个 Final 终态。
///
/// 数据构造（含关键数值的推导过程）：
///   PCM 时长 = 20 ms
///   PCM bytes = 16,000 samples/s × 2 bytes/sample × 20/1,000 = 640 bytes
///   partial = “打开”
///   final = “打开 Java 项目”
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动支持 partial、无时长上限的识别会话
///   2. 推送 640 bytes PCM → recognizer 返回一个 partial
///   3. 以 User 原因结束输入 → pipeline flush final
///   4. 读取事件直到事件流关闭
///
/// 预期结果：
///   - Partial("打开") 应存在
///   - InputEnded(User) 应存在且位于 Final 前
///   - Final("打开 Java 项目") 应恰好出现一次
///   - Empty、Failed、Cancelled 均不应出现
#[test]
fn recognition_pipeline_emits_partial_input_end_and_one_final() {
    let recognizer = ScriptedRecognizer {
        accepted_bytes: 0,
        partial: Some("打开".to_string()),
        final_text: Some("打开 Java 项目".to_string()),
    };
    let pipeline = RecognitionPipeline::new(AsrProviderCapabilities::new(true, None));
    let session = pipeline.start(recognizer);

    session
        .push_pcm(pcm_with_duration_ms(20))
        .expect("活跃识别会话应接受首个 PCM chunk");
    session
        .finish(InputEndReason::User)
        .expect("活跃识别会话应接受用户结束命令");
    let events = session.collect_events();

    assert_eq!(
        events,
        vec![
            RecognitionEvent::Partial("打开".to_string()),
            RecognitionEvent::InputEnded(InputEndReason::User),
            RecognitionEvent::Final("打开 Java 项目".to_string()),
        ],
        "事件必须按 partial、用户结束、唯一 final 的顺序产生"
    );
    assert!(
        !events.contains(&RecognitionEvent::Empty),
        "非空 final 会话不应同时产生 Empty"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RecognitionEvent::Failed(_))),
        "成功会话不应同时产生 Failed"
    );
    assert!(
        !events.contains(&RecognitionEvent::Cancelled),
        "正常完成会话不应同时产生 Cancelled"
    );
}

/// RP-2：达到 Azure 60 秒上限时应自动结束并提交，不接受上限后的音频。
///
/// 数据构造（含关键数值的推导过程）：
///   sample rate = 16,000 samples/s
///   sample width = 2 bytes
///   hard limit = 60 s
///   max PCM = 16,000 × 2 × 60 = 1,920,000 bytes
///   first chunk = 59,000 ms = 1,888,000 bytes
///   second chunk = 2,000 ms = 64,000 bytes，其中只有前 32,000 bytes 可进入本片段
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 启动不支持 partial、60 秒上限的识别会话
///   2. 推送 59 秒 PCM → 会话保持活跃
///   3. 再推送 2 秒 PCM → pipeline 只接收达到 60 秒所需的 1 秒并自动结束
///   4. 尝试推送额外 PCM → 命令被拒绝
///
/// 预期结果：
///   - InputEnded(DurationLimit) 应存在
///   - 已缓冲音频应产生一个非空 Final
///   - 不应产生 Partial、Empty、Failed 或 Cancelled
///   - 上限后的 PCM 不得进入同一会话
#[test]
fn recognition_pipeline_auto_finishes_at_sixty_second_limit() {
    let recognizer = ScriptedRecognizer {
        accepted_bytes: 0,
        partial: None,
        final_text: Some("达到上限前的文本".to_string()),
    };
    let capabilities = AsrProviderCapabilities::new(false, Some(60));
    let pipeline = RecognitionPipeline::new(capabilities);
    let session = pipeline.start(recognizer);

    session
        .push_pcm(pcm_with_duration_ms(59_000))
        .expect("59 秒 PCM 尚未达到 60 秒上限，应被接受");
    session
        .push_pcm(pcm_with_duration_ms(2_000))
        .expect("跨越上限的 PCM 应接受至 60 秒并自动结束");
    let rejected = session.push_pcm(pcm_with_duration_ms(20));
    let events = session.collect_events();

    assert_eq!(
        rejected,
        Err(AsrError::SessionClosed),
        "自动结束后不得接受第 60 秒之后的 PCM"
    );
    assert_eq!(
        events,
        vec![
            RecognitionEvent::InputEnded(InputEndReason::DurationLimit),
            RecognitionEvent::Final("达到上限前的文本".to_string()),
        ],
        "达到 60 秒时应报告时长上限并产生唯一 final"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RecognitionEvent::Partial(_))),
        "Azure 能力声明不支持 partial 时不得伪造 Partial"
    );
    assert!(
        !events.contains(&RecognitionEvent::Empty),
        "非空 final 不应同时产生 Empty"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RecognitionEvent::Failed(_))),
        "时长上限是正常输入边界，不应报告 Failed"
    );
    assert!(
        !events.contains(&RecognitionEvent::Cancelled),
        "自动提交不是用户取消，不应报告 Cancelled"
    );
}

struct CancellationAwareRecognizer {
    cancellation: Mutex<Option<asr::RecognitionCancellation>>,
}

impl Recognizer for CancellationAwareRecognizer {
    fn accept_pcm(&mut self, _chunk: &PcmAudioChunk) -> Result<Vec<String>, AsrError> {
        Ok(Vec::new())
    }

    fn finish(&mut self) -> Result<Option<String>, AsrError> {
        let cancellation = self
            .cancellation
            .lock()
            .expect("取消令牌锁不应中毒")
            .clone()
            .expect("pipeline 应在启动 worker 前注入取消令牌");
        while !cancellation.is_cancelled() {
            std::thread::yield_now();
        }
        Err(AsrError::Provider("late transport error".to_string()))
    }

    fn set_cancellation_token(&mut self, cancellation: asr::RecognitionCancellation) {
        *self.cancellation.lock().expect("取消令牌锁不应中毒") = Some(cancellation);
    }
}

/// RP-3：自动提交后的阻塞供应商必须能被会话外取消句柄立即终止。
///
/// 数据构造（含关键数值的推导过程）：
///   hard limit = 1 s
///   PCM bytes = 16,000 samples/s × 2 bytes/sample × 1 s = 32,000 bytes
///   provider final = 一直等待 cancellation，随后故意返回迟到错误
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 推送恰好 1 秒 PCM → pipeline 自动发送 DurationLimit 并进入 finish
///   2. 从 RecognitionSession 取得独立取消句柄并取消
///   3. 收集 worker 事件直到关闭
///
/// 预期结果：
///   - 正断言：事件为 InputEnded(DurationLimit) + Cancelled
///   - 负断言：供应商取消后的迟到错误不得覆盖 Cancelled 为 Failed
#[test]
fn recognition_pipeline_cancellation_wins_over_late_provider_error() {
    let recognizer = CancellationAwareRecognizer {
        cancellation: Mutex::new(None),
    };
    let pipeline = RecognitionPipeline::new(AsrProviderCapabilities::new(false, Some(1)));
    let session = pipeline.start(recognizer);

    session
        .push_pcm(pcm_with_duration_ms(1_000))
        .expect("恰好达到 1 秒上限的 PCM 应触发自动提交");
    session.cancellation().cancel();
    let events = session.collect_events();

    assert_eq!(
        events,
        vec![
            RecognitionEvent::InputEnded(InputEndReason::DurationLimit),
            RecognitionEvent::Cancelled,
        ],
        "取消必须成为唯一终态，不能被供应商迟到错误覆盖"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, RecognitionEvent::Failed(_))),
        "取消后的迟到 transport 错误不得暴露为 Failed"
    );
}
