use super::*;
use canvas::{AgentProfile, AgentSpec, CanvasNodeKind, CanvasRect, RectNode};
use gpui::{ClipboardEntry, ClipboardString, Image, ImageFormat};
use runtime::Runtime;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::time::{Duration, SystemTime};

fn test_image(format: ImageFormat, bytes: &[u8]) -> Image {
    Image::from_bytes(format, bytes.to_vec())
}

/// 图片格式映射：剪切板图片应保存为与原始编码匹配的扩展名。
///
/// 数据构造（含关键数值的推导过程）：
///   supported formats = 7（PNG/JPEG/WebP/GIF/SVG/BMP/TIFF）
///   每个 format 只映射 1 个小写 ASCII 扩展名，避免路径注入和 shell quoting 需求
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 对每个 GPUI ImageFormat 调用 clipboard_image_extension
///   2. 检查返回扩展名是否精确匹配保存文件名预期
///   3. 检查 PNG/JPEG 不会互相误映射
///
/// 预期结果：
///   - 正断言：全部 7 种格式都有稳定扩展名
///   - 负断言：PNG 不应保存为 jpg，JPEG 不应保存为 png
#[test]
fn test_clipboard_image_extension_matches_original_format() {
    assert_eq!(
        clipboard_image_extension(ImageFormat::Png),
        "png",
        "PNG 剪切板图片应保存为 .png"
    );
    assert_eq!(
        clipboard_image_extension(ImageFormat::Jpeg),
        "jpg",
        "JPEG 剪切板图片应保存为常见 .jpg 扩展名"
    );
    assert_eq!(
        clipboard_image_extension(ImageFormat::Webp),
        "webp",
        "WebP 剪切板图片应保存为 .webp"
    );
    assert_eq!(
        clipboard_image_extension(ImageFormat::Gif),
        "gif",
        "GIF 剪切板图片应保存为 .gif"
    );
    assert_eq!(
        clipboard_image_extension(ImageFormat::Svg),
        "svg",
        "SVG 剪切板图片应保存为 .svg"
    );
    assert_eq!(
        clipboard_image_extension(ImageFormat::Bmp),
        "bmp",
        "BMP 剪切板图片应保存为 .bmp"
    );
    assert_eq!(
        clipboard_image_extension(ImageFormat::Tiff),
        "tiff",
        "TIFF 剪切板图片应保存为 .tiff"
    );
    assert_ne!(
        clipboard_image_extension(ImageFormat::Png),
        "jpg",
        "PNG 不应误保存为 JPEG 扩展名"
    );
    assert_ne!(
        clipboard_image_extension(ImageFormat::Jpeg),
        "png",
        "JPEG 不应误保存为 PNG 扩展名"
    );
}

/// Agent-only 目标判断：只有 Agent 卡片应把图片剪切板转换为路径粘贴。
///
/// 数据构造（含关键数值的推导过程）：
///   agent node_id = 1，CanvasNodeKind::Agent(Codex)
///   rect node_id  = 2，CanvasNodeKind::Rect
///   missing id    = 99，不存在于 document
///   共 3 个候选目标，其中仅 1 个应启用图片路径粘贴
///
/// 执行过程（逐步说明系统如何处理）：
///   1. Runtime document 加入 Agent 与 Rect 节点
///   2. 分别调用 accepts_clipboard_image_path
///   3. 检查非 Agent 和失效 id 都不会触发图片落盘
///
/// 预期结果：
///   - 正断言：Agent 节点返回 true
///   - 负断言：Rect 节点返回 false
///   - 负断言：不存在节点返回 false
#[test]
fn test_accepts_clipboard_image_path_is_agent_only() {
    let mut runtime = Runtime::new();
    let agent = runtime.document.add_node(
        CanvasNodeKind::Agent(AgentSpec {
            profile: AgentProfile::Codex,
            alias: None,
        }),
        CanvasRect::new(0.0, 0.0, 100.0, 80.0),
    );
    let rect = runtime.document.add_node(
        CanvasNodeKind::Rect(RectNode { color: 0x4080ff }),
        CanvasRect::new(120.0, 0.0, 100.0, 80.0),
    );

    assert!(
        accepts_clipboard_image_path(&runtime, agent),
        "Agent 节点应启用图片剪切板转路径粘贴"
    );
    assert!(
        !accepts_clipboard_image_path(&runtime, rect),
        "普通 Rect 节点不应启用图片剪切板转路径粘贴"
    );
    assert!(
        !accepts_clipboard_image_path(&runtime, 99),
        "不存在的节点不应触发图片落盘或路径粘贴"
    );
}

