use app::MobileAccess;
use norma::mobile::{PairingRequest, ServerIdentity};
use std::time::{Duration, Instant};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// Mac 手机服务身份在同一 AppDB 中重启后保持稳定，损坏记录禁止自动换身份。
///
/// 数据构造（含关键数值的推导过程）：一个隔离 app.sqlite，两次打开和一次损坏写入。
/// 执行过程（逐步说明系统如何处理）：
///   1. 首次打开生成身份并存入 SQLite。
///   2. 第二次打开同一库，检查 machine_id 不变。
///   3. 把身份记录改为坏 JSON，第三次打开必须失败。
/// 预期结果：
///   - 正断言：两次打开返回同一非空身份。
///   - 负断言：损坏记录不能被新身份覆盖。
#[cfg(target_os = "macos")]
#[test]
fn mobile_identity_uses_sqlite_and_fails_closed_on_corruption() {
    let dir = tempfile::tempdir().expect("测试应创建隔离数据库目录");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let first = MobileAccess::open(store.clone()).expect("首次打开应生成手机服务身份");
    let second = MobileAccess::open(store.clone()).expect("重开应恢复手机服务身份");
    assert!(!first.machine_id().is_empty(), "手机服务身份不能是空值");
    assert_eq!(
        first.machine_id(),
        second.machine_id(),
        "重开同一 SQLite 不得变更身份"
    );
    store
        .settings_store()
        .set_string("mobile.server_identity", "invalid-json")
        .expect("测试应能模拟损坏身份记录");
    assert!(
        MobileAccess::open(store.clone()).is_err(),
        "损坏身份记录必须阻止服务启动"
    );
    assert_eq!(
        store
            .settings_store()
            .get_string("mobile.server_identity")
            .expect("应读取损坏记录")
            .as_deref(),
        Some("invalid-json"),
        "启动失败不能覆盖损坏记录"
    );
}

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

/// Mac 明确授予手机全项目控制后，新注册项目可用；降权和撤销立即使旧凭据失效。
///
/// 数据构造（含关键数值的推导过程）：
///   同一 Mac 的项目 A、B 各 1 个，B 在授权后注册；手机凭据 64 hex 字符 = 32 字节。
///   授权版本从 1 开始；改回逐项目后为 2，撤销后为 3。
///
/// 执行过程：
///   1. 创建项目 A，扫码但在 Mac 确认前尝试认证。
///   2. Mac 明确批准全项目控制，随后注册项目 B 并重新鉴权。
///   3. Mac 改回仅 A 只读，再撤销设备并重新鉴权。
///
/// 预期结果：批准前不可访问；批准后 A/B 可控制但未知项目不可访问；降权后 B 不可读、A 不可控制；撤销后无法认证。
#[test]
fn mobile_pairing_explicit_all_projects_covers_future_projects_and_revalidates() {
    let dir = tempfile::tempdir().expect("隔离 Mac 与项目目录");
    let store = AppStore::new(dir.path().join("mac.sqlite"));
    let project_a = dir.path().join("project-a");
    std::fs::create_dir(&project_a).expect("创建项目 A 目录");
    let a = store
        .workspace_identity_store()
        .resolve_or_create(&project_a, "项目 A")
        .expect("注册项目 A");
    let identity = ServerIdentity::generate().expect("测试 TLS 身份");
    let machine_id = identity.machine_id();
    let mut access = MobileAccess::with_identity_for_e2e(store.clone(), identity);
    let now = Instant::now();
    let qr = access
        .begin_pairing("Mac", vec!["wss://192.168.1.2:9443/mobile".into()], now)
        .expect("生成二维码");
    let pending = access
        .claim(
            make_request(&qr.to_json().expect("QR"), "phone-all", 'a'),
            now,
        )
        .expect("手机扫码");
    let token = "a".repeat(64);
    assert!(
        access
            .authenticate(&machine_id, "phone-all", &token)
            .expect("批准前鉴权")
            .is_none(),
        "扫码不等于获得全项目授权"
    );

    let granted = access
        .approve_all_projects(&pending.request_id, MobilePermission::Control, now)
        .expect("Mac 明确授予全项目控制");
    assert_eq!(
        granted.all_projects_access,
        Some(MobilePermission::Control),
        "授权来源必须可见"
    );
    assert!(granted.allows(&a.workspace_id, true), "已注册项目 A 可控制");
    assert!(
        !granted.allows("invented-project", true),
        "不存在的项目不可控制"
    );

    let project_b = dir.path().join("project-b");
    std::fs::create_dir(&project_b).expect("创建项目 B 目录");
    let b = store
        .workspace_identity_store()
        .resolve_or_create(&project_b, "项目 B")
        .expect("随后注册项目 B");
    let refreshed = access
        .authenticate(&machine_id, "phone-all", &token)
        .expect("B 注册后重新鉴权")
        .expect("手机凭据有效");
    assert!(
        refreshed.allows(&b.workspace_id, true),
        "新注册项目 B 无需再次逐项授权"
    );

    let lowered = access
        .replace_permissions(
            "phone-all",
            refreshed.revision,
            &[MobileProjectAccess {
                workspace_id: a.workspace_id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("Mac 改回逐项目只读");
    assert_eq!(lowered.revision, 2, "降权必须推进修订号");
    assert!(
        !lowered.allows(&a.workspace_id, true),
        "项目 A 的旧控制权必须失效"
    );
    assert!(
        !lowered.allows(&b.workspace_id, false),
        "项目 B 的旧全项目范围必须失效"
    );
    access.revoke("phone-all").expect("Mac 撤销手机");
    assert!(
        access
            .authenticate(&machine_id, "phone-all", &token)
            .expect("撤销后鉴权")
            .is_none(),
        "撤销后旧手机凭据不能恢复访问"
    );
}

fn make_request(qr: &str, phone: &str, token: char) -> PairingRequest {
    let q: serde_json::Value = serde_json::from_str(qr).expect("QR JSON");
    serde_json::from_value(serde_json::json!({"offer_id":q["offer_id"],"secret":q["secret"],"device_id":phone,"device_name":"iPhone","device_token":token.to_string().repeat(64)})).expect("手机请求")
}
