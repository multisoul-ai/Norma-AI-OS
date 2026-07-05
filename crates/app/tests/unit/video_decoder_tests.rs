//! Compile-checked unit test for the `VideoDecoder` spike.
//!
//! `#[ignore]` because it needs a real mp4 on disk. Run with:
//!   `MULTISOUL_SPIKE_VIDEO=/path/x.mp4 cargo test -p app --lib \
//!      video_decoder_tests -- --ignored`
//! (the test is wired into `app::video_decoder_tests` via `#[path]` in
//! `crates/app/src/app/mod.rs`.)

use crate::app::video_decoder::{DecodedVideoFrame, VideoDecoder};
#[cfg(target_os = "macos")]
use core_video::pixel_buffer::{
    kCVPixelFormatType_32BGRA, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, CVPixelBuffer,
};

/// Live surface frame wrapper: wrong pixel formats must be rejected before GPUI paint.
///
/// 数据构造（含关键数值的推导过程）：
///   wrong_format = kCVPixelFormatType_32BGRA（缩略图/CPU 图片格式，不是 GPUI surface live 视频格式）
///   width = 2px, height = 2px（CoreVideo 可创建的最小稳定测试尺寸之一；像素数量 = 2 * 2 = 4）
///   required_format = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange（GPUI Metal surface renderer 会 assert 的格式）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. CVPixelBuffer::new(32BGRA, 2, 2, None) 构造一个真实 CoreVideo buffer
///   2. DecodedVideoFrame::from_buffer(buffer) 尝试把它包装成 live surface frame
///   3. 构造入口检查 pixel format，不允许错误格式继续进入 render_video_wallpaper_surface
///
/// 预期结果：
///   - 正断言：返回 Err，说明错误格式被拦截
///   - 负断言：不应返回 Ok，否则 32BGRA 会到达 gpui::surface 并触发 Metal renderer assert
#[cfg(target_os = "macos")]
#[test]
fn test_decoded_video_frame_rejects_non_420f_pixel_buffer() {
    let buffer = CVPixelBuffer::new(kCVPixelFormatType_32BGRA, 2, 2, None)
        .expect("test should construct a 2x2 BGRA CVPixelBuffer");

    let result = DecodedVideoFrame::from_buffer(buffer);

    assert!(
        result.is_err(),
        "DecodedVideoFrame must reject non-420f buffers before gpui::surface can paint them"
    );
    let accepted_bgra = result.is_ok();
    assert!(
        !accepted_bgra,
        "DecodedVideoFrame must not accept 32BGRA live frames because GPUI surface only accepts 420f"
    );
}

/// Live surface frame wrapper: synthetic 420f buffers without IOSurface backing are rejected.
///
/// 数据构造（含关键数值的推导过程）：
///   format = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange（live 视频要求格式）
///   width = 2px, height = 2px（CoreVideo 可创建的最小稳定测试尺寸之一）
///   IOSurface option = None（故意不提供 kCVPixelBufferIOSurfacePropertiesKey）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. CVPixelBuffer::new(420f, 2, 2, None) 构造一个真实 CoreVideo buffer
///   2. has_io_surface() 确认该 buffer 没有 IOSurface，可复现低拷贝 surface 前置条件缺失
///   3. DecodedVideoFrame::from_buffer(buffer) 尝试包装为 live surface frame
///
/// 预期结果：
///   - 正断言：无 IOSurface 的 420f buffer 被拒绝
///   - 负断言：无 IOSurface 的 420f buffer 不应进入 GPUI surface 渲染链路
#[cfg(target_os = "macos")]
#[test]
fn test_decoded_video_frame_rejects_420f_without_iosurface() {
    let buffer = CVPixelBuffer::new(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, 2, 2, None)
        .expect("test should construct a 2x2 420f CVPixelBuffer without IOSurface options");

    assert!(
        !DecodedVideoFrame::buffer_has_io_surface(&buffer),
        "test setup should create a 420f buffer without IOSurface backing"
    );

    let result = DecodedVideoFrame::from_buffer(buffer);

    assert!(
        result.is_err(),
        "DecodedVideoFrame must reject 420f buffers that lack IOSurface backing"
    );
    let accepted_without_iosurface = result.is_ok();
    assert!(
        !accepted_without_iosurface,
        "DecodedVideoFrame must not accept non-IOSurface 420f buffers for live GPUI surface rendering"
    );
}

/// Spike: live video frames must decode as 420f CVPixelBuffer surfaces.
///
/// 数据构造（含关键数值的推导过程）：
///   path = $MULTISOUL_SPIKE_VIDEO 或默认 /Users/alan/Documents/壁纸/mut/1403512.mp4
///   required_pixel_format = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange（GPUI surface 唯一支持的 live 视频格式）
///   frame_count = 2（连续取两帧，验证 reader 可持续推进而非只返回首帧）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 若 path 不存在 → 提前返回（CI / 无 asset 环境，跳过而非失败）
///   2. VideoDecoder::open(path) → AVAssetReader 以 420f outputSettings startReading
///   3. next_frame() 解码第一帧和第二帧，返回可交给 gpui::surface 的 CVPixelBuffer
///
/// 预期结果：
///   - 正断言：两帧 pixel format 都是 420f
///   - 正断言：两帧 width/height > 0
///   - 负断言：两帧不应返回 32BGRA/RGBA 图片路径格式，否则 live 视频会退回 CPU 图片帧
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs a real mp4 on disk; set MULTISOUL_SPIKE_VIDEO or use default path"]
fn spike_decoder_returns_420f_surface_frames() {
    let path = std::env::var("MULTISOUL_SPIKE_VIDEO")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/Users/alan/Documents/壁纸/mut/1403512.mp4"));
    if !path.exists() {
        // 无 asset 时静默跳过（CI）；不应被当作测试失败。
        return;
    }

    let mut dec = VideoDecoder::open(&path).expect("open mp4 for 420f surface decode");
    let first = dec.next_frame().expect("decode first 420f frame");
    let second = dec.next_frame().expect("decode second 420f frame");

    assert_eq!(
        first.buffer().get_pixel_format(),
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        "first live video frame must be 420f for gpui::surface"
    );
    assert_eq!(
        first.buffer().get_plane_count(),
        2,
        "first live video frame must be bi-planar 420f for GPUI Metal surface rendering"
    );
    assert!(
        DecodedVideoFrame::buffer_has_io_surface(first.buffer()),
        "first live video frame must be backed by IOSurface for low-copy GPUI surface rendering"
    );
    assert!(
        first.buffer().get_width() > 0,
        "first decoded frame width should be positive"
    );
    assert!(
        first.buffer().get_height() > 0,
        "first decoded frame height should be positive"
    );
    assert_eq!(
        second.buffer().get_pixel_format(),
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        "second live video frame must remain 420f after reader advances"
    );
    assert_eq!(
        second.buffer().get_plane_count(),
        2,
        "second live video frame must remain bi-planar 420f after reader advances"
    );
    assert!(
        DecodedVideoFrame::buffer_has_io_surface(second.buffer()),
        "second live video frame must remain IOSurface-backed after reader advances"
    );
    assert!(
        second.buffer().get_width() > 0,
        "second decoded frame width should be positive"
    );
    assert!(
        second.buffer().get_height() > 0,
        "second decoded frame height should be positive"
    );
}
