use app::{LiveApprovalRequest, MobileAccess, MobileServer};
use norma::mobile::ApprovalSnapshot;
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use storage::{AppStore, MobilePermission, MobileProjectAccess};

#[path = "support/mobile.rs"]
mod support;

/// 只读手机可读取当前会话，但审批读取期间撤销权限必须阻止内容返回。
///
/// 数据构造：隔离项目 A / 未授权 B、phone-a 的 read 权限、有界审批通道容量8与正式连接上限一致。
/// 执行过程：读取A审批 → 只读决定拒绝 → 升级control后决定/持久化/重放 → B越权拒绝 → 读取中撤销。
/// 预期结果：只读成功得到工具输入；B 不泄漏；撤销竞争时返回 unauthorized，不回传已读工具输入。
#[test]
fn approval_read_rechecks_permission_after_background_work() {
    let dir = tempfile::tempdir().expect("隔离库");
    let store = AppStore::new(dir.path().join("app.db"));
    let project = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "A")
        .expect("项目");
    store
        .mobile_access_store()
        .register(
            "phone-a",
            "iPhone",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读授权");
    let identity = norma::mobile::ServerIdentity::generate().expect("证书");
    let cert = identity.certificate_der().to_vec();
    let machine = identity.machine_id();
    let (tx, rx) = async_channel::bounded::<LiveApprovalRequest>(8);
    let (command_tx, command_rx) = async_channel::bounded::<app::LiveCommandRequest>(8);
    let server = Arc::new(
        MobileServer::start_with_mobile_sources(
            MobileAccess::with_identity_for_e2e(store, identity),
            vec!["127.0.0.1:0".parse().expect("回环")],
            app::MobileSources {
                approvals: Some(tx),
                commands: Some(command_tx),
                ..Default::default()
            },
        )
        .expect("服务"),
    );
    let action = json!({"type":"approvals","device_id":"phone-a","token":"a".repeat(64),"request":{
        "workspace_id":project.workspace_id,"route_id":"route-a"
    }});
    let fetch = |action: serde_json::Value| {
        let server = server.clone();
        let cert = cert.clone();
        let machine = machine.clone();
        std::thread::spawn(move || {
            let mut socket = support::connect(server.addresses()[0], cert);
            support::exchange(&mut socket, &machine, "read", action)
        })
    };
    let pending = || {
        // 5s = 3s TLS + 2s thread scheduling; test failures must not leave an unbounded receive.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(request) = rx.try_recv() {
                break request;
            }
            assert!(Instant::now() < deadline, "授权请求必须到达前台");
            std::thread::sleep(Duration::from_millis(2));
        }
    };
    let page = || -> Vec<ApprovalSnapshot> {
        vec![serde_json::from_value(json!({"target":{"node_id":7,"profile":"Claude","run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":"request-a"},"card_title":"Review","tool":"Bash","details":{"command":"printf exact"},"decisions":["allow","deny"]})).expect("完整审批")]
    };
    let first = fetch(action.clone());
    pending().reply.send(Ok(page())).expect("模拟后台完成");
    let result = first.join().expect("读取完成");
    assert_eq!(
        result["result"]["approvals"]["items"][0]["details"]["command"], "printf exact",
        "只读权限允许审批查询"
    );
    let target = page()[0].target.clone();
    let mut decide = json!({"type":"command","device_id":"phone-a","token":"a".repeat(64),"request":{
        "operation_id":"choice-a","workspace_id":project.workspace_id,"route_id":"route-a","grant_revision":1,
        "command":{"type":"decide_approval","expected":target,"decision":"deny"}
    }});
    assert_eq!(
        fetch(decide.clone()).join().expect("只读决定被拒")["error"],
        "unauthorized",
        "只读可以看但不能决定"
    );
    assert!(command_rx.try_recv().is_err(), "未获控制权限不可进入执行器");
    server
        .replace_permissions(
            "phone-a",
            1,
            &[MobileProjectAccess {
                workspace_id: project.workspace_id.clone(),
                access: MobilePermission::Control,
            }],
        )
        .expect("测试内升级控制权限");
    decide["request"]["grant_revision"] = json!(2);
    let accepted = fetch(decide.clone()).join().expect("唯一决定");
    assert_eq!(
        accepted["result"]["receipt"]["state"], "accepted",
        "只接收请求不能报决定已提交"
    );
    let delivery = command_rx.try_recv().expect("accepted回执之前已投递前台");
    assert!(delivery.lease.begin(), "准确授权的一次执行许可");
    let confirmed = json!({"node_id":7,"run_id":"run-a","session_id":"session-a","turn_id":"turn-a","request_id":"request-a","decision":"deny","status":"submitted"});
    delivery
        .reply
        .send(Ok(confirmed.clone()))
        .expect("模拟正式审计确认");
    let receipt = json!({"type":"receipt","device_id":"phone-a","token":"a".repeat(64),"workspace_id":project.workspace_id,"operation_id":"choice-a"});
    // 5s = network round trips plus background command receipt persistence.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let value = fetch(receipt.clone()).join().expect("回查决定");
        if value["result"]["receipt"]["state"] == "succeeded" {
            assert_eq!(
                value["result"]["receipt"]["result"], confirmed,
                "准确决定与目标必须保留"
            );
            break;
        }
        assert!(Instant::now() < deadline, "决定回执应保存完成");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(
        fetch(decide).join().expect("重复决定")["result"]["receipt"]["result"],
        confirmed,
        "同ID返回原回执"
    );
    assert!(
        command_rx.try_recv().is_err(),
        "重复决定不能再次送给Provider"
    );
    drop(delivery);
    let mut foreign = action.clone();
    foreign["request"]["workspace_id"] = json!("project-b");
    assert_eq!(
        fetch(foreign).join().expect("越权完成")["error"],
        "unauthorized",
        "B 未授权必须拒绝"
    );
    assert!(rx.try_recv().is_err(), "未授权项目不得进入前台读取");
    let second = fetch(action);
    let held = pending();
    server.revoke("phone-a").expect("读取期间撤销");
    held.reply.send(Ok(page())).expect("迟到工具输入");
    let denied = second.join().expect("回查授权后完成");
    assert_eq!(
        denied["error"], "unauthorized",
        "第二次授权校验必须识别撤销"
    );
    assert!(
        denied.get("result").is_none(),
        "撤销后不得泄露已读取的工具输入"
    );
}
