use super::{
    audio_capture_for_test, pcm16_bytes_from_f32_samples, pcm_stream_chunk_from_samples,
    PCM_STREAM_SAMPLE_RATE_HZ,
};
use realtime::PcmFormat;

fn assert_send<T: Send>() {}

/// AUDIO-3: f32 样本应转换为 little-endian PCM16，并在越界时饱和。
///
/// 数据构造（含关键数值的推导过程）：
///   sample -1.0 → clamp(-1.0) * 32768 = -32768 → i16 LE [0x00, 0x80]
///   sample  0.0 → 0 * 32767 = 0              → i16 LE [0x00, 0x00]
///   sample  1.0 → clamp(1.0) * 32767 = 32767 → i16 LE [0xff, 0x7f]
///   sample -2.0 → clamp(-1.0) = -1.0         → 饱和到 -32768
///   sample  2.0 → clamp(1.0) = 1.0           → 饱和到 32767
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将 [-1.0, 0.0, 1.0] 传给 PCM16 转换 helper
///   2. 检查输出 bytes 是否按 i16 little-endian 排列
///   3. 将 [-2.0, 2.0] 传给同一 helper，验证越界样本先 clamp 再编码
///
/// 预期结果：
///   - 正断言：边界三样本编码为 [-32768, 0, 32767] 的 little-endian bytes
///   - 正断言：越界样本编码为同样的上下界 bytes
///   - 负断言：输出不能是 big-endian 或把 1.0 错映射为 -32768
#[test]
fn test_pcm16_conversion_uses_little_endian_saturation() {
    let boundary_bytes = pcm16_bytes_from_f32_samples(&[-1.0, 0.0, 1.0]);

    assert_eq!(
        boundary_bytes,
        vec![0x00, 0x80, 0x00, 0x00, 0xff, 0x7f],
        "[-1.0, 0.0, 1.0] 应编码为 little-endian PCM16 [-32768, 0, 32767]"
    );
    assert_ne!(
        boundary_bytes,
        vec![0x80, 0x00, 0x00, 0x00, 0x7f, 0xff],
        "PCM16 输出不应是 big-endian 字节序"
    );

    let saturated_bytes = pcm16_bytes_from_f32_samples(&[-2.0, 2.0]);
    assert_eq!(
        saturated_bytes,
        vec![0x00, 0x80, 0xff, 0x7f],
        "越界 f32 样本应先饱和到 [-1.0, 1.0] 再编码为 PCM16"
    );
    assert_ne!(
        saturated_bytes,
        vec![0x00, 0x80, 0x00, 0x80],
        "正向越界样本不应溢出成 -32768"
    );
}

/// AUDIO-4: realtime PCM chunk 应使用 24kHz / mono / PCM16 元数据。
///
/// 数据构造（含关键数值的推导过程）：
///   input samples = [-1.0, 0.0, 1.0]，共 3 个 f32 mono 样本
///   PCM16 bytes   = 3 samples * 2 bytes/sample = 6 bytes
///   sample_rate   = 24_000Hz（Realtime MVP 目标采样率）
///   channels      = 1（麦克风输入转 mono）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将 3 个 f32 样本转换为 realtime::PcmAudioChunk
///   2. 检查 chunk 的 format / sample_rate_hz / channels
///   3. 检查 bytes 与 AUDIO-3 的 little-endian PCM16 输出一致
///
/// 预期结果：
///   - 正断言：chunk 元数据固定为 Pcm16 / 24_000Hz / mono
///   - 正断言：chunk bytes 保留转换后的 6 bytes PCM16 payload
///   - 负断言：realtime stream 不应沿用 one-shot recognition 的 16_000Hz
#[test]
fn test_pcm_stream_chunk_uses_realtime_metadata() {
    let chunk = pcm_stream_chunk_from_samples(&[-1.0, 0.0, 1.0])
        .expect("非空样本应生成 realtime PCM chunk");

    assert_eq!(
        PCM_STREAM_SAMPLE_RATE_HZ, 24_000,
        "realtime stream 目标采样率应固定为 24_000Hz"
    );
    assert_eq!(
        chunk.sample_rate_hz, 24_000,
        "PcmAudioChunk.sample_rate_hz 应写入 Realtime MVP 要求的 24_000Hz"
    );
    assert_ne!(
        chunk.sample_rate_hz, 16_000,
        "realtime stream 不应沿用 sherpa one-shot recognizer 的 16_000Hz"
    );
    assert_eq!(chunk.channels, 1, "PcmAudioChunk.channels 应固定为 mono");
    assert_eq!(
        chunk.format,
        PcmFormat::Pcm16,
        "PcmAudioChunk.format 应固定为 PCM16"
    );
    assert_eq!(
        chunk.bytes,
        vec![0x00, 0x80, 0x00, 0x00, 0xff, 0x7f],
        "chunk payload 应为 f32 样本转换出的 little-endian PCM16 bytes"
    );
}

