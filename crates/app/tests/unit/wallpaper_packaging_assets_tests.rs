use std::path::{Path, PathBuf};

fn asset_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(rel)
}

fn inspect_webp(rel: &str) -> (u64, (u32, u32), [u8; 12]) {
    let path = asset_path(rel);
    let bytes = std::fs::read(&path).expect("optimized WebP asset must be readable");
    let header: [u8; 12] = bytes
        .get(..12)
        .expect("optimized WebP asset must include a complete RIFF header")
        .try_into()
        .expect("the 12-byte WebP header slice must convert to an array");
    let decoded = image::ImageReader::open(&path)
        .expect("optimized WebP asset must open through the production image decoder")
        .decode()
        .expect("optimized WebP asset must decode through the production image decoder");
    (
        bytes.len() as u64,
        (decoded.width(), decoded.height()),
        header,
    )
}

/// T-R11: 六张体积优化后的 WebP 必须可解码、保留原分辨率并满足逐文件预算。
///
/// 数据构造（含关键数值的推导过程）：
///   default lossless budget = 4 MiB = 4 × 1024 × 1024 = 4,194,304 bytes
///   Aurora budget          = 6 MiB = 6 × 1024 × 1024 = 6,291,456 bytes
///   Nebula budget          = 2 MiB = 2 × 1024 × 1024 = 2,097,152 bytes
///   74174 budget           = 512 KiB = 512 × 1024 = 524,288 bytes
///   1411492 budget         = 3 MiB = 3 × 1024 × 1024 = 3,145,728 bytes
///   1411385 budget         = 2 MiB = 2 × 1024 × 1024 = 2,097,152 bytes
///   aggregate ceiling      = 18 MiB = 18 × 1024 × 1024 = 18,874,368 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 逐张读取 RIFF/WEBP 头，并用 production `image` decoder 解码。
///   2. 逐张核对转换前后的像素宽高不变。
///   3. 逐张核对大小上限，再汇总 6 张资源的总体积。
///
/// 预期结果：
///   - 正断言：6 张 WebP 均可解码且分辨率精确匹配原图。
///   - 正断言：每张均低于独立预算，合计不超过 18 MiB。
///   - 负断言：任何文件不得缺少 `RIFF....WEBP` 头或膨胀回 PNG 级体积。
#[test]
fn test_optimized_builtin_webps_decode_at_original_dimensions_within_budgets() {
    let default = inspect_webp("wallpapers/1331865.webp");
    let aurora = inspect_webp("wallpapers/1350954.webp");
    let nebula = inspect_webp("wallpapers/1358524.webp");
    let mut_74174 = inspect_webp("wallpapers/74174.webp");
    let mut_1411492 = inspect_webp("wallpapers/1411492.webp");
    let mut_1411385 = inspect_webp("wallpapers/1411385.webp");

    assert_eq!(
        (&default.2[..4], &default.2[8..]),
        (&b"RIFF"[..], &b"WEBP"[..]),
        "default wallpaper must retain a valid RIFF/WEBP container header"
    );
    assert_eq!(
        default.1,
        (2184, 1224),
        "lossless default WebP must preserve the original 2184×1224 resolution"
    );
    assert!(
        default.0 <= 4 * 1024 * 1024,
        "lossless default WebP must stay within 4 MiB; got {} bytes",
        default.0
    );

    assert_eq!(
        aurora.1,
        (5824, 3264),
        "Aurora WebP must preserve the original 5824×3264 resolution"
    );
    assert!(
        aurora.0 <= 6 * 1024 * 1024,
        "Aurora WebP must stay within 6 MiB; got {} bytes",
        aurora.0
    );
    assert_eq!(
        nebula.1,
        (2912, 1632),
        "Nebula WebP must preserve the original 2912×1632 resolution"
    );
    assert!(
        nebula.0 <= 2 * 1024 * 1024,
        "Nebula WebP must stay within 2 MiB; got {} bytes",
        nebula.0
    );
    assert_eq!(
        mut_74174.1,
        (1920, 1371),
        "MUT 74174 WebP must preserve the original 1920×1371 resolution"
    );
    assert!(
        mut_74174.0 <= 512 * 1024,
        "MUT 74174 WebP must stay within 512 KiB; got {} bytes",
        mut_74174.0
    );
    assert_eq!(
        mut_1411492.1,
        (2944, 1648),
        "MUT 1411492 WebP must preserve the original 2944×1648 resolution"
    );
    assert!(
        mut_1411492.0 <= 3 * 1024 * 1024,
        "MUT 1411492 WebP must stay within 3 MiB; got {} bytes",
        mut_1411492.0
    );
    assert_eq!(
        mut_1411385.1,
        (3840, 2160),
        "MUT 1411385 WebP must preserve the original 3840×2160 resolution"
    );
    assert!(
        mut_1411385.0 <= 2 * 1024 * 1024,
        "MUT 1411385 WebP must stay within 2 MiB; got {} bytes",
        mut_1411385.0
    );

    let total = default.0 + aurora.0 + nebula.0 + mut_74174.0 + mut_1411492.0 + mut_1411385.0;
    assert!(
        total <= 18 * 1024 * 1024,
        "the six optimized WebPs must total at most 18 MiB; got {total} bytes"
    );
}

/// T-R12: 已转换的旧 PNG 不得继续留在资源目录造成双份打包。
///
/// 数据构造（含关键数值的推导过程）：
///   migrated basenames = 6 个
///   forbidden legacy files = 每个 basename × 1 个 `.png` = 6 个
///   expected duplicate bytes = 0
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 分别构造 6 条旧 PNG 的源码资源路径。
///   2. 逐一检查文件不存在，确保 Packager 的目录复制不会再次带入旧格式。
///
/// 预期结果：
///   - 负断言：6 个旧 PNG 逐一不存在。
///   - 正断言：同 basename 的 WebP 已由 T-R11 独立验证可读。
#[test]
fn test_legacy_png_sources_are_absent_after_webp_migration() {
    assert!(
        !asset_path("wallpapers/1331865.png").exists(),
        "legacy 1331865.png must be absent or Packager will duplicate the default wallpaper"
    );
    assert!(
        !asset_path("wallpapers/1350954.png").exists(),
        "legacy 1350954.png must be absent or Packager will duplicate Aurora"
    );
    assert!(
        !asset_path("wallpapers/1358524.png").exists(),
        "legacy 1358524.png must be absent or Packager will duplicate Nebula"
    );
    assert!(
        !asset_path("wallpapers/74174.png").exists(),
        "legacy 74174.png must be absent or Packager will duplicate the MUT wallpaper"
    );
    assert!(
        !asset_path("wallpapers/1411492.png").exists(),
        "legacy 1411492.png must be absent or Packager will duplicate the MUT wallpaper"
    );
    assert!(
        !asset_path("wallpapers/1411385.png").exists(),
        "legacy 1411385.png must be absent or Packager will duplicate the MUT wallpaper"
    );
}
