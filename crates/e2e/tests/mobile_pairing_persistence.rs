use app::MobileAccess;
use norma::mobile::{PairingRequest, ServerIdentity};
use std::time::{Duration, Instant};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// 从扫码到 Mac 项目授权、手机认证和撤销形成真实 SQLite 闭环，未批准/过期申请不能留下设备。
///
/// 数据构造：两台 Mac 的隔离 AppStore，A 含真实目录项目；测试 TLS 身份与手机 token 均独立。
/// 执行过程：扫码 → 认证被拒 → 授权不存在项目被拒 → Mac 授予真实项目只读 → 认证 → 降权/撤销 → 重新打开。
/// 预期结果：仅 A 上精确项目可读、不可控制；B 和错误电脑 ID 不可认证；撤销后旧 token 无效，过期申请无部分写入。
#[test]
fn mobile_pairing_authorization_survives_reopen_without_cross_machine_access() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let a = AppStore::new(dir.path().join("mac-a.sqlite"));
    let b = AppStore::new(dir.path().join("mac-b.sqlite"));
    let project = a
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Norma")
        .expect("稳定项目身份");
    let identity = ServerIdentity::generate().expect("测试证书");
    let machine_id = identity.machine_id();
    let mut access = MobileAccess::with_identity_for_e2e(a.clone(), identity);
    let other = MobileAccess::with_identity_for_e2e(b, ServerIdentity::generate().expect("B 身份"));
    let now = Instant::now();
    let qr = access
        .begin_pairing("Mac", vec!["wss://192.168.1.2:9443/mobile".into()], now)
        .expect("二维码");
    let request = make_request(&qr.to_json().expect("QR"), "phone-a", 'a');
    let pending = access.claim(request, now).expect("扫码");
    let token = "a".repeat(64);
    assert!(
        access
            .authenticate(&machine_id, "phone-a", &token)
            .expect("未授权")
            .is_none(),
        "扫码后不能自动获得访问权"
    );
    assert!(
        access
            .approve(
                &pending.request_id,
                &[MobileProjectAccess {
                    workspace_id: "invented-project".into(),
                    access: MobilePermission::Control
                }],
                now
            )
            .is_err(),
        "不存在的项目不能授权"
    );
    let granted = access
        .approve(
            &pending.request_id,
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Read,
            }],
            now,
        )
        .expect("Mac 明确授权");
    assert!(
        granted.allows(&project.workspace_id, false),
        "授权真实项目读取"
    );
    assert!(
        !granted.allows(&project.workspace_id, true),
        "不能扩大到控制权"
    );
    assert!(
        access
            .authenticate(&machine_id, "phone-a", &token)
            .expect("认证")
            .is_some(),
        "批准后的独立手机凭据可认证"
    );
    assert!(
        access
            .authenticate("wrong-mac", "phone-a", &token)
            .expect("错机器")
            .is_none(),
        "明确拒绝错误电脑目标"
    );
    let other_id = other.machine_id();
    assert!(
        other
            .authenticate(&other_id, "phone-a", &token)
            .expect("B")
            .is_none(),
        "A 配对不授权 B"
    );
    access
        .replace_permissions("phone-a", granted.revision, &[])
        .expect("Mac 移除项目");
    let lowered = access
        .authenticate(&machine_id, "phone-a", &token)
        .expect("新认证")
        .expect("设备仍配对");
    assert_eq!(lowered.revision, 2, "不能复用旧 revision 快照");
    assert!(
        !lowered.allows(&project.workspace_id, false),
        "移除项目后立即失去读取范围"
    );
    access.revoke("phone-a").expect("Mac 撤销");
    assert!(
        a.mobile_access_store()
            .authenticate("phone-a", &token)
            .expect("重开授权存储")
            .is_none(),
        "撤销跨重新打开持久化"
    );
    let qr = access
        .begin_pairing("Mac", vec!["wss://100.64.1.2:9443/mobile".into()], now)
        .expect("下一张 QR");
    let pending = access
        .claim(
            make_request(&qr.to_json().expect("QR"), "phone-c", 'c'),
            now,
        )
        .expect("C 扫码");
    // 120s = 签发到本地批准的统一 TTL；边界等于到期时间，必须拒绝。
    assert!(
        access
            .approve(&pending.request_id, &[], now + Duration::from_secs(120))
            .is_err(),
        "等待 Mac 过久的申请失效"
    );
    assert_eq!(
        access.devices().expect("设备列表").len(),
        1,
        "失效 C 不能留下设备记录，原撤销 A 仍保留"
    );
    let qr = access
        .begin_pairing("Mac", vec!["wss://100.64.1.2:9443/mobile".into()], now)
        .expect("重开配对");
    let pending = access
        .claim(
            make_request(&qr.to_json().expect("QR"), "phone-d", 'd'),
            now,
        )
        .expect("D 扫码");
    access.cancel_pairing();
    assert!(
        access.approve(&pending.request_id, &[], now).is_err(),
        "关闭/取消配对后迟到批准无效"
    );
    assert_eq!(
        access.devices().expect("设备列表").len(),
        1,
        "取消 D 不能留下设备记录"
    );
}

fn make_request(qr: &str, phone: &str, token: char) -> PairingRequest {
    let q: serde_json::Value = serde_json::from_str(qr).expect("QR JSON");
    serde_json::from_value(serde_json::json!({"offer_id":q["offer_id"],"secret":q["secret"],"device_id":phone,"device_name":"iPhone","device_token":token.to_string().repeat(64)})).expect("手机请求")
}