/// AUDIO-5: 空样本输入不应发送 realtime PCM chunk。
///
/// 数据构造（含关键数值的推导过程）：
///   input samples = []，样本数 0
///   PCM16 bytes   = 0 samples * 2 bytes/sample = 0 bytes
///   expected      = None，避免向 sidecar 发送空音频帧
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 将空 f32 slice 传给 chunk 构造 helper
///   2. helper 在创建 PcmAudioChunk 前发现没有样本
///   3. 返回 None，调用方不会执行 send
///
/// 预期结果：
///   - 正断言：返回 None，表示没有 chunk 可发送
///   - 负断言：不应返回 Some(empty bytes)，避免 transport 产生空音频事件
#[test]
fn test_empty_samples_do_not_create_pcm_stream_chunk() {
    let chunk = pcm_stream_chunk_from_samples(&[]);

    assert!(
        chunk.is_none(),
        "空样本输入应返回 None，让 stream forwarder 跳过发送"
    );
    assert_ne!(
        chunk.map(|chunk| chunk.bytes),
        Some(Vec::new()),
        "空样本输入不应产生带 empty bytes 的 PcmAudioChunk"
    );
}

/// AUDIO-6: PcmStreamSession 必须满足 Send，供 app 后台任务移动。
///
/// 数据构造（含关键数值的推导过程）：
///   PcmStreamSession = start_pcm_stream 返回的 session handle
///   background task  = GPUI background executor capture，要求 captured value: Send
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 编译期调用 assert_send::<crate::PcmStreamSession>()
///   2. 若 session 持有非 Send 的 AVAudioEngine / RcBlock，测试无法编译
///
/// 预期结果：
///   - 正断言：PcmStreamSession 可 Send，可跨线程移动给后台 orchestration
///   - 负断言：session handle 不应直接暴露非 Send 的 macOS capture 对象
#[test]
fn test_pcm_stream_session_is_send() {
    assert_send::<crate::PcmStreamSession>();
}

/// AUDIO-2: macOS 录音层不得调用 AVAudioInputNode::audioUnit。
///
/// 数据构造（含关键数值的推导过程）：
///   source = include_str!("../../src/audio.rs")（编译期读取生产源码）
///   banned selector 1 = ".audioUnit()"（objc2 会校验返回类型编码）
///   banned selector 2 = "AudioUnitSetProperty"（依赖 audioUnit 返回值）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 读取 audio.rs 源码文本
///   2. 检查源码不包含 ".audioUnit()" 调用
///   3. 检查源码不包含 AudioUnitSetProperty 设置当前设备
///
/// 预期结果：
///   - 正断言：源码仍包含 AVAudioEngine，说明保留原有稳定录音路径
///   - 负断言：源码不应包含 audioUnit / AudioUnitSetProperty，避免启动录音时 runtime abort
#[cfg(target_os = "macos")]
#[test]
fn test_macos_audio_capture_does_not_use_aborting_audio_unit_selector() {
    let source = include_str!("../../src/audio.rs");

    assert!(
        source.contains("AVAudioEngine"),
        "macOS 录音层应继续使用已有 AVAudioEngine 路径"
    );
    assert!(
        !source.contains(".audioUnit()"),
        "AVAudioInputNode::audioUnit 返回类型编码不兼容，会在 objc2 message send 校验阶段 abort"
    );
    assert!(
        !source.contains("AudioUnitSetProperty"),
        "不应通过 AudioUnitSetProperty 设置输入设备，因为它依赖 aborting audioUnit selector"
    );
}

/// AUDIO-1: AudioCapture::start 启动后应向 PCM 队列发送录音样本。
///
/// 数据构造（含关键数值的推导过程）：
///   wait_ms = 300ms（给 AVAudioEngine tap 至少一次回调机会）
///   expected_chunks >= 1（真实麦克风有输入时，队列至少收到一个 Samples event）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 crossbeam channel 作为录音输出队列
///   2. 调用 AudioCapture::start(sender)
///   3. 等待 300ms 后停止 capture
///   4. drain channel 并统计收到的 PCM chunk 数量
///
/// 预期结果：
///   - 正断言：有麦克风权限和硬件时至少收到 1 个 chunk
///   - 负断言：不应在启动成功后 300ms 内完全没有 PCM 输出
#[test]
#[ignore = "requires microphone hardware and permission"]
fn test_audio_capture_sends_pcm_chunks() {
    use crossbeam_channel::unbounded;
    use std::time::Duration;

    let (tx, rx) = unbounded();
    let capture =
        audio_capture_for_test(tx).expect("AudioCapture::start should succeed with mic permission");
    std::thread::sleep(Duration::from_millis(300));
    capture.stop();

    let mut chunks_received = 0usize;
    while rx.try_recv().is_ok() {
        chunks_received += 1;
    }
    assert!(
        chunks_received >= 1,
        "should receive at least one PCM chunk in 300ms but received {}",
        chunks_received
    );
}
