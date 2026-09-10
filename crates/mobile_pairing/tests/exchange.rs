use mobile_pairing::{new_code, respond, Client};

/// 六位验证码通过 PAKE 保护电脑身份；错误密码、篡改和跨会话重放不能读出配对凭据。
///
/// 数据构造：012345 含前导零；错误码 012346；明文为测试 offer；33 字节为 SPAKE2 公共消息长度。
/// 执行过程：正确交换并解密 → 用错码解密 → 翻转密文 → 将上一会话响应用于新客户端 → 拒绝非六位输入。
/// 预期结果：只有正确会话拿到原文；公共消息不含验证码；其他路径不能返回配对信息。
#[test]
fn code_authenticates_offer_and_rejects_tampering_and_replay() {
    let pin = "012345";
    let payload = b"test-offer-with-pinned-certificate";
    let (client, message) = Client::start(pin).expect("六位验证码");
    let reply = respond(pin, &message, payload).expect("桌面响应");
    assert_eq!(message.len(), 33, "只发送标准 SPAKE2 公共消息");
    assert!(
        !message.windows(6).any(|w| w == pin.as_bytes()),
        "不能发送明文验证码"
    );
    assert_eq!(
        client
            .finish(&reply.message, &reply.sealed)
            .expect("正确解密"),
        payload,
        "认证后才返回电脑身份"
    );
    let (wrong, message) = Client::start("012346").expect("错误但格式合法");
    let reply = respond(pin, &message, payload).expect("不泄露密码是否匹配");
    assert!(
        wrong.finish(&reply.message, &reply.sealed).is_err(),
        "错码不得解密"
    );
    let (tampered, message) = Client::start(pin).expect("新会话");
    let mut reply = respond(pin, &message, payload).expect("响应");
    reply.sealed[12] ^= 1; // 前 12 字节是随机 nonce，翻转其后第一个密文字节。
    assert!(
        tampered.finish(&reply.message, &reply.sealed).is_err(),
        "篡改必须被 AEAD 拒绝"
    );
    let (original, message) = Client::start(pin).expect("原会话");
    let reply = respond(pin, &message, payload).expect("原响应");
    let (replay, _) = Client::start(pin).expect("另一会话");
    assert!(
        replay.finish(&reply.message, &reply.sealed).is_err(),
        "不能跨会话重放"
    );
    assert!(
        original.finish(&reply.message, &reply.sealed).is_ok(),
        "同一原会话仍可认证"
    );
    assert!(Client::start("12345").is_err(), "五位数字无效");
    assert!(Client::start("１２３４５６").is_err(), "拒绝非 ASCII 数字");
    assert!(
        respond(pin, &[0; 34], payload).is_err(),
        "33+1 字节消息超过固定上限"
    );
    let generated = new_code().expect("系统安全随机数");
    assert_eq!(generated.len(), 6, "桌面显示固定六位数");
    assert!(
        generated.bytes().all(|c| c.is_ascii_digit()),
        "验证码仅含数字"
    );
}
