use norma::mobile::{PairingRequest, PairingSession, ServerIdentity};
use std::time::{Duration, Instant};

/// 扫码只提出配对申请；Mac 授权前不得拿到可登记凭据，一次二维码只能认领一次。
///
/// 数据构造：独立 TLS 身份；120s 二维码，64 hex 字符的手机独立 token；过期边界 = 签发时刻 + 120s。
/// 执行过程：错误二维码凭据 → 正确认领 → 重复认领 → 错误本地请求号 → 本地授权落盘失败 → 重试成功。
/// 预期结果：错凭据/重复/错请求号不泄露凭据；只有准确本地批准调用保存；失败可重试，成功不可重复批准。
#[test]
fn scan_requires_exact_desktop_approval_and_is_single_use() {
    let identity = ServerIdentity::generate().expect("身份");
    let now = Instant::now();
    let (mut pairing, qr) = PairingSession::new(
        &identity,
        "MacBook Pro",
        vec!["wss://192.168.1.2:9443/mobile".into()],
        now,
    )
    .expect("签发");
    let qr_json = qr.to_json().expect("二维码");
    let payload: serde_json::Value = serde_json::from_str(&qr_json).expect("QR JSON");
    let token = payload["secret"].as_str().expect("QR secret");
    assert!(!format!("{qr:?}").contains(token), "调试不能输出一次性凭据");
    let make = |secret: &str| {
        serde_json::from_value::<PairingRequest>(serde_json::json!({
        "offer_id": payload["offer_id"], "secret":secret, "device_id":"phone-a", "device_name":"iPhone", "device_token":"a".repeat(64)
    })).expect("手机请求")
    };
    assert!(
        pairing.claim(make(&"0".repeat(64)), now).is_err(),
        "错误 QR secret 不能消耗授权"
    );
    let pending = pairing.claim(make(token), now).expect("正确扫码");
    assert_eq!(pending.device_name, "iPhone", "Mac 显示准确设备名");
    assert_eq!(
        pending.verification_code.len(),
        6,
        "6 位仅用于双端核对，不是认证密钥"
    );
    assert!(
        pairing.claim(make(token), now).is_err(),
        "二维码已认领，不能置换设备"
    );
    assert!(
        pairing
            .approve("wrong-request", now, |_, _, _| Ok(()))
            .is_err(),
        "本地审批必须指向准确申请"
    );
    assert!(
        pairing
            .approve::<()>(&pending.request_id, now, |_, _, _| anyhow::bail!(
                "模拟存储失败"
            ))
            .is_err(),
        "落盘失败不能报告配对成功"
    );
    let saved = pairing
        .approve(&pending.request_id, now, |id, name, credential| {
            assert_eq!(id, "phone-a", "登记已认领的准确设备");
            assert_eq!(name, "iPhone", "登记已核对名称");
            assert_eq!(
                credential,
                "a".repeat(64),
                "登记手机预存在 Keychain 的独立凭据"
            );
            Ok("stored")
        })
        .expect("Mac 批准");
    assert_eq!(saved, "stored", "只返回真实持久化结果");
    assert!(
        pairing
            .approve(&pending.request_id, now, |_, _, _| Ok(()))
            .is_err(),
        "成功后不能再次注册"
    );
    let (mut expired, expired_qr) = PairingSession::new(
        &identity,
        "Mac",
        vec!["wss://mac.tail.ts.net:9443/mobile".into()],
        now,
    )
    .expect("第二张 QR");
    let q: serde_json::Value =
        serde_json::from_str(&expired_qr.to_json().expect("JSON")).expect("解析");
    let req = serde_json::from_value(serde_json::json!({"offer_id":q["offer_id"],"secret":q["secret"],"device_id":"phone-b","device_name":"iPhone B","device_token":"b".repeat(64)})).expect("B 请求");
    assert!(
        expired.claim(req, now + Duration::from_secs(120)).is_err(),
        "二维码在 120 秒边界失效"
    );
}
