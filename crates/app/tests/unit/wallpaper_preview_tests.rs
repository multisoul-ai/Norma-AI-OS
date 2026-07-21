use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame};

use super::preview::{decode_image_preview, render_preview_from_rgba, PreviewCache};

/// 壁纸预览缩放：高分辨率像素源只能生成 640px 最长边的单帧预览。
///
/// 数据构造（含关键数值的推导过程）：
///   source = 1280 × 720 RGBA = 1280 × 720 × 4 = 3,686,400 bytes
///   scale  = 640 / 1280 = 0.5
///   output = 640 × 360 RGBA = 640 × 360 × 4 = 921,600 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 1280×720 的纯色 RGBA 像素源。
///   2. 通过壁纸预览公共入口生成 GPUI RenderImage。
///   3. 读取预览帧数、像素尺寸和实际字节长度。
///
/// 预期结果：
///   - 正断言：输出恰好一帧，避免 GIF/视频预览保留动画帧。
///   - 正断言：输出尺寸为 640×360，保持宽高比且最长边不超过 640px。
///   - 正断言：输出占用恰好 921,600 bytes。
///   - 负断言：输出不得保留 1280×720 原始尺寸或 3,686,400 bytes 原始像素量。
#[test]
fn wallpaper_preview_downscales_to_one_bounded_frame() {
    let source = image::RgbaImage::from_pixel(1280, 720, image::Rgba([0x12, 0x34, 0x56, 0xff]));

    let preview = render_preview_from_rgba(source);
    let size = preview.image.size(0);
    let bytes = preview
        .image
        .as_bytes(0)
        .expect("单帧壁纸预览必须暴露第 0 帧像素数据");

    assert_eq!(
        preview.image.frame_count(),
        1,
        "壁纸设置缩略图必须只有一帧，不能保留 GIF 或视频动画帧"
    );
    assert_eq!(
        (u32::from(size.width), u32::from(size.height)),
        (640, 360),
        "1280×720 预览应按 0.5 比例缩为 640×360"
    );
    assert_eq!(
        bytes.len(),
        921_600,
        "640×360 RGBA 预览应占 640×360×4 = 921,600 bytes"
    );
    assert_ne!(
        (u32::from(size.width), u32::from(size.height)),
        (1280, 720),
        "预览不得保留 1280×720 原始分辨率"
    );
    assert_ne!(
        bytes.len(),
        3_686_400,
        "预览不得保留 1280×720×4 = 3,686,400 bytes 原始像素量"
    );
}

/// GIF 预览解码：多帧源只能暴露缩放后的第一帧。
///
/// 数据构造（含关键数值的推导过程）：
///   source frames = 2
///   each frame    = 800 × 400 × 4 = 1,280,000 bytes
///   expanded GIF  = 2 × 1,280,000 = 2,560,000 bytes
///   scale         = 640 / 800 = 0.8
///   preview       = 640 × 320 × 4 = 819,200 bytes
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入红色首帧与绿色第二帧组成的临时 GIF。
///   2. 通过文件预览入口解码并生成 GPUI RenderImage。
///   3. 检查帧数、缩放尺寸、字节预算与首像素颜色。
///
/// 预期结果：
///   - 正断言：预览只有一帧且尺寸为 640×320。
///   - 正断言：预览字节数为 819,200，首像素来自红色第一帧。
///   - 负断言：不得保留第二帧绿色像素或 2,560,000 bytes 的展开动画。
#[test]
fn wallpaper_gif_preview_decodes_only_first_frame() {
    let temp = tempfile::tempdir().expect("应创建隔离的 GIF 预览测试目录");
    let path = temp.path().join("two-frames.gif");
    let file = File::create(&path).expect("应创建临时 GIF 文件");
    let mut encoder = GifEncoder::new(file);
    encoder
        .set_repeat(Repeat::Infinite)
        .expect("应写入 GIF 循环标记");
    let delay = Delay::from_saturating_duration(Duration::from_millis(20));
    encoder
        .encode_frame(Frame::from_parts(
            image::RgbaImage::from_pixel(800, 400, image::Rgba([0xff, 0x00, 0x00, 0xff])),
            0,
            0,
            delay,
        ))
        .expect("应写入红色 GIF 首帧");
    encoder
        .encode_frame(Frame::from_parts(
            image::RgbaImage::from_pixel(800, 400, image::Rgba([0x00, 0xff, 0x00, 0xff])),
            0,
            0,
            delay,
        ))
        .expect("应写入绿色 GIF 第二帧");
    drop(encoder);

    let preview = decode_image_preview(&path).expect("两帧 GIF 应生成有界单帧预览");
    let size = preview.image.size(0);
    let bytes = preview
        .image
        .as_bytes(0)
        .expect("GIF 预览必须暴露第 0 帧像素数据");

    assert_eq!(
        preview.image.frame_count(),
        1,
        "两帧 GIF 在设置页中必须退化为单帧预览"
    );
    assert_eq!(
        (u32::from(size.width), u32::from(size.height)),
        (640, 320),
        "800×400 GIF 首帧应按 0.8 比例缩为 640×320"
    );
    assert_eq!(
        preview.byte_len, 819_200,
        "640×320 RGBA 单帧应占 640×320×4 = 819,200 bytes"
    );
    assert_eq!(
        &bytes[..4],
        &[0x00, 0x00, 0xff, 0xff],
        "GPUI BGRA 首像素应来自红色 GIF 第一帧"
    );
    assert_ne!(
        &bytes[..4],
        &[0x00, 0xff, 0x00, 0xff],
        "预览不得错误保留绿色 GIF 第二帧"
    );
    assert_ne!(
        preview.byte_len, 2_560_000,
        "预览不得保留两帧 800×400×4 = 2,560,000 bytes 展开动画"
    );
}

