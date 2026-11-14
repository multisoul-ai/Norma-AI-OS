use super::{compose_image_message, image_message_parts, parse_image_message, MAX_CHAT_IMAGES};

fn sample(ext: &str) -> String {
    format!("/tmp/app.mobile-images/ws/{}.{ext}", "ab".repeat(32))
}

/// 场景：幕僚沿用手机聊天的图片消息包装，界面只显示用户原文。
/// 数据构造：64 位小写哈希的 png 路径；正文为「看这张」；另有一条空正文。
/// 执行过程：compose 后 parse，并核对 JSON 路径不会被拆开。
/// 预期结果：正文与路径还原；协议外壳不出现在可见正文里。
#[test]
fn image_message_round_trips_visible_text_and_store_paths() {
    let path = sample("png");
    let packed = compose_image_message("看这张", &[path.clone()]).expect("应能打包");
    assert!(
        packed.contains(&format!("[\"{path}\"]")),
        "路径必须是 JSON 字符串，避免空格或括号被改写"
    );
    let (visible, paths) = parse_image_message(&packed).expect("应能解析");
    assert_eq!(visible, "看这张", "可见正文应去掉图片协议");
    assert_eq!(paths, vec![path.clone()], "应保留完整路径");
    assert!(
        !image_message_parts(&packed).0.contains("[[norma-images]]"),
        "展示层不能把协议原文当作用户正文"
    );

    let only = compose_image_message("", &[path.clone()]).expect("纯图片也应能打包");
    let (visible, paths) = parse_image_message(&only).expect("纯图片应能解析");
    assert_eq!(visible, "", "纯图片没有正文");
    assert_eq!(paths, vec![path], "纯图片仍要保留路径");
}

/// 场景：协议只接受既有上传目录里的 png/jpg，且单条不超过 4 张。
/// 数据构造：密码文件、上级目录、大写哈希、第 5 张图，以及普通句子。
/// 执行过程：分别 compose 或 parse。
/// 预期结果：非法路径和超额被拒绝；普通句子不会被当成图片消息。
#[test]
fn image_message_rejects_foreign_paths_and_plain_text() {
    let good = sample("png");
    assert_eq!(
        compose_image_message("x", &["/etc/passwd".into()]).expect_err("系统路径不能当图片"),
        "invalid_image"
    );
    let escaped = format!("/tmp/app.mobile-images/ws/../{}.png", "ab".repeat(32));
    assert!(
        compose_image_message("x", &[escaped]).is_err(),
        "路径穿越不能通过文件名检查"
    );
    let upper = format!("/tmp/app.mobile-images/ws/{}.png", "AB".repeat(32));
    assert!(
        compose_image_message("x", &[upper]).is_err(),
        "哈希必须是小写十六进制"
    );
    let too_many = vec![good.clone(); MAX_CHAT_IMAGES + 1];
    assert_eq!(
        compose_image_message("x", &too_many).expect_err("第 5 张应拒绝"),
        "too_many_images",
        "上限与手机聊天一致：4 张"
    );
    assert!(
        parse_image_message("普通消息 [[norma-images]]").is_none(),
        "普通正文里的标记不能被隐藏"
    );
    let (visible, paths) = image_message_parts("只要文字");
    assert_eq!(visible, "只要文字", "无协议时正文保持原样");
    assert!(paths.is_empty(), "无协议时不能变出图片");
}
