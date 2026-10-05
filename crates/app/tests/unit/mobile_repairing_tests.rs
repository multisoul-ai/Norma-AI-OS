use super::{attach, Command};
use gpui::TestAppContext;
use std::{sync::Arc, time::Instant};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

/// Mac 再次批准已撤销 iPhone 时保留真实错误，重配成功后清除申请并持久化新范围。
///
/// 数据构造（含关键数值的推导过程）：
///   一个真实注册项目、隔离 SQLite 与回环 TLS 服务；手机沿用原设备 ID 和 token，
///   token=64 hex 字符=32 字节；revision 初次 1 → 撤销 2 → 重配 3。
/// 执行过程：
///   1. 初次全项目控制授权后撤销；手机认领新二维码，Mac 显示待审批。
///   2. 用不存在的项目批准 → UI 显示真实项目错误，申请保留且设备仍撤销。
///   3. 用生产 Command::ApproveAll 明确批准全项目只读 → 核验 UI 及持久化鉴权。
/// 预期结果：项目错误不得被伪装成验证码过期；失败不得解除撤销；
///   成功后只读可用、旧控制不可用，修订号递增且 UI 不再显示错误或待审批。
#[test]
fn reapproval_reports_real_errors_and_approves_revoked_phone() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let store = AppStore::new(dir.path().join("app.sqlite"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "Repairing Project")
        .expect("注册项目");
    let token = "a".repeat(64);
    store
        .mobile_access_store()
        .register_all_projects("phone-repair", "iPhone", &token, MobilePermission::Control)
        .expect("首次 Mac 批准");
    store
        .mobile_access_store()
        .revoke("phone-repair")
        .expect("撤销");
    let mut access = crate::mobile::MobileAccess::with_identity_for_e2e(
        store.clone(),
        norma::mobile::ServerIdentity::generate().expect("TLS 身份"),
    );
    let now = Instant::now();
    let qr = access
        .begin_pairing("Mac", vec!["wss://127.0.0.1:9443/mobile".into()], now)
        .expect("新配对二维码");
    let offer: serde_json::Value =
        serde_json::from_str(&qr.to_json().expect("二维码 JSON")).expect("解析 offer");
    let pending = access
        .claim(
            serde_json::from_value(serde_json::json!({
                "offer_id": offer["offer_id"], "secret": offer["secret"],
                "device_id": "phone-repair", "device_name": "iPhone", "device_token": token
            }))
            .expect("重配申请"),
            now,
        )
        .expect("手机认领");
    let server = Arc::new(
        crate::mobile::MobileServer::start(access, vec!["127.0.0.1:0".parse().expect("回环端口")])
            .expect("启动真实服务"),
    );
    let mut cx = TestAppContext::single();
    let owner = cx.update(|cx| attach(store.clone(), cx));
    owner.update(&mut cx, |host, cx| {
        host.server = Some(server.clone());
        host.pending = Some(pending.clone());
        host.request(
            Command::Approve {
                request_id: pending.request_id.clone(),
                projects: vec![MobileProjectAccess {
                    workspace_id: "unknown-project".into(),
                    access: MobilePermission::Control,
                }],
            },
            cx,
        );
    });
    cx.run_until_parked();
    owner.read_with(&cx, |host, _| {
        let message = host.error.as_ref().expect("失败必须显示错误").to_string();
        assert!(
            message.contains("unknown mobile project"),
            "必须显示真实错误，实际为 {message}"
        );
        assert!(
            !message.contains("expired"),
            "项目错误不得被误报为验证码过期"
        );
        assert_eq!(
            host.pending.as_ref().map(|p| &p.request_id),
            Some(&pending.request_id),
            "失败必须保留准确申请以便重试"
        );
    });
    assert!(
        store
            .mobile_access_store()
            .authenticate("phone-repair", &token)
            .expect("失败后鉴权")
            .is_none(),
        "失败不能复活已撤销设备"
    );
    owner.update(&mut cx, |host, cx| {
        host.request(
            Command::ApproveAll {
                request_id: pending.request_id,
                access: MobilePermission::Read,
            },
            cx,
        )
    });
    cx.run_until_parked();
    owner.read_with(&cx, |host, _| {
        assert!(host.error.is_none(), "批准重配不得再报错：{:?}", host.error);
        assert!(host.pending.is_none(), "成功后待审批必须清除");
        assert!(!host.busy, "成功后操作必须解锁");
        assert_eq!(host.devices.len(), 1, "只能保留原手机的一条设备记录");
        assert_eq!(
            host.devices[0].device_id, "phone-repair",
            "只能批准核对过的手机"
        );
    });
    let grant = store
        .mobile_access_store()
        .authenticate("phone-repair", &token)
        .expect("重配后鉴权")
        .expect("iPhone 原凭据经明确批准后可用");
    assert_eq!(grant.revision, 3, "新授权不能复用旧修订号");
    assert!(
        grant.allows(&project.workspace_id, false),
        "新全项目只读授权必须生效"
    );
    assert!(
        !grant.allows(&project.workspace_id, true),
        "旧控制权限不得被继承"
    );
    assert!(
        !grant.allows("unknown-project", false),
        "不存在的项目仍不可访问"
    );
}