/// 壁纸预览缓存：达到字节预算后按最近最少使用顺序精确淘汰。
///
/// 数据构造（含关键数值的推导过程）：
///   budget = 12 bytes
///   A/B/C/D 每张 = 1 × 1 × 4 = 4 bytes
///   A + B + C = 12 bytes，恰好满额
///   访问 A 后 LRU 顺序 = B, C, A
///   插入 D 后临时总量 = 16 bytes > 12 bytes，因此只需淘汰 B(4 bytes)
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次完成 A、B、C 三个预览请求，填满缓存。
///   2. 读取 A，使 A 成为最近使用项。
///   3. 完成 D 请求，收集被淘汰的 RenderImage。
///   4. 查询 A、B、C、D 的最终存在状态与总字节数。
///
/// 预期结果：
///   - 正断言：A、C、D 均保留，总量仍为 12 bytes。
///   - 正断言：淘汰列表恰好包含 B 的 image id。
///   - 负断言：B 不得继续存在，A 不得因插入顺序较早而被错误淘汰。
#[test]
fn wallpaper_preview_cache_evicts_least_recently_used_image_by_byte_budget() {
    let mut cache = PreviewCache::with_budget(12);
    let key_a = PathBuf::from("A.png");
    let key_b = PathBuf::from("B.png");
    let key_c = PathBuf::from("C.png");
    let key_d = PathBuf::from("D.png");

    let id_a = insert_one_pixel(&mut cache, key_a.clone(), [0x10, 0, 0, 0xff]);
    let id_b = insert_one_pixel(&mut cache, key_b.clone(), [0x20, 0, 0, 0xff]);
    let id_c = insert_one_pixel(&mut cache, key_c.clone(), [0x30, 0, 0, 0xff]);
    assert!(
        cache.get(&key_a).is_some(),
        "读取 A 应命中并把它提升为最近使用项"
    );

    let generation = cache
        .begin_request(key_d.clone())
        .expect("未缓存的 D 应登记为新的预览请求");
    let preview_d = render_preview_from_rgba(image::RgbaImage::from_pixel(
        1,
        1,
        image::Rgba([0x40, 0, 0, 0xff]),
    ));
    let id_d = preview_d.image.id;
    let evicted = cache.complete_request(key_d.clone(), generation, preview_d);

    assert!(cache.get(&key_a).is_some(), "最近访问的 A 必须保留");
    assert!(cache.get(&key_c).is_some(), "未到淘汰边界的 C 必须保留");
    assert!(cache.get(&key_d).is_some(), "刚完成的 D 必须写入缓存");
    assert!(cache.get(&key_b).is_none(), "最久未使用的 B 必须被淘汰");
    assert_eq!(
        cache.total_bytes(),
        12,
        "淘汰 B 后 A+C+D = 4+4+4 = 12 bytes，必须回到预算内"
    );
    assert_eq!(
        evicted.len(),
        1,
        "16-byte 临时总量只超出 4 bytes，因此应只淘汰一张预览"
    );
    assert_eq!(
        evicted[0].id, id_b,
        "淘汰列表必须返回 B 的 RenderImage，供调用方显式释放 Metal texture"
    );
    assert_ne!(id_a, id_b, "A 与 B 必须是不同 RenderImage，避免断言退化");
    assert_ne!(id_c, id_b, "C 与 B 必须是不同 RenderImage，避免断言退化");
    assert_ne!(id_d, id_b, "D 与 B 必须是不同 RenderImage，避免断言退化");
}

