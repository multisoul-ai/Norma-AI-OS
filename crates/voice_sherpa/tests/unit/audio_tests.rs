use super::audio_capture_for_test;

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
