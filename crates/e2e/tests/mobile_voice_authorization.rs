use app::{MobileAccess, MobileServer};
use serde_json::json;
use storage::{AppStore, MobilePermission, MobileProjectAccess};
#[path = "support/mobile.rs"]
mod support;

/// 已配对的控制设备自动获得固定语音服务授权，无需手机配置供应商Key或网关口令。
///
/// 数据构造：隔离TLS身份、一个项目、control-phone与read-phone各有独立64字符配对token。
///   授权有效期 = 24小时 × 60分钟 × 60秒 = 86400秒；发行时间取真实系统时钟。
/// 执行过程：错误token和只读设备请求 → 控制设备领取签名 → Node网关验签及换取媒体票据 →
///   撤销控制设备后再次请求；所有调用均使用实际TLS和现有授权库。
/// 预期结果：控制设备无需额外Key即可开聊；签名不得包含配对token，错误/只读/已撤销设备无授权。
#[test]
fn paired_control_device_gets_signed_voice_access_without_provider_keys() {
    let directory = tempfile::tempdir().expect("独立授权数据");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(directory.path(), "Voice")
        .expect("项目")
        .workspace_id;
    for (device, permission) in [
        ("control-phone", MobilePermission::Control),
        ("read-phone", MobilePermission::Read),
    ] {
        store
            .mobile_access_store()
            .register(
                device,
                device,
                &"a".repeat(64),
                &[MobileProjectAccess {
                    workspace_id: workspace.clone(),
                    access: permission,
                }],
            )
            .expect("设备权限");
    }
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS身份");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start(
        MobileAccess::with_identity_for_e2e(store.clone(), identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
    )
    .expect("TLS服务");
    let request = |device: &str, token: &str| {
        let mut socket = support::connect(server.addresses()[0], cert.clone());
        support::exchange(
            &mut socket,
            &machine,
            "voice",
            json!({"type":"voice_authorization","device_id":device,"token":token}),
        )
    };
    let valid = request("control-phone", &"a".repeat(64));
    assert_eq!(
        valid["result"]["status"], "voice_authorized",
        "已配对控制设备应该无需Key获得语音授权"
    );
    let grant = &valid["result"]["voice_authorization"];
    assert_eq!(
        grant["origin"], "https://soulvoiceai.com/norma/voice",
        "只能签发固定服务授权，不能由手机任意指定"
    );
    assert_eq!(
        grant["device_id"], "control-phone",
        "授权必须绑定已验证设备"
    );
    assert!(
        !grant.to_string().contains(&"a".repeat(64)),
        "不能把Mac配对token泄漏给公网网关"
    );
    assert!(
        grant["token"]
            .as_str()
            .is_some_and(|token| token.starts_with("nva1.")),
        "必须返回安装身份签名而非共享口令"
    );
    let wrong = request("control-phone", &"b".repeat(64));
    assert_eq!(
        wrong["error"], "unauthorized",
        "错误配对口令不能领取语音授权"
    );
    assert!(wrong.get("result").is_none(), "失败不能携带可用授权");
    let read = request("read-phone", &"a".repeat(64));
    assert_eq!(
        read["error"], "unauthorized",
        "只读设备不能获得付费语音能力"
    );
    assert!(read.get("result").is_none(), "只读拒绝不得附带授权");
    let fixture = directory.path().join("voice.json");
    std::fs::write(
        &fixture,
        json!({"certificate":cert,"authorization":grant}).to_string(),
    )
    .expect("测试授权");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("workspace");
    let output = std::process::Command::new("node")
        .arg(root.join("services/voice/check-authorization.mjs"))
        .arg(fixture)
        .output()
        .expect("真实网关验签");
    assert!(
        output.status.success(),
        "跨语言验签和媒体票据失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    store
        .mobile_access_store()
        .revoke("control-phone")
        .expect("撤销设备");
    let revoked = request("control-phone", &"a".repeat(64));
    assert_eq!(revoked["error"], "unauthorized", "撤销后不能刷新语音授权");
    assert!(
        revoked.get("result").is_none(),
        "已撤销设备不得收到任何新授权"
    );
    drop(server);
}
