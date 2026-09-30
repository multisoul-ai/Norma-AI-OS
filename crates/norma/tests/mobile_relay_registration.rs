use norma::mobile::ServerIdentity;
use serde_json::json;
use std::io::Write;
use std::process::{Command, Stdio};

/// Mac 中继注册签名可由 Node 使用既有配对证书验证，不能跨挑战、部署或用途重用。
///
/// 数据构造：1 个临时安装身份；32 字节挑战 = 64 个十六进制字符；另一个挑战只改末字节。
/// 执行过程：Mac 签名 → Node 用公开证书验签 → 改挑战、audience 和用途前缀分别验签。
/// 预期结果：原注册成功，三个篡改均失败；输出仅含 Mac ID / 签名，无私钥和设备 token。
#[test]
fn registration_binds_challenge_deployment_and_purpose() {
    let identity = ServerIdentity::generate().expect("临时身份");
    let audience = "http://127.0.0.1:8787";
    let challenge = "ab".repeat(32);
    let reply = identity
        .relay_registration(audience, &challenge)
        .expect("签署中继注册");
    assert_eq!(reply["type"], "register", "签名只用于中继注册");
    assert_eq!(
        reply["machine_id"],
        identity.machine_id(),
        "注册必须沿用配对安装身份"
    );
    assert!(reply.get("token").is_none(), "不能输出手机设备令牌");
    assert!(reply.get("private_key").is_none(), "不能序列化安装私钥");
    let script = r#"
const {X509Certificate, verify} = require('node:crypto');
let input = '';
process.stdin.on('data', bytes => input += bytes);
process.stdin.on('end', () => {
  const value = JSON.parse(input), key = new X509Certificate(Buffer.from(value.certificate)).publicKey;
  const signature = Buffer.from(value.registration.signature, 'hex');
  const check = (prefix, audience, challenge) => verify('sha256',
    Buffer.from(prefix + audience + '\n' + challenge), key, signature);
  process.stdout.write(JSON.stringify([
    check('Norma relay registration v1\n', value.audience, value.challenge),
    check('Norma relay registration v1\n', value.audience, value.challenge.slice(0, -2) + 'ac'),
    check('Norma relay registration v1\n', 'https://other.example', value.challenge),
    check('Norma voice authorization v1\n', value.audience, value.challenge)
  ]));
});
"#;
    let mut child = Command::new("node")
        .args(["-e", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("启动本机 Node 验签，未调用网络模型");
    child
        .stdin
        .take()
        .expect("输入")
        .write_all(
            &serde_json::to_vec(&json!({"certificate":identity.certificate_der(),
                "registration":reply,"audience":audience,"challenge":challenge}))
            .expect("公开验证载荷"),
        )
        .expect("传入公开证书与签名");
    let output = child.wait_with_output().expect("Node 验签结束");
    assert!(output.status.success(), "跨语言验签必须实际运行成功");
    let outcomes: Vec<bool> = serde_json::from_slice(&output.stdout).expect("验签结果");
    assert_eq!(outcomes, [true, false, false, false], "仅原始转录可验证");
}

/// 仅对受控格式的中继挑战签名，避免签名接口变成任意文本签名器。
///
/// 数据构造：合法 32 字节挑战；63 字符短值 = 64 - 1；明文公网和带用户凭据的 URL。
/// 执行过程：分别提交不合规挑战、非回环 HTTP、URL 凭据、换行注入；最后提交生产 HTTPS。
/// 预期结果：所有非法输入失败，合法部署可签名；失败不影响安装身份后续使用。
#[test]
fn registration_rejects_unsafe_origins_and_malformed_challenges() {
    let identity = ServerIdentity::generate().expect("身份");
    let challenge = "cd".repeat(32);
    assert!(
        identity
            .relay_registration("https://relay.example", &challenge[..63])
            .is_err(),
        "短挑战不能签名"
    );
    assert!(
        identity
            .relay_registration("http://relay.example", &challenge)
            .is_err(),
        "公网明文不能成为中继部署"
    );
    assert!(
        identity
            .relay_registration("https://secret@relay.example", &challenge)
            .is_err(),
        "URL 不能包含凭据"
    );
    assert!(
        identity
            .relay_registration("https://relay.example\nother", &challenge)
            .is_err(),
        "用途绑定转录不得接受换行注入"
    );
    assert!(
        identity
            .relay_registration("https://soulvoiceai.com/norma/voice", &challenge)
            .is_ok(),
        "合法生产基址必须沿用既有证书签名"
    );
}
