#![cfg(feature = "test-support")]
use audio::{PcmAudioChunk, PcmFormat};
use voice_input::{SherpaVadClient, VadClient, VadEvent};

fn pcm(sample: i16) -> PcmAudioChunk {
    // 16000 samples/s × 0.1s = 1600 samples, PCM16 = 3200 bytes.
    PcmAudioChunk::new(
        PcmFormat::Pcm16,
        16_000,
        1,
        sample.to_le_bytes().repeat(1600),
    )
    .expect("100ms PCM")
}

/// AC05：VAD给出相对于实际输入PCM的完整语音段边界，不能把检测通知到达时刻当起点。
/// 数据构造：确定性VAD，100ms静音→100ms语音→100ms静音，共4800样本。
/// 执行过程：逐块observe_pcm，读取已完成段；reset后从新时间原点再次送入语音和静音。
/// 预期结果：SpeechStart即时出现但不伪造完整段；完成段为[1600,3200)，reset后为[0,1600)。
#[test]
fn timed_vad_reports_sample_spans_and_resets_its_clock() {
    let mut vad = SherpaVadClient::deterministic_for_tests();
    let silent = vad.observe_pcm(&pcm(0)).expect("静音");
    assert!(silent.completed_spans.is_empty(), "静音不形成语音段");
    let start = vad.observe_pcm(&pcm(i16::MAX)).expect("语音");
    assert_eq!(
        start.events,
        vec![VadEvent::SpeechStart],
        "语音起点可立即用于取消旧规划"
    );
    assert!(
        start.completed_spans.is_empty(),
        "不能把延迟检测时间冒充已确认起点"
    );
    let end = vad.observe_pcm(&pcm(0)).expect("尾静音");
    assert_eq!(end.completed_spans.len(), 1, "只有一个完整语音段");
    assert_eq!(
        end.completed_spans[0].start_sample, 1600,
        "段起点属于原始上传PCM时轴"
    );
    assert_eq!(
        end.completed_spans[0].end_sample, 3200,
        "段终点不包括尾静音"
    );
    vad.reset();
    vad.observe_pcm(&pcm(i16::MAX)).expect("reset后语音");
    let reset = vad.observe_pcm(&pcm(0)).expect("reset后静音");
    assert_eq!(
        reset.completed_spans[0].start_sample, 0,
        "reset清除旧采样时钟"
    );
    assert_eq!(reset.completed_spans[0].end_sample, 1600, "新段仅占100ms");
}
