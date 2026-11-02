use crate::image_store;
use image::{ImageFormat, Rgb, RgbImage};
use sha2::{Digest, Sha256};
use std::io::Cursor;

fn png() -> Vec<u8> {
    let image = RgbImage::from_pixel(2, 2, Rgb([7, 19, 31]));
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

/// 场景：手机附件必须通过真实图片校验并在私有目录中原样保存和读取。
/// 构造：2×2 PNG以及正文SHA-256地址；隔离根目录/项目目录均由实际store创建。
/// 执行：验证、保存、读取同一图片，随后破坏正文再次读取。
/// 预期：首轮字节完全相同；篡改后不能返回正文，也不能把坏图当作成功附件。
#[test]
fn image_roundtrip_then_tampering_is_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("attachments").join("project");
    let bytes = png();
    assert_eq!(
        image_store::validate(&bytes),
        Ok("png"),
        "真实PNG应通过解码和尺寸校验"
    );
    let id = format!("{:x}", Sha256::digest(&bytes));
    let path = image_store::save(&folder, &id, "png", &bytes).unwrap();
    let (_, loaded) = image_store::read(&folder, &id).unwrap();
    assert_eq!(loaded, bytes, "读回必须等于原附件，不能截断或替换");
    std::fs::write(path, b"tampered").unwrap();
    assert_eq!(
        image_store::read(&folder, &id).unwrap_err(),
        "invalid_image",
        "哈希地址不能返回被替换的正文"
    );
}

/// 场景：附件父目录被普通文件占据时必须拒绝，不得覆盖用户文件。
/// 构造：attachments路径写入固定标记，项目目录尚不存在。
/// 执行：实际save尝试建目录。
/// 预期：明确存储错误且标记不变，不留下部分附件。
#[test]
fn image_parent_file_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("attachments");
    std::fs::write(&parent, b"keep").unwrap();
    let bytes = png();
    let id = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(
        image_store::save(&parent.join("project"), &id, "png", &bytes).unwrap_err(),
        "image_storage_unavailable",
        "非目录父路径必须拒绝"
    );
    assert_eq!(std::fs::read(parent).unwrap(), b"keep", "不得覆盖原文件");
}

/// 场景：读取超过2MiB的缓存正文必须拒绝，即使文件名匹配其真实哈希。
/// 构造：实际LIMIT+1字节，超过上限仅1字节，不需要解码大图片。
/// 执行：把正文放入真实项目目录并调用实际读取入口。
/// 预期：invalid_image，不能因内容寻址正确而绕过资源上限。
#[test]
fn oversized_image_file_cannot_bypass_read_limit() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("attachments").join("project");
    std::fs::create_dir_all(&folder).unwrap();
    let bytes = vec![3; image_store::LIMIT + 1];
    let id = format!("{:x}", Sha256::digest(&bytes));
    std::fs::write(folder.join(format!("{id}.png")), bytes).unwrap();
    assert_eq!(
        image_store::read(&folder, &id).unwrap_err(),
        "invalid_image",
        "超过实际协议上限的文件必须拒绝"
    );
}
