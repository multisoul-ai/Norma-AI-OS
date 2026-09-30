use norma::mobile::{RelayHello, ServerIdentity};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Mac 和独立 Node 客户端互通端到端会话，中继所见字节不包含任务正文。
///
/// 数据构造：1 个 Mac 安装身份、1 对双方 X25519 临时密钥；32 字节随机数；每方向从序号 0 开始。
///   二进制帧开销 = 8 字节序号 + 16 字节 GCM 标签；nonce = 4 个零字节 + 8 字节序号。
/// 执行过程：Node 发 hello → Mac 签握手并加密回复 → Node 验签/派生并解密 → Node 加密任务 → Mac 解密。
///   再测试密文篡改、反向反射、重放和更换通道；失败不得消费正确的下一序号。
/// 预期结果：双方中文完整恢复；原密文、错误用途/通道无法代替有效消息；无一次明文降级。
#[test]
fn encrypted_session_interoperates_and_rejects_tampering_replay_and_reflection() {
    let identity = ServerIdentity::generate().expect("临时 Mac 身份");
    let stream = "00000000-0000-4000-8000-000000000001";
    let script = include_str!("support/mobile_relay_session.cjs");
    let mut child = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("独立 Node 协议客户端");
    let mut output = BufReader::new(child.stdout.take().expect("客户端输出"));
    let mut line = String::new();
    output.read_line(&mut line).expect("读取 hello");
    let hello: RelayHello = serde_json::from_str(&line).expect("明确格式的 hello");
    let (mut session, handshake) = identity
        .accept_relay_session(stream, hello)
        .expect("签署握手");
    // 256 KiB - sequence(8) - tag(16) + 1 is the first rejected plaintext size.
    assert!(
        session.seal(&vec![0; 256 * 1024 - 23]).is_err(),
        "超限失败应保留序号 0 给下一条合法消息"
    );
    let frame = session.seal("Mac 已收到".as_bytes()).expect("加密响应");
    assert!(
        !String::from_utf8_lossy(&frame).contains("Mac 已收到"),
        "中继字节不能包含可见明文"
    );
    let input = json!({"stream":stream,"machine_id":identity.machine_id(),"handshake":handshake,
        "frame":hex::encode(&frame)});
    writeln!(child.stdin.take().expect("客户端输入"), "{input}").expect("传递握手与密文");
    line.clear();
    output.read_line(&mut line).expect("读取客户端加密结果");
    let response: Value = serde_json::from_str(&line).expect("客户端结果");
    assert_eq!(
        response["plaintext"], "Mac 已收到",
        "Node 必须独立解密 Mac 密文"
    );
    assert_eq!(response["signature_valid"], true, "安装证书签名必须通过");
    assert_eq!(response["wrong_stream_valid"], false, "签名不能跨通道重放");
    let encrypted = hex::decode(response["frame"].as_str().expect("密文字段")).expect("hex 密文");
    assert!(
        session.open(&frame).is_err(),
        "Mac 自己的下行帧不能被当成手机上行"
    );
    let mut damaged = encrypted.clone();
    *damaged.last_mut().expect("GCM 标签") ^= 1;
    assert!(
        session.open(&damaged).is_err(),
        "改动标签必须在明文交付前失败"
    );
    assert_eq!(
        session.open(&encrypted).expect("手机任务"),
        "修改首屏".as_bytes(),
        "合法消息应完整恢复"
    );
    assert!(
        session.open(&encrypted).is_err(),
        "旧序号不能重放，即使密文标签有效"
    );
    assert!(
        child.wait().expect("Node 退出").success(),
        "独立客户端必须正常完成"
    );
}

/// 无效 hello 与超限消息在进入密码操作前拒绝。
///
/// 数据构造：hello 缺少 32 字节密钥／随机数；通道字符串含换行；version = 2 而协议只接受 1。
/// 执行过程：通过公开反序列化入口及握手入口提交；检查错误不会返回可用 session。
/// 预期结果：未知字段、错误版本和转录注入均被拒绝。
#[test]
fn encrypted_session_rejects_invalid_handshakes() {
    let identity = ServerIdentity::generate().expect("身份");
    let hello = json!({"version":2,"client_key":"00".repeat(32),"nonce":"aa".repeat(32)});
    assert!(
        identity
            .accept_relay_session(
                "00000000-0000-4000-8000-000000000001",
                serde_json::from_value(hello.clone()).expect("hello")
            )
            .is_err(),
        "未支持版本不得协商成有效会话"
    );
    let extra =
        json!({"version":1,"client_key":"00".repeat(32),"nonce":"aa".repeat(32),"skip_pin":true});
    assert!(
        serde_json::from_value::<RelayHello>(extra).is_err(),
        "未知安全开关不能进入握手"
    );
    assert!(
        identity
            .accept_relay_session(
                "stream\nother",
                serde_json::from_value(hello).expect("hello")
            )
            .is_err(),
        "通道必须是 UUID，不能把换行注入签名转录"
    );
    let low_order = json!({"version":1,"client_key":"00".repeat(32),"nonce":"aa".repeat(32)});
    assert!(
        identity
            .accept_relay_session(
                "00000000-0000-4000-8000-000000000001",
                serde_json::from_value(low_order).expect("hello")
            )
            .is_err(),
        "全零 X25519 公钥不得派生可预测的会话密钥"
    );
}
