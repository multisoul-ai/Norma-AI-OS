use std::path::Path;

/// License 激活壳背景：必须打包用户指定的 MP4 并复用 Workspace 低拷贝播放管线。
///
/// 数据构造（含关键数值的推导过程）：
///   source video size = 14,532,815 bytes（用户指定 Downloads MP4 的 stat 结果）
///   decoded dimensions = 1280×720（ffprobe 对指定 MP4 的实际视频流结果）
///   packaged asset    = assets/wallpapers/license-gate.mp4
///   live frame path   = AVAssetReader 420f CVPixelBuffer → gpui::surface
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 检查打包资源 → 确认指定 MP4 已复制到 wallpapers 目录
///   2. 检查 GateRoot 组装 → 确认激活壳启动并挂载视频背景
///   3. 检查渲染路径 → 确认复用 video_wallpaper_surface 而非 CPU RenderImage
///
/// 预期结果：
///   - 正断言：license-gate.mp4 存在且大小与指定源文件一致
///   - 正断言：GateRoot 使用独立 License 资源并复用 surface 管线
///   - 负断言：GateRoot 不得使用 RenderImage 或 CPU RGBA 路径
#[test]
fn test_license_gate_uses_packaged_video_on_low_copy_surface_path() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let asset = manifest_dir.join("assets/wallpapers/license-gate.mp4");
    let gate_source = include_str!("../../src/license_gate/gate_root.rs");

    assert!(
        asset.is_file(),
        "License 激活壳指定的 assets/wallpapers/license-gate.mp4 必须存在并被打包"
    );
    let asset_size = std::fs::metadata(&asset)
        .expect("License 背景 MP4 必须可读取元数据")
        .len();
    assert_eq!(
        asset_size, 14_532_815,
        "License 背景必须与用户指定的 14,532,815-byte MP4 一致"
    );
    assert!(
        gate_source.contains("wallpapers/license-gate.mp4"),
        "GateRoot 必须解析独立的 License 背景资源"
    );
    assert!(
        gate_source.contains("sync_license_video_background"),
        "激活壳渲染前必须启动 License 视频同步循环"
    );
    assert!(
        gate_source.contains("video_wallpaper_surface"),
        "License 背景必须复用 Workspace 的 CVPixelBuffer surface 渲染路径"
    );
    assert!(
        !gate_source.contains("RenderImage"),
        "License 视频背景不得每帧构造 RenderImage"
    );
    assert!(
        !gate_source.contains("render_image_from_rgba"),
        "License 视频背景不得回退到 CPU RGBA 转换"
    );
}
