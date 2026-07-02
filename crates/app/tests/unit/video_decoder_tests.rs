//! Compile-checked unit test for the `VideoDecoder` spike.
//!
//! `#[ignore]` because it needs a real mp4 on disk. Run with:
//!   `MULTISOUL_SPIKE_VIDEO=/path/x.mp4 cargo test -p app --lib \
//!      video_decoder_tests -- --ignored`
//! (the test is wired into `app::video_decoder_tests` via `#[path]` in
//! `crates/app/src/app/mod.rs`.)

use crate::app::video_decoder::VideoDecoder;

/// Spike: two frames from the test mp4 must decode into non-empty RGBA images.
///
/// 数据构造（含关键数值的推导过程）：
///   path = $MULTISOUL_SPIKE_VIDEO 或默认 /Users/alan/Documents/壁纸/mut/1403512.mp4
///   bytes_per_pixel = 4（RgbaImage 固定 RGBA8）
///   expected_len = width * height * 4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 若 path 不存在 → 提前返回（CI / 无 asset 环境，跳过而非失败）
///   2. VideoDecoder::open_bgra(path) → 内部 AVAssetReader startReading
///   3. next_frame_rgba() 解码第一帧和第二帧
///
/// 预期结果：
///   - 正断言：两帧 width/height > 0，且 raw bytes == width * height * 4
///   - 负断言：两帧 raw bytes 不为空，否则无法生成 RenderImage
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs a real mp4 on disk; set MULTISOUL_SPIKE_VIDEO or use default path"]
fn spike_decoder_returns_rgba_images() {
    let path = std::env::var("MULTISOUL_SPIKE_VIDEO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/Users/alan/Documents/壁纸/mut/1403512.mp4"));
    if !path.exists() {
        // 无 asset 时静默跳过（CI）；不应被当作测试失败。
        return;
    }

    let mut dec = VideoDecoder::open_bgra(&path).expect("open mp4 for BGRA decode");
    let first = dec.next_frame_rgba().expect("decode first RGBA frame");
    let second = dec.next_frame_rgba().expect("decode second RGBA frame");

    let expected_first_len = first.width() as usize * first.height() as usize * 4;
    let expected_second_len = second.width() as usize * second.height() as usize * 4;

    assert!(
        first.width() > 0,
        "first decoded frame width should be positive"
    );
    assert!(
        first.height() > 0,
        "first decoded frame height should be positive"
    );
    assert_eq!(
        first.as_raw().len(),
        expected_first_len,
        "first decoded frame should contain width * height * 4 RGBA bytes"
    );
    assert!(
        !first.as_raw().is_empty(),
        "first decoded frame raw bytes must not be empty"
    );
    assert!(
        second.width() > 0,
        "second decoded frame width should be positive"
    );
    assert!(
        second.height() > 0,
        "second decoded frame height should be positive"
    );
    assert_eq!(
        second.as_raw().len(),
        expected_second_len,
        "second decoded frame should contain width * height * 4 RGBA bytes"
    );
    assert!(
        !second.as_raw().is_empty(),
        "second decoded frame raw bytes must not be empty"
    );
}
