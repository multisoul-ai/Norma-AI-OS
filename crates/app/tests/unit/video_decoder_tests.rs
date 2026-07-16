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

/// V-PTS-2：生产 4K 视频解码帧必须携带递增 PTS，且不破坏低拷贝 surface 契约。
///
/// 数据构造（含关键数值的推导过程）：
///   source = wallpapers/1406443.mp4（3840×2160，约 29.93fps）
///   frame_count = 2（首帧 + 下一帧）
///   nominal_delta ≈ 1 / 29.93s ≈ 33.4ms
///   合理区间 = 20ms..50ms（容纳容器 timescale 舍入，但排除 0 与固定 40ms 伪造值）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 通过生产 asset resolver 打开真实 4K MP4
///   2. `VideoDecoder::next_frame()` 连续解码两帧
///   3. 读取两帧从 CMSampleBuffer 保留下来的 presentation timestamp
///   4. 复核尺寸、420f、双平面和 IOSurface
///
/// 预期结果：
///   - 正断言：两帧都有 PTS，第二帧严格晚于第一帧
///   - 正断言：PTS 差值位于 20ms..50ms
///   - 正断言：两帧均保持 3840×2160、420f、双平面、IOSurface-backed
///   - 负断言：不得为了时间戳回退到 32BGRA/CPU 图片帧
#[cfg(target_os = "macos")]
#[test]
fn test_decoder_preserves_increasing_presentation_timestamps_for_4k_source() {
    let path = crate::app::wallpaper_resolvers::resolve_builtin_asset("wallpapers/1406443.mp4")
        .expect("生产 4K Launchpad 视频必须存在于应用资源目录");
    let mut decoder = VideoDecoder::open(&path).expect("AVFoundation 应能打开生产 4K 视频");
    let first = decoder.next_frame().expect("生产 4K 视频首帧应能解码");
    let second = decoder
        .next_frame()
        .expect("生产 4K 视频第二帧应能连续解码");
    let first_pts = first
        .presentation_time()
        .expect("首帧必须保留 CMSampleBuffer presentation timestamp");
    let second_pts = second
        .presentation_time()
        .expect("第二帧必须保留 CMSampleBuffer presentation timestamp");
    let delta = second_pts
        .checked_sub(first_pts)
        .expect("第二帧 PTS 必须严格晚于首帧");

    assert!(
        second_pts > first_pts,
        "生产 4K 视频第二帧 PTS 必须递增，worker 才能按真实时间推进"
    );
    assert!(
        delta >= std::time::Duration::from_millis(20)
            && delta <= std::time::Duration::from_millis(50),
        "约 29.93fps 视频的相邻 PTS 必须位于 20ms..50ms，实际为 {delta:?}"
    );
    assert_eq!(
        (first.buffer().get_width(), first.buffer().get_height()),
        (3840, 2160),
        "首帧必须保持生产源的真实 4K 尺寸"
    );
    assert_eq!(
        (second.buffer().get_width(), second.buffer().get_height()),
        (3840, 2160),
        "第二帧必须继续保持生产源的真实 4K 尺寸"
    );
    assert_eq!(
        first.buffer().get_pixel_format(),
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        "携带 PTS 的首帧必须是 420f，不能回退为 32BGRA"
    );
    assert_eq!(
        second.buffer().get_pixel_format(),
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        "携带 PTS 的 live 帧仍必须是 420f，不能回退为 32BGRA"
    );
    assert_eq!(
        first.buffer().get_plane_count(),
        2,
        "携带 PTS 的首帧必须保持双平面 420f"
    );
    assert_eq!(
        second.buffer().get_plane_count(),
        2,
        "携带 PTS 的 live 帧必须继续保持双平面 420f"
    );
    assert!(
        DecodedVideoFrame::buffer_has_io_surface(first.buffer()),
        "携带 PTS 的首帧必须由 IOSurface 支撑低拷贝上屏"
    );
    assert!(
        DecodedVideoFrame::buffer_has_io_surface(second.buffer()),
        "携带 PTS 的 live 帧必须继续由 IOSurface 支撑低拷贝上屏"
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

/// HD-BENCH-D1：真实 `VideoDecoder` 连续解码指定视频，输出可比较的吞吐与尺寸证据。
///
/// 数据构造（含关键数值的推导过程）：
///   path            = `$NORMA_HD_BENCHMARK_VIDEO`（实验矩阵中的一个真实 mp4）
///   expected_width  = `$NORMA_HD_EXPECTED_WIDTH`（1280 / 1920 / 2560 / 3840）
///   expected_height = `$NORMA_HD_EXPECTED_HEIGHT`（720 / 1080 / 1440 / 2160）
///   frame_count     = 180 frames（30fps × 6s = 180，足以越过解码器冷启动并保持单次实验可控）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. `VideoDecoder::open(path)` 通过 AVAssetReader 打开真实视频
///   2. 单独解码首帧并记录其 420f/IOSurface/尺寸契约
///   3. 再连续解码 179 帧，总计 180 帧，并记录末帧契约
///   4. 用 `180 / elapsed_seconds` 计算无 sleep 的实际解码吞吐
///
/// 预期结果：
///   - 正断言：首帧和末帧尺寸均等于实验档位声明
///   - 正断言：首帧和末帧均为双平面 420f、IOSurface-backed
///   - 负断言：输出不得变成 32BGRA 或零尺寸帧
///   - 正断言：打印一行 `NORMA_HD_DECODE_RESULT` 供基准脚本采集
#[cfg(target_os = "macos")]
#[test]
#[ignore = "HD 实验：需要 NORMA_HD_BENCHMARK_VIDEO 与预期尺寸环境变量"]
fn benchmark_video_decoder_hd_matrix() {
    use std::time::Instant;

    let path = std::path::PathBuf::from(
        std::env::var("NORMA_HD_BENCHMARK_VIDEO")
            .expect("HD benchmark requires NORMA_HD_BENCHMARK_VIDEO"),
    );
    let expected_width: usize = std::env::var("NORMA_HD_EXPECTED_WIDTH")
        .expect("HD benchmark requires NORMA_HD_EXPECTED_WIDTH")
        .parse()
        .expect("NORMA_HD_EXPECTED_WIDTH must be a positive integer");
    let expected_height: usize = std::env::var("NORMA_HD_EXPECTED_HEIGHT")
        .expect("HD benchmark requires NORMA_HD_EXPECTED_HEIGHT")
        .parse()
        .expect("NORMA_HD_EXPECTED_HEIGHT must be a positive integer");
    // frame_count = playback_fps(30) × measurement_window(6s) = 180 frames.
    let frame_count = 180usize;

    assert!(
        path.is_file(),
        "HD benchmark input must be a real video file: {}",
        path.display()
    );
    assert!(
        expected_width > 0 && expected_height > 0,
        "HD benchmark expected dimensions must both be positive"
    );

    let mut decoder = VideoDecoder::open(&path).expect("HD benchmark should open the video");
    let started = Instant::now();
    let first = decoder
        .next_frame()
        .expect("HD benchmark should decode the first frame");
    let mut last = decoder
        .next_frame()
        .expect("HD benchmark should decode the second frame");
    for _ in 2..frame_count {
        last = decoder
            .next_frame()
            .expect("HD benchmark should continuously decode every requested frame");
    }
    let elapsed = started.elapsed();
    let decode_fps = frame_count as f64 / elapsed.as_secs_f64();
    let milliseconds_per_frame = elapsed.as_secs_f64() * 1_000.0 / frame_count as f64;

    assert_eq!(
        first.buffer().get_width(),
        expected_width,
        "first decoded frame width must equal the declared experiment width"
    );
    assert_eq!(
        first.buffer().get_height(),
        expected_height,
        "first decoded frame height must equal the declared experiment height"
    );
    assert_eq!(
        last.buffer().get_width(),
        expected_width,
        "last decoded frame width must remain equal to the declared experiment width"
    );
    assert_eq!(
        last.buffer().get_height(),
        expected_height,
        "last decoded frame height must remain equal to the declared experiment height"
    );
    assert_eq!(
        first.buffer().get_pixel_format(),
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        "first benchmark frame must stay on the 420f live surface path"
    );
    assert_eq!(
        last.buffer().get_pixel_format(),
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        "last benchmark frame must stay on the 420f live surface path"
    );
    assert_eq!(
        first.buffer().get_plane_count(),
        2,
        "first benchmark frame must be bi-planar instead of a packed BGRA image"
    );
    assert_eq!(
        last.buffer().get_plane_count(),
        2,
        "last benchmark frame must remain bi-planar instead of a packed BGRA image"
    );
    assert!(
        DecodedVideoFrame::buffer_has_io_surface(first.buffer()),
        "first benchmark frame must be IOSurface-backed for low-copy rendering"
    );
    assert!(
        DecodedVideoFrame::buffer_has_io_surface(last.buffer()),
        "last benchmark frame must remain IOSurface-backed for low-copy rendering"
    );
    assert!(
        elapsed.as_nanos() > 0,
        "HD benchmark elapsed time must be measurable and non-zero"
    );

    println!(
        "NORMA_HD_DECODE_RESULT\tfile={}\twidth={}\theight={}\tframes={}\telapsed_ms={:.3}\tdecode_fps={:.3}\tms_per_frame={:.3}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown"),
        expected_width,
        expected_height,
        frame_count,
        elapsed.as_secs_f64() * 1_000.0,
        decode_fps,
        milliseconds_per_frame,
    );
}
