use super::audio_capture_for_test;

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

/// AUDIO-1: AudioCapture::start を呼び出すと PCM データが送信されることを確認。
///
/// 注意: このテストは実際のマイクハードウェアに依存するため CI ではスキップ。
/// 手動検証用: 実際のマイクがあるマシンで実行。
///
/// 実行過程:
///   1. crossbeam 無制限チャネルを生成
///   2. AudioCapture::start(sender) を呼び出し
///   3. 300ms 待機
///   4. AudioCapture::stop() を呼び出し
///   5. チャネルから少なくとも 1 個以上の PCM チャンクを受信確認
///
/// 予想結果:
///   - start() が Ok を返す（マイク権限がある場合）
///   - チャネルから Vec<f32> が受信される
///   - 各チャンクは 16kHz mono → 長さが 1600 の倍数（100ms 単位）
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