/// 壁纸预览缓存关闭：清空已缓存图片并拒绝关闭前启动的后台结果。
///
/// 数据构造（含关键数值的推导过程）：
///   budget = 64 bytes
///   stale A = 1 × 1 × 4 = 4 bytes，在 generation 0 发起但尚未完成
///   ready B = 1 × 1 × 4 = 4 bytes，在 clear 后的新 generation 1 完成
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 generation 0 登记 A 请求，模拟用户立即关闭 Settings 并 clear。
///   2. 让 A 的旧后台结果完成，尝试写回缓存。
///   3. 在新 generation 登记并完成 B，确认缓存重新可用。
///   4. 再次 clear，收集应交给 GPUI 释放的 B RenderImage。
///
/// 预期结果：
///   - 正断言：旧 A 被拒绝并作为释放项返回，缓存保持 0 bytes。
///   - 正断言：新 B 可以写入，第二次 clear 恰好返回 B。
///   - 负断言：clear 后 A、B 均不得留在缓存，旧任务不得复活缓存。
#[test]
fn wallpaper_preview_cache_clear_rejects_stale_background_completion() {
    let mut cache = PreviewCache::with_budget(64);
    let key_a = PathBuf::from("stale-A.png");
    let generation_a = cache
        .begin_request(key_a.clone())
        .expect("generation 0 应允许登记 A 请求");

    let initially_released = cache.clear();
    assert!(
        initially_released.is_empty(),
        "A 尚未完成，第一次 clear 不应返回任何已缓存图片"
    );
    let stale_a = render_preview_from_rgba(image::RgbaImage::from_pixel(
        1,
        1,
        image::Rgba([0xaa, 0, 0, 0xff]),
    ));
    let stale_a_id = stale_a.image.id;
    let rejected = cache.complete_request(key_a.clone(), generation_a, stale_a);

    assert_eq!(
        rejected.len(),
        1,
        "关闭前启动的 A 完成后必须被拒绝并返回一张待释放图片"
    );
    assert_eq!(
        rejected[0].id, stale_a_id,
        "被拒绝的释放项必须是旧 generation 的 A RenderImage"
    );
    assert!(
        cache.get(&key_a).is_none(),
        "旧 generation 的 A 不得在 Settings 关闭后复活缓存"
    );
    assert_eq!(cache.total_bytes(), 0, "拒绝旧 A 后缓存必须保持 0 bytes");

    let key_b = PathBuf::from("fresh-B.png");
    let generation_b = cache
        .begin_request(key_b.clone())
        .expect("clear 后的新 generation 应允许登记 B 请求");
    let fresh_b = render_preview_from_rgba(image::RgbaImage::from_pixel(
        1,
        1,
        image::Rgba([0xbb, 0, 0, 0xff]),
    ));
    let fresh_b_id = fresh_b.image.id;
    let evicted = cache.complete_request(key_b.clone(), generation_b, fresh_b);
    assert!(
        evicted.is_empty(),
        "4-byte 的 B 在 64-byte 预算内，不应被 LRU 淘汰"
    );
    assert!(
        cache.get(&key_b).is_some(),
        "新 generation 的 B 必须成功写入"
    );

    let released = cache.clear();
    assert_eq!(released.len(), 1, "第二次 clear 应恰好释放已缓存的 B");
    assert_eq!(
        released[0].id, fresh_b_id,
        "第二次 clear 返回的必须是 B RenderImage"
    );
    assert!(
        cache.get(&key_a).is_none(),
        "第二次 clear 后旧 A 仍不得存在"
    );
    assert!(
        cache.get(&key_b).is_none(),
        "第二次 clear 后 B 不得继续存在"
    );
    assert_eq!(cache.total_bytes(), 0, "第二次 clear 后缓存必须归零");
}

fn insert_one_pixel(cache: &mut PreviewCache, key: PathBuf, rgba: [u8; 4]) -> gpui::ImageId {
    let generation = cache
        .begin_request(key.clone())
        .expect("未缓存的一像素预览应登记新请求");
    let preview = render_preview_from_rgba(image::RgbaImage::from_pixel(1, 1, image::Rgba(rgba)));
    let id = preview.image.id;
    let evicted = cache.complete_request(key, generation, preview);
    assert!(
        evicted.is_empty(),
        "前三张一像素预览仍在 12-byte 预算内，不应提前淘汰"
    );
    id
}
