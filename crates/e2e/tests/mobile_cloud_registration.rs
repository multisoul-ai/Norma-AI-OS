use app::{MobileAccess, MobileServer};
use serde_json::json;
use storage::{AppStore, MobilePermission};
#[path = "support/mobile.rs"]
mod support;

/// 扫码配对后，手机自动获得一次性服务注册证明，不需要第二张邀请二维码。
///
/// 数据构造：独立 Mac TLS 身份；control-phone 获全部项目控制权，尚无项目。
///   配对 token 为 32 字节的 64 位十六进制；云 token 为 32 字节的 43 位 base64url。
///   注册证明有效期 = 2 分钟 × 60 秒 = 120 秒；证明仅含云 token 的 SHA256。
/// 执行过程：真实 TLS 请求注册 → 错 token → Node 验证并注册/重试/撤销 → Mac 撤销后再请求。
/// 预期结果：不依赖项目存在即可注册；不泄漏配对 token；错误及已撤销设备无证明；云身份可持久恢复。
#[test]
fn paired_phone_registers_cloud_without_second_qr() {
    let directory = tempfile::tempdir().expect("独立注册数据");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    store.mobile_access_store().register_all_projects(
        "control-phone", "iPhone", &"a".repeat(64), MobilePermission::Control,
    ).expect("预先由 Mac 批准全部项目控制权");
    let identity = norma::mobile::ServerIdentity::generate().expect("TLS身份");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let server = MobileServer::start(
        MobileAccess::with_identity_for_e2e(store.clone(), identity),
        vec!["127.0.0.1:0".parse().expect("回环地址")],
    ).expect("TLS服务");
    let request = |token: &str| {
        let mut socket = support::connect(server.addresses()[0], cert.clone());
        support::exchange(&mut socket, &machine, "registration", json!({
            "type":"cloud_registration", "device_id":"control-phone", "token":token,
            "cloud_device_id":"13f257c9-c14a-4d76-974c-ebdc1f06a5dc",
            // SHA256(base64url(32 zero bytes))：云 token 和 Mac 配对 token 完全独立。
            "token_hash":"0f007385b6f9d4b7eeb2748605afe1a984a0a3bfa3f014d09e2a784ce9e5cd1a"
        }))
    };
    let valid = request(&"a".repeat(64));
    assert_eq!(valid["result"]["status"], "cloud_registration", "配对完成后必须能直接注册服务，不能再要求邀请二维码");
    assert!(!valid.to_string().contains(&"a".repeat(64)), "注册证明不能泄漏 Mac 配对 token");
    let wrong = request(&"b".repeat(64));
    assert_eq!(wrong["error"], "unauthorized", "错误配对 token 不得获得注册证明");
    assert!(wrong.get("result").is_none(), "拒绝时不得附带注册证明");
    let fixture = directory.path().join("registration.json");
    std::fs::write(&fixture, json!({"certificate":cert, "proof":valid["result"]["registration_proof"]}).to_string()).expect("隔离验签输入");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(std::path::Path::parent).expect("workspace");
    let output = std::process::Command::new("node").arg(root.join("services/norma-cloud/check-paired-registration.mjs"))
        .arg(fixture).output().expect("真实服务注册测试");
    assert!(output.status.success(), "服务注册验证失败：{}", String::from_utf8_lossy(&output.stderr));
    store.mobile_access_store().revoke("control-phone").expect("撤销 Mac 授权");
    let revoked = request(&"a".repeat(64));
    assert_eq!(revoked["error"], "unauthorized", "撤销后不得再签发注册证明");
    assert!(revoked.get("result").is_none(), "撤销后不得返回有效注册数据");
}

/// 原生 App 从二维码图片解码，经真实 Mac 审批自动注册并连接真实中继。
/// 数据构造：全新TLS身份/空授权库/临时云数据库；两分钟配对码，仅本测试批准新申请。
/// 执行过程：Node启动真实网关→Mac注册中继→XCTest扫码配对→批准→手机自动注册及恢复。
/// 预期结果：仅一条手机云记录、零邀请记录；初始未授权，批准后真实中继访问通过。
#[test]
#[ignore = "requires prebuilt iOS tests and NORMA_IOS_SIMULATOR"]
fn native_single_scan_registers_and_connects_relay() {
    use std::{io::{BufRead, Write}, process::{Command, Stdio}, time::{Duration, Instant}};
    let directory = tempfile::tempdir().expect("隔离原生验收目录");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let identity = norma::mobile::ServerIdentity::generate().expect("独立身份");
    let cert = identity.certificate_der().to_vec();
    let server = MobileServer::start(MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环地址")]).expect("真实Mac服务");
    let offer = server.begin_pairing("Single Scan Mac").expect("唯一配对码").to_json().expect("二维码JSON");
    let path = directory.path().join("native.json");
    std::fs::write(&path, json!({"certificate":cert,"offer":offer}).to_string()).expect("原生输入");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(std::path::Path::parent).expect("workspace");
    let mut child = Command::new("node").arg(root.join("services/norma-cloud/check-native-registration.mjs"))
        .arg(path).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().expect("Node网关");
    let mut ready = String::new();
    let mut output = std::io::BufReader::new(child.stdout.take().expect("stdout"));
    output.read_line(&mut ready).expect("启动回执");
    let ready: serde_json::Value = serde_json::from_str(&ready).expect("网关真实端口");
    server.start_local_collaboration_relay_for_e2e(ready["port"].as_u64().expect("端口") as u16).expect("真实Mac中继");
    child.stdin.as_mut().expect("stdin").write_all(b"ready\n").expect("启动原生验收");
    // 210s = Node 180s验收预算 + 30s进程关闭；20ms仅轮询隔离审批，禁止无限挂起。
    let ui = std::env::var_os("NORMA_REGISTRATION_UI").is_some();
    // Computer Use receives ten minutes; only the QR itself still expires after two minutes.
    let deadline = Instant::now() + Duration::from_secs(if ui { 630 } else { 210 });
    loop {
        if let Some(status) = child.try_wait().expect("验收进程状态") {
            assert!(status.success(), "原生单次扫码验收失败"); break;
        }
        if Instant::now() >= deadline {
            child.kill().expect("终止超时测试"); child.wait().expect("回收超时测试");
            panic!("单次扫码验收超时");
        }
        if let Some(pending) = server.pending().expect("隔离Mac审批") {
            server.approve_all_projects(&pending.request_id, MobilePermission::Control).expect("仅批准隔离测试手机");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