/// GPUI 剪切板 entries 选择：含文本和图片时应优先取图片。
///
/// 数据构造（含关键数值的推导过程）：
///   image bytes = [0x89, b'P', b'N', b'G']，长度 4
///   text item   = "https://example.test/image.png"，长度 30
///   entries     = 2，其中 Image 位于文本之后，验证不是简单取 first
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造含 String + Image 的 ClipboardItem
///   2. 调用 image_from_clipboard_item
///   3. 检查返回图片格式和字节，而不是返回 None 或文本
///
/// 预期结果：
///   - 正断言：返回 PNG 图片，字节精确保留
///   - 负断言：返回值不应为空
#[test]
fn test_image_from_clipboard_item_prefers_image_over_text() {
    let image = test_image(ImageFormat::Png, b"\x89PNG");
    let entries = vec![
        ClipboardEntry::String(ClipboardString::new(
            "https://example.test/image.png".to_string(),
        )),
        ClipboardEntry::Image(image.clone()),
    ];

    let selected = image_from_clipboard_entries(&entries);

    assert!(
        selected.is_some(),
        "含图片的剪切板项应返回图片，即使同时存在文本"
    );
    let selected = selected.expect("image should be selected");
    assert_eq!(
        selected.format(),
        ImageFormat::Png,
        "应选择 PNG 图片格式，而不是文本 fallback"
    );
    assert_eq!(
        selected.bytes(),
        b"\x89PNG",
        "图片 bytes 应精确保留，供后续原样写入临时文件"
    );
}

/// 图片落盘：保存路径应位于系统临时图片目录，文件名安全且内容精确。
///
/// 数据构造（含关键数值的推导过程）：
///   temp root = tempfile 目录
///   now = UNIX_EPOCH + 1_700_000_000s（固定时间戳，避免测试随当前时间变化）
///   image bytes = 4 bytes: [1,2,3,4]
///   expected dir = temp_root / "multisoul-clipboard-images"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. save_clipboard_image_to_temp_root(temp_root, image, now)
///   2. 读取返回 path 和文件内容
///   3. 检查文件名只包含 ASCII 安全字符，并以 .png 结尾
///
/// 预期结果：
///   - 正断言：文件在 multisoul-clipboard-images 目录内
///   - 正断言：文件内容等于原始 bytes
///   - 正断言：注入文本为绝对路径本身
///   - 负断言：文件名不包含空格或引号
#[test]
fn test_save_clipboard_image_to_temp_root_writes_safe_absolute_path() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let image = test_image(ImageFormat::Png, &[1, 2, 3, 4]);

    let saved =
        save_clipboard_image_to_temp_root(temp.path(), &image, now).expect("image should be saved");

    assert!(
        saved
            .path
            .starts_with(temp.path().join(CLIPBOARD_IMAGE_DIR_NAME)),
        "图片应写入专用临时目录，实际路径: {:?}",
        saved.path
    );
    assert!(
        saved.path.is_absolute(),
        "返回给 Agent 的图片路径必须是绝对路径，实际: {:?}",
        saved.path
    );
    assert_eq!(
        fs::read(&saved.path).expect("saved image should be readable"),
        vec![1, 2, 3, 4],
        "落盘图片 bytes 应与剪切板原始 bytes 完全一致"
    );
    assert_eq!(
        saved.text,
        saved.path.to_string_lossy(),
        "注入 Agent 的文本应是绝对路径本身，不追加说明或回车"
    );
    let filename = saved
        .path
        .file_name()
        .and_then(|name| name.to_str())
        .expect("filename should be utf-8");
    assert!(
        filename.ends_with(".png"),
        "PNG 图片文件名应以 .png 结尾，实际: {filename}"
    );
    assert!(
        !filename.contains(' ') && !filename.contains('"') && !filename.contains('\''),
        "文件名应避免空格和引号，实际: {filename}"
    );
}

/// 临时图片清理：只删除超过 24 小时的本目录文件。
///
/// 数据构造（含关键数值的推导过程）：
///   max_age = 24h = 86_400s
///   now = UNIX_EPOCH + 200_000s
///   old.png modified = now - 86_401s → 超过 24h，应删除
///   fresh.png modified = now - 86_399s → 未超过 24h，应保留
///   missing metadata 模拟错误 1 次 → 应跳过，不影响其他文件
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在临时目录创建 old.png、fresh.png、unknown.png
///   2. 注入 modified_at 函数返回固定修改时间或错误
///   3. cleanup_expired_clipboard_images 删除过期文件
///
/// 预期结果：
///   - 正断言：old.png 被删除
///   - 正断言：fresh.png 保留
///   - 负断言：metadata 错误不应导致 fresh.png 被误删
#[test]
fn test_cleanup_expired_clipboard_images_removes_only_older_than_one_day() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let old = temp.path().join("old.png");
    let fresh = temp.path().join("fresh.png");
    let unknown = temp.path().join("unknown.png");
    fs::write(&old, b"old").expect("old file should be created");
    fs::write(&fresh, b"fresh").expect("fresh file should be created");
    fs::write(&unknown, b"unknown").expect("unknown file should be created");

    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(200_000);
    let mut modified = HashMap::new();
    modified.insert("old.png".to_string(), now - Duration::from_secs(86_401));
    modified.insert("fresh.png".to_string(), now - Duration::from_secs(86_399));

    let removed = cleanup_expired_clipboard_images(temp.path(), now, |entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        modified
            .get(&name)
            .copied()
            .ok_or_else(|| io::Error::new(io::ErrorKind::Other, "missing test metadata"))
    })
    .expect("cleanup should continue despite per-file metadata errors");

    assert_eq!(removed, 1, "清理应只删除超过 24 小时的 old.png");
    assert!(!old.exists(), "old.png 修改时间超过 24 小时，应被删除");
    assert!(fresh.exists(), "fresh.png 修改时间未超过 24 小时，应保留");
    assert!(
        unknown.exists(),
        "读取 metadata 失败的文件应跳过，不应被误删"
    );
}
