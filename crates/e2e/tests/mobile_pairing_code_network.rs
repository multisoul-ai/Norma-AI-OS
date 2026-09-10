use app::{MobileAccess, MobileServer};
use norma::mobile::ServerIdentity;
use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// Swift 原生 PAKE/Keychain 经真实 Rust WSS 验证短码，随后只获 Mac 选择的范围并观察撤销。
///
/// 数据构造：仅含地址、六位码和预期机器 ID 的 0600 临时 fixture；120s=配对期限，进程超时同期限。
/// 执行过程：错误码被拒 → 正确码 → Mac批准只读项目 → 原生注册/关闭/重开提醒并拒绝旧版本 → Mac撤销。
/// 预期结果：Swift解密并固定证书；无控制升权，撤销后不能查询订阅或沿用凭据；不发送真实APNs。
#[test]
#[ignore = "requires NORMA_SWIFT_PROBE, optionally the native iOS runner"]
fn native_code_client_pairs_and_observes_revoke() {
    use std::os::unix::fs::PermissionsExt;
    let probe = std::env::var("NORMA_SWIFT_PROBE").expect("显式原生 probe");
    let dir = tempfile::tempdir().expect("隔离目录");
    let store = AppStore::new(dir.path().join("db"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Code project")
        .expect("只读项目");
    let identity = ServerIdentity::generate().expect("身份");
    let machine = identity.machine_id();
    // One catalog request at a time; the fixture keeps the production authorization/store path.
    let (source, snapshots) = async_channel::bounded::<app::LiveSnapshotRequest>(1);
    let server = MobileServer::start_with_snapshots(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        Some(source),
    )
    .expect("WSS");
    let qr = server.begin_pairing("Code Mac").expect("签发短码");
    let file = dir.path().join("code.json");
    fs::write(&file, b"").expect("空文件");
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).expect("凭据文件权限");
    fs::write(&file, serde_json::json!({"address":server.addresses()[0].to_string(),"code":qr.pairing_code(),"machine":machine}).to_string()).expect("fixture");
    let log = dir.path().join("probe.log");
    let mut child = Command::new(probe)
        .arg(&file)
        .arg("code")
        .stdout(fs::File::create(&log).expect("日志"))
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("原生客户端");
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut approved = None;
    let mut revoked = false;
    let result = loop {
        if let Ok(request) = snapshots.try_recv() {
            request
                .reply
                .send(request.projects)
                .expect("返回真实授权项目元数据");
        }
        if let Some(status) = child.try_wait().expect("客户端状态") {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill().expect("停止超时客户端");
            child.wait().expect("回收");
            panic!(
                "原生验证码流程超时：{}",
                fs::read_to_string(&log).unwrap_or_default()
            );
        }
        if let Some(pending) = server.pending().expect("申请状态") {
            assert!(
                dir.path().join("wrong-code-rejected").exists(),
                "错误验证码不能生成申请"
            );
            assert!(approved.is_none(), "不能重复登记");
            server
                .approve(
                    &pending.request_id,
                    &[MobileProjectAccess {
                        workspace_id: project.workspace_id.clone(),
                        access: MobilePermission::Read,
                    }],
                )
                .expect("仅测试客户端批准隔离项目");
            approved = Some(pending.device_id);
        }
        if !revoked && dir.path().join("authorized").exists() {
            server
                .revoke(approved.as_deref().expect("先批准再撤销"))
                .expect("撤销");
            revoked = true;
        }
        // 20ms 与监听循环一致，等待测试事件；不改变生产节流或时钟。
        std::thread::sleep(Duration::from_millis(20));
    };
    let output = fs::read_to_string(log).expect("日志");
    assert!(result.success(), "原生验证码验证失败：{output}");
    assert!(
        output.contains("NATIVE_PUSH_REGISTER_DISABLE_REVOKE_READY"),
        "原生客户端必须通过WSS完成订阅、旧版本拒绝、关闭和重开"
    );
    assert!(
        output.contains("NATIVE_CODE_PAIR_AUTH_REVOKE_OK"),
        "必须完整走原生配对与撤销"
    );
    assert!(revoked, "真实后端必须已撤销测试设备");
    assert!(
        !server.devices().expect("设备").iter().any(|d| !d.revoked),
        "不得残留有效测试授权"
    );
}
