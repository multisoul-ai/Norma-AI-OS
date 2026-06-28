//! Compile-checked unit test for the `VideoDecoder` spike.
//!
//! `#[ignore]` because it needs a real mp4 on disk. Run with:
//!   `MULTISOUL_SPIKE_VIDEO=/path/x.mp4 cargo test -p app --lib \
//!      video_decoder_tests -- --ignored`
//! (the test is wired into `app::video_decoder_tests` via `#[path]` in
//! `crates/app/src/app/mod.rs`.)

use crate::app::video_decoder::VideoDecoder;

/// Spike: 30 frames from the test mp4 must each be 420f.
///
/// 数据构造（含关键数值的推导过程）：
///   path = $MULTISOUL_SPIKE_VIDEO 或默认 /Users/alan/Documents/壁纸/mut/1403512.mp4
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 若 path 不存在 → 提前返回（CI / 无 asset 环境，跳过而非失败）
///   2. VideoDecoder::open(path) → 内部 AVAssetReader startReading
///   3. 循环 30 次 next_frame() → 每帧 CVPixelBuffer.get_pixel_format()
///
/// 预期结果：
///   - 正断言：每帧 pixel format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
///   - 原因：GPUI 0.2.2 metal_renderer.rs:1085 在非 420f 上 assert_eq! panic，
///     420f 是 surface() 喂入视频的唯一可用格式。
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs a real mp4 on disk; set MULTISOUL_SPIKE_VIDEO or use default path"]
fn spike_decoder_returns_420f_frames() {
    use core_video::pixel_buffer::kCVPixelFormatType_420YpCbCr8BiPlanarFullRange;

    let path = std::env::var("MULTISOUL_SPIKE_VIDEO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/Users/alan/Documents/壁纸/mut/1403512.mp4"));
    if !path.exists() {
        // 无 asset 时静默跳过（CI）；不应被当作测试失败。
        return;
    }
    let mut dec = VideoDecoder::open(&path).expect("open mp4");
    for i in 0..30 {
        let f = dec.next_frame().expect("next frame");
        let pf = f.buffer.get_pixel_format();
        assert_eq!(
            pf, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            "frame {}: GPUI surface() requires 420f; got 0x{:08x}",
            i, pf
        );
    }
}
