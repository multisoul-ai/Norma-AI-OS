use norma::mobile::{PairingSession, ServerIdentity};
use std::time::{Duration, Instant};

/// 验证码每次签发全局最多三次交换；到期、取消、QR 认领都同时关闭验证码入口。
///
/// 数据构造：随机证书与一台回环电脑；3 次=单个 120s offer 的总尝试额度，120s 为精确到期边界。
/// 执行过程：用正确码解密同一个 QR → 再请求两次耗尽 → 第四次拒绝 → 取消拒绝 → 新 offer 到期拒绝 → 扫码认领后拒绝。
/// 预期结果：成功仅取回身份，不产生 pending/授权；错误入口与有效 QR 共享生命周期，不能靠换连接重置次数。
#[test]
fn short_code_shares_offer_lifetime_and_bounded_attempts() {
    let identity = ServerIdentity::generate().expect("隔离身份");
    let now = Instant::now();
    let make = || {
        PairingSession::new(
            &identity,
            "Mac",
            vec!["wss://127.0.0.1:9443/mobile".into()],
            now,
        )
        .expect("签发")
    };
    let (mut session, qr) = make();
    let (client, message) = mobile_pairing::Client::start(qr.pairing_code()).expect("显示的验证码");
    let reply = session
        .exchange_code(&hex::encode(&message), now)
        .expect("验证码交换");
    let decrypted = client
        .finish(
            &hex::decode(&reply.message).expect("消息"),
            &hex::decode(&reply.sealed).expect("密文"),
        )
        .expect("认证成功");
    assert_eq!(
        decrypted,
        qr.to_json().expect("QR").as_bytes(),
        "必须取回同一证书和一次性凭据"
    );
    assert!(
        session.pending(now).is_none(),
        "验证六位码不能自动发起或批准配对"
    );
    assert!(
        session.exchange_code(&hex::encode(&message), now).is_ok(),
        "第二次交换有额度"
    );
    assert!(
        session.exchange_code(&hex::encode(&message), now).is_ok(),
        "第三次交换有额度"
    );
    assert!(
        session.exchange_code(&hex::encode(&message), now).is_err(),
        "第四次必须锁定，即使密码正确"
    );
    session.cancel();
    assert!(
        session.exchange_code(&hex::encode(&message), now).is_err(),
        "取消不恢复次数"
    );
    let (mut expired, _) = make();
    assert!(
        expired
            .exchange_code(&hex::encode(&message), now + Duration::from_secs(120))
            .is_err(),
        "120秒精确失效"
    );
    let (mut claimed, qr) = make();
    let value: serde_json::Value = serde_json::from_str(&qr.to_json().expect("QR")).expect("JSON");
    let request = serde_json::from_value(serde_json::json!({"offer_id":value["offer_id"],"secret":value["secret"],"device_id":"test-phone","device_name":"iPhone","device_token":"a".repeat(64)})).expect("请求");
    claimed.claim(request, now).expect("扫码认领");
    assert!(
        claimed.exchange_code(&hex::encode(&message), now).is_err(),
        "扫码认领后验证码同时失效"
    );
}
