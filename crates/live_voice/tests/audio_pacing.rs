use audio::{PcmAudioChunk, PcmFormat};
use live_voice::AudioPacer;
use std::time::Duration;

/// AC03：积压音频按采样时长发送，延迟之后不以突发方式追赶旧时钟。
///
/// 数据构造：3200字节/(16000Hz×2字节)=100ms；用显式时间替代真实sleep。
/// 执行过程：0ms预约首包，50ms预约第二包，延迟到1秒后预约第三包。
/// 预期结果：开始时间分别0、100、1000ms；无效输入不推进后续预约。
#[test]
fn capture_backlog_is_paced_and_late_chunks_do_not_burst_to_catch_up() {
    let mut pacer = AudioPacer::default();
    let mut pcm =
        PcmAudioChunk::new(PcmFormat::Pcm16, 16_000, 1, vec![0; 3_200]).expect("100ms PCM有效");
    assert_eq!(
        pacer.reserve(Duration::ZERO, &pcm),
        Ok(Duration::ZERO),
        "首包可立即发送"
    );
    assert_eq!(
        pacer.reserve(Duration::from_millis(50), &pcm),
        Ok(Duration::from_millis(100)),
        "第二包至少保留前包100ms节奏"
    );
    assert_eq!(
        pacer.reserve(Duration::from_secs(1), &pcm),
        Ok(Duration::from_secs(1)),
        "迟到后从现在继续，不向过去补发"
    );
    pcm.sample_rate_hz = 24_000;
    assert!(
        pacer.reserve(Duration::from_secs(10), &pcm).is_err(),
        "错误格式不能改变发送时钟"
    );
    pcm.sample_rate_hz = 16_000;
    assert_eq!(
        pacer.reserve(Duration::from_secs(1), &pcm),
        Ok(Duration::from_millis(1100)),
        "拒绝的包不能把时钟推进到10秒"
    );
}
