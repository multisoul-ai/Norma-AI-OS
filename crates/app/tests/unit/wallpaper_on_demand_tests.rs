use super::resolve_builtin_asset_from_paths;

// 场景：轻量 App 的 bundle 没有外置视频，而源码 checkout 恰有原视频。
// 构造 bundle 空目录和 dev 中同名原字节；预期明确缺资源，不偷偷读取源码。
#[test]
fn packaged_missing_optional_media_never_falls_back_to_source_checkout() {
    let fixture = tempfile::tempdir().expect("resource fixture must be created");
    let bundle = fixture.path().join("bundle");
    let dev = fixture.path().join("dev");
    std::fs::create_dir(&bundle).expect("bundle root must be created");
    std::fs::create_dir_all(dev.join("wallpapers")).expect("dev wallpaper root must be created");
    let rel = "wallpapers/1403512.mp4";
    std::fs::write(dev.join(rel), b"source-video").expect("dev video fixture must be written");
    assert_eq!(
        resolve_builtin_asset_from_paths(rel, Some(&bundle), Some(&dev)),
        None,
        "a packaged app cannot conceal missing external media using checkout assets"
    );
    assert_eq!(
        resolve_builtin_asset_from_paths(rel, None, Some(&dev)),
        Some(dev.join(rel)),
        "unpackaged development must retain its source resource resolution"
    );
}
