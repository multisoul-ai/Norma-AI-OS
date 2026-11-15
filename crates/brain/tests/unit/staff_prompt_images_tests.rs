use super::{attachments, base64};
use norma::protocol::{compose_image_message, Task};
use sha2::{Digest, Sha256};
use std::fs;

const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xde, 0x00, 0x00, 0x00, 0x0c, 0x49, 0x44, 0x41, 0x54, 0x08, 0xd7, 0x63, 0xf8, 0xcf, 0xc0, 0x00,
    0x00, 0x00, 0x03, 0x00, 0x01, 0x18, 0xdd, 0x8d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e,
    0x44, 0xae, 0x42, 0x60, 0x82,
];

/// 场景：最近一条用户消息里的上传图片要变成 ACP 图片块；坏路径不能读进提示词。
/// 数据构造：1×1 PNG 的标准字节，文件名为其 SHA-256；另一条消息指向不存在的哈希。
/// 数值：PNG 文件头 8 字节为 89 50 4E 47 0D 0A 1A 0A。单张上限 2 MiB，本图远小于该值。
/// 执行过程：写入临时 `.mobile-images` 文件，解析任务附件。
/// 预期结果：成功的图进入 image/png 块且 base64 与字节一致；缺失文件只留下 invalid_image。
#[test]
fn latest_user_image_becomes_a_prompt_block_when_the_hash_matches() {
    let dir = tempfile::tempdir().expect("临时目录");
    let id = format!("{:x}", Sha256::digest(PNG));
    let path = dir.path().join(format!("app.mobile-images/ws/{id}.png"));
    fs::create_dir_all(path.parent().expect("父目录")).expect("创建上传目录");
    fs::write(&path, PNG).expect("写入图片");
    let stored = path.to_str().expect("路径应为 utf-8").to_owned();
    let mut task = Task::new("t", "w", "p", "图");
    task.user_message(&compose_image_message("看logo", &[stored.clone()]).expect("打包"));
    let attached = attachments(&task);
    assert!(attached.missing.is_empty(), "匹配哈希的文件不应报缺失");
    assert_eq!(attached.blocks.len(), 1, "只附最近一条里的一张图");
    assert_eq!(
        attached.blocks[0].mime, "image/png",
        "png 魔数应对应 image/png"
    );
    assert_eq!(
        attached.blocks[0].data,
        base64(PNG),
        "块内容必须是原字节的 base64"
    );

    let missing = dir
        .path()
        .join(format!("app.mobile-images/ws/{}.png", "cd".repeat(32)));
    let missing = missing.to_str().expect("utf-8").to_owned();
    task.user_message(&compose_image_message("", &[missing]).expect("打包缺失路径"));
    let attached = attachments(&task);
    assert!(attached.blocks.is_empty(), "读不到的图片不能变成内容块");
    assert_eq!(
        attached.missing,
        vec!["invalid_image"],
        "缺失要给出明确原因，而不是静默丢掉"
    );
}

/// 场景：base64 填充必须让模型收到可解码的图片。
/// 数据构造：1、2、3 字节以及上面的 PNG。
/// 执行过程：调用与上传块相同的编码器。
/// 预期结果：标准向量 Zg== / Zm8= / Zm9v；PNG 编码后能按 4 字符对齐。
#[test]
fn base64_padding_matches_the_standard_alphabet() {
    assert_eq!(base64(b"f"), "Zg==", "1 字节应补两个等号");
    assert_eq!(base64(b"fo"), "Zm8=", "2 字节应补一个等号");
    assert_eq!(base64(b"foo"), "Zm9v", "3 字节不需要填充");
    assert_eq!(base64(PNG).len() % 4, 0, "图片 base64 长度必须是 4 的倍数");
}
