use super::audio_capture_for_test;

#[cfg(target_os = "macos")]
use super::{resolve_core_audio_input_device_id, CoreAudioInputDevice};

/// AUDIO-2: 已保存的麦克风 UID 应映射到对应 CoreAudio device id。
///
/// 数据构造（含关键数值的推导过程）：
///   built-in uid = "built-in" → device_id = 11
///   headset uid  = "headset"  → device_id = 42
///   selected uid = "headset"
///   expected     = 42
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造两个 CoreAudioInputDevice，模拟系统中同时存在内置麦克风和耳机麦克风
///   2. 将 selected uid "headset" 传入 resolve_core_audio_input_device_id
///   3. helper 按 uid 精确匹配并返回耳机对应的 device_id
///
/// 预期结果：
///   - 正断言：返回 42，说明录音层会选择耳机麦克风
///   - 负断言：不应返回 11，避免继续录到内置麦克风
#[cfg(target_os = "macos")]
#[test]
fn test_resolve_core_audio_input_device_id_selects_saved_uid() {
    let devices = vec![
        CoreAudioInputDevice {
            uid: "built-in".to_string(),
            device_id: 11,
        },
        CoreAudioInputDevice {
            uid: "headset".to_string(),
            device_id: 42,
        },
    ];

    let selected = resolve_core_audio_input_device_id("headset", &devices)
        .expect("headset uid should resolve to a CoreAudio device id");

    assert_eq!(
        selected, 42,
        "已保存的 headset uid 应解析到耳机麦克风 device_id"
    );
    assert_ne!(
        selected, 11,
        "已保存的 headset uid 不应解析到内置麦克风 device_id"
    );
}

/// AUDIO-3: 已保存的麦克风 UID 不存在时应返回明确错误。
///
/// 数据构造（含关键数值的推导过程）：
///   built-in uid = "built-in" → device_id = 11
///   headset uid  = absent（设备列表中不存在）
///   selected uid = "headset"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造只包含内置麦克风的 CoreAudioInputDevice 列表
///   2. 将 selected uid "headset" 传入 resolve_core_audio_input_device_id
///   3. helper 无法找到匹配设备，应返回 SpeechError::Audio
///
/// 预期结果：
///   - 正断言：错误消息包含 headset，方便用户判断是耳机不可用
///   - 负断言：不应静默回退到 11，否则用户选择耳机后仍会录到电脑麦克风
#[cfg(target_os = "macos")]
#[test]
fn test_resolve_core_audio_input_device_id_missing_uid_returns_error() {
    let devices = vec![CoreAudioInputDevice {
        uid: "built-in".to_string(),
        device_id: 11,
    }];

    let error = resolve_core_audio_input_device_id("headset", &devices)
        .expect_err("missing headset uid should return a microphone selection error");

    assert!(
        error.to_string().contains("headset"),
        "缺失设备错误应包含 headset uid，实际错误为 {error}"
    );
    assert_ne!(
        error,
        crate::SpeechError::Audio("selected microphone is unavailable: built-in".to_string()),
        "缺失 headset 时不应把内置麦克风伪装成失败设备"
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
