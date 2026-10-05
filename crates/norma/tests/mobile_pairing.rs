use norma::mobile::{PairingRequest, PairingSession, ServerIdentity};
use std::time::{Duration, Instant};

/// 手机已认领的配对请求不随二维码到期，Mac 核对并批准后才完成授权。
///
/// 数据构造：二维码有效期 120s；手机在 120 - 1 = 119s 认领；
///   在 120 + 1 = 121s 查询待审批，在 119 + 120 + 2 = 241s 批准。
///   phone-wait 使用独立的 64 位 hex token，不复用二维码 secret。
/// 执行过程：生成二维码 → 临近到期认领 → 到期后查询 → 拒绝错误请求号
///   → 等待超过 120s 后准确批准 → 再次批准被拒绝。
/// 预期结果：原设备及核对码始终保留；错误请求不登记、不删除待审批；
///   只有一次准确的本地批准能够登记，不能因等待人工核对报过期。
#[test]
fn claimed_phone_can_be_approved_after_qr_expiry() {
    let identity = ServerIdentity::generate().expect("隔离 TLS 身份");
    let issued = Instant::now();
    let (mut pairing, qr) = PairingSession::new(
        &identity,
        "Mac",
        vec!["wss://127.0.0.1:9443/mobile".into()],
        issued,
    )
    .expect("生成二维码");
    let offer: serde_json::Value =
        serde_json::from_str(&qr.to_json().expect("二维码 JSON")).expect("解析二维码");
    let request = serde_json::from_value(serde_json::json!({
        "offer_id": offer["offer_id"], "secret": offer["secret"],
        "device_id": "phone-wait", "device_name": "Waiting iPhone", "device_token": "a".repeat(64)
    }))
    .expect("手机请求");
    // 119 = 二维码有效期 120 - 1，在入口关闭之前完成认领。
    let pending = pairing
        .claim(request, issued + Duration::from_secs(119))
        .expect("到期前认领");
    // 121 = 二维码有效期 120 + 1，二维码应已不能再被使用。
    let after_expiry = issued + Duration::from_secs(121);
    let visible = pairing
        .pending(after_expiry)
        .expect("二维码到期不能删除已认领的待审批手机");
    assert_eq!(visible.device_id, "phone-wait", "待审批必须保留准确手机");
    assert_eq!(
        visible.verification_code, pending.verification_code,
        "等待不能改变双端核对码"
    );
    assert!(
        pairing
            .approve::<()>("wrong-request", after_expiry, |_, _, _| {
                panic!("错误请求号不得触发登记")
            })
            .is_err(),
        "二维码到期后仍须拒绝错误的本地请求号"
    );
    // 241 = 认领时刻 119 + 原等待预算 120 + 2，人工核对不再受该预算限制。
    let approved_at = issued + Duration::from_secs(241);
    pairing
        .approve(&pending.request_id, approved_at, |id, name, token| {
            assert_eq!(id, "phone-wait", "只能登记已认领手机");
            assert_eq!(name, "Waiting iPhone", "只能登记已核对设备名");
            assert_eq!(token, "a".repeat(64), "不能登记二维码 secret");
            Ok(())
        })
        .expect("人工核对后的准确批准不能报二维码过期");
    assert!(
        pairing.pending(approved_at).is_none(),
        "批准必须清除待审批请求"
    );
    assert!(
        pairing
            .approve(&pending.request_id, approved_at, |_, _, _| Ok(()))
            .is_err(),
        "已完成授权不能再次登记"
    );
}

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
