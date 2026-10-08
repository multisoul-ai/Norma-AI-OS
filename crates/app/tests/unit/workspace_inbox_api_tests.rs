use crate::mobile::{
    activity, workspace_chat::LiveWorkspaceAction, MobileAccess, MobileServer, MobileSources,
};
use norma::mobile::{inbox_item_key, WorkspaceActivityTarget};
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use storage::{
    AppStore, MobileCommandStore, MobilePermission, MobileProjectAccess, NewMobileCommand,
};

/// 场景：设备级pending读全部授权桌面项目，真实交付分页完整且不包含手机线程成果。
/// 数据构造：空间A的4授权项目、空间B重复项目0和1未授权项目；21桌面交付跨20项页界，另1手机交付。
/// 执行过程：Read权限读取→存项目0失败忽略→全量和B单独读取→捕获时撤权再读。
/// 预期结果：25=4失败+21验收；忽略后24；B无失败；未授权项目/手机交付不返回，撤权拒绝迟到结果。
#[test]
fn pending_api_reads_authorized_complete_list_and_persisted_project_dismissals() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let app = AppStore::new(dir.path().join("app.sqlite"));
    let catalog = app.desktop_workspace_store();
    let a = catalog.create("桌面空间A").expect("空间A");
    let b = catalog.create("桌面空间B").expect("空间B");
    let mut grants = vec![];
    let mut primary = None;
    let mut denied_project = None;
    for n in 0..5 {
        let root = dir.path().join(format!("project-{n}"));
        std::fs::create_dir(&root).expect("项目目录");
        let area = catalog
            .add_project(if n < 4 { &a.id } else { &b.id }, &root)
            .expect("绑定项目");
        let identity = app
            .workspace_identity_store()
            .resolve_or_create(&root, "项目")
            .expect("项目身份");
        if n < 4 {
            grants.push(MobileProjectAccess {
                workspace_id: identity.workspace_id.clone(),
                access: MobilePermission::Read,
            });
        } else {
            denied_project = Some(identity.workspace_id.clone());
        }
        if n == 0 {
            catalog.add_project(&b.id, &root).expect("跨空间重复绑定");
            primary = Some((area, identity.workspace_id));
        }
    }
    let (area, project_id) = primary.expect("项目0");
    let token = "a".repeat(64);
    app.mobile_access_store()
        .register("reader", "只读手机", &token, &grants)
        .expect("项目只读授权");
    let scope = activity::scope(&a.id, &area.id);
    let mut fingerprints = vec![];
    for n in 0..21 {
        fingerprints.push(save_delivery(app.path(), dir.path(), &scope, n).fingerprint);
    }
    let mobile = save_delivery(app.path(), dir.path(), "mobile-thread-59", 99);
    let access = MobileAccess {
        store: app.clone(),
        identity: norma::mobile::ServerIdentity::generate().expect("隔离TLS身份"),
        pairing: None,
    };
    let mac = access.machine_id();
    let (sender, receiver) = async_channel::bounded::<crate::mobile::LiveWorkspaceRequest>(2);
    let close = receiver.clone();
    let server = MobileServer::start_with_mobile_sources(
        access,
        vec!["127.0.0.1:0".parse().expect("回环")],
        MobileSources {
            workspaces: Some(sender),
            ..Default::default()
        },
    )
    .expect("真实认证服务");
    let revoke = Arc::new(AtomicBool::new(false));
    let worker_revoke = revoke.clone();
    let worker_app = app.clone();
    let worker = std::thread::spawn(move || {
        while let Ok(call) = receiver.recv_blocking() {
            assert!(
                matches!(call.action, LiveWorkspaceAction::ActivityCapture),
                "pending只能捕获，不能执行控制动作"
            );
            if worker_revoke.swap(false, Ordering::AcqRel) {
                worker_app
                    .mobile_access_store()
                    .revoke("reader")
                    .expect("捕获时撤权");
            }
            let entries = call.areas.iter().map(|bound| json!({
                "project":{"area_id":bound.area.id,"project_id":bound.project_id,
                    "name":bound.area.display_name,"root":"","definitions":[],
                    "agents":[{"definition_id":1,"name":"Agent","profile":"Codex",
                        "card":{"node_id":1,"profile":"Codex","title":"Agent","mobile_control":true,
                            "state":"failed","run_id":"run","session_id":"session","turn_id":"turn"}}]},
                "approvals":[]
            })).collect::<Vec<_>>();
            call.reply.send(Ok(json!(entries))).expect("捕获回复");
        }
    });
    let request: activity::Request =
        serde_json::from_value(json!({"kind":"pending"})).expect("设备级pending请求");
    let read = || activity::handle(&server.shared, &mac, "reader", &token, &request);
    let first = read().expect("Read授权可读pending");
    let items = first["workspace_activity"]["pending"]["items"]
        .as_array()
        .expect("pending列表");
    assert_eq!(
        first["workspace_activity"]["pending"]["machine_id"], mac,
        "设备级返回来源Mac"
    );
    assert_eq!(items.len(), 25, "完整4失败+21验收，不能截断到3项目或20交付");
    assert!(
        !items
            .iter()
            .any(|i| i["project_id"] == *denied_project.as_ref().expect("未授权项目")),
        "未授权项目不得替换任何获授权事项混入列表"
    );
    for grant in &grants {
        assert!(
            items.iter().any(|i| i["project_id"] == grant.workspace_id),
            "每个获授权项目都必须返回其真实失败项"
        );
    }
    assert_eq!(
        items.iter().filter(|i| i["kind"] == "failed").count(),
        4,
        "重复绑定只有一个失败，未授权第5项目不可见"
    );
    assert_eq!(
        items.iter().filter(|i| i["kind"] == "review").count(),
        21,
        "第二页交付不能遗漏"
    );
    assert!(
        !items
            .iter()
            .any(|i| i["target"]["fingerprint"] == mobile.fingerprint),
        "#59手机线程交付不进入桌面收件箱"
    );
    assert!(
        items.iter().filter(|i| i["kind"] == "review").all(|i| {
            i["created_at"].as_u64().is_some_and(|t| t > 0)
                && fingerprints
                    .iter()
                    .any(|f| i["target"]["fingerprint"] == *f)
        }),
        "每项必须携带真实保存时间与持久化指纹"
    );
    assert!(
        first["workspace_activity"]["pending"]
            .get("failure_count")
            .is_none(),
        "N只能来自items.len而非摘要计数"
    );
    let target: WorkspaceActivityTarget =
        serde_json::from_value(json!({"area_id":area.id,"node_id":1,"profile":"Codex",
        "run_id":"run","session_id":"session","turn_id":"turn"}))
        .expect("失败目标");
    let key = inbox_item_key("failed", &project_id, &target);
    let commands = MobileCommandStore::new(app.path().to_owned());
    let source = NewMobileCommand {
        device_id: "reader".into(),
        operation_id: "dismiss-fixture".into(),
        workspace_id: scope,
        route_id: "route".into(),
        owner_id: "owner".into(),
        request_hash: "b".repeat(64),
    };
    commands.reserve(&source).expect("预留fixture");
    commands
        .dismiss_inbox_item(&source, &project_id, storage::InboxItemKind::Failed, &key)
        .expect("已存在合约存储忽略fixture");
    let second = read().expect("忽略后重新读取");
    let visible = second["workspace_activity"]["pending"]["items"]
        .as_array()
        .expect("完整列表");
    assert_eq!(visible.len(), 24, "真实存储忽略恰好隐藏一个事项");
    assert!(
        !visible.iter().any(|i| i["item_key"] == key),
        "不能从第二工作空间再次出现"
    );
    let scoped: activity::Request =
        serde_json::from_value(json!({"kind":"pending","desktop_workspace_id":b.id}))
            .expect("第二空间pending");
    let second_space = activity::handle(&server.shared, &mac, "reader", &token, &scoped)
        .expect("B的获授权项目可读");
    assert_eq!(
        second_space["workspace_activity"]["pending"]["items"],
        json!([]),
        "B单独读取也不能返回已忽略失败或未授权项目"
    );
    revoke.store(true, Ordering::Release);
    assert_eq!(read(), Err("unauthorized"), "捕获期间撤权不得泄露迟到列表");
    close.close();
    worker.join().expect("捕获worker结束");
    server.shutdown();
}

fn save_delivery(
    path: &std::path::Path,
    root: &std::path::Path,
    scope: &str,
    n: u64,
) -> storage::StoredTaskDelivery {
    let key = storage::MobileTurnKey {
        node_id: 1,
        profile: "Codex".into(),
        run_id: format!("delivery-run-{n}"),
        session_id: "old-session".into(),
        turn_id: format!("old-turn-{n}"),
    };
    let writer = storage::BackgroundApprovalAudit::new(
        storage::ApprovalAuditStore::new(root.join("audit.sqlite")).with_turn_store(
            storage::MobileTurnStore::for_workspace(path.to_owned(), scope.into()),
        ),
    )
    .expect("真实终态writer");
    // 5秒沿用后台审计提交预算；等commit后才能冻结交付。
    writer
        .record_turn(storage::MobileTurnOutcome {
            key: key.clone(),
            state: storage::MobileTurnState::Completed,
            summary: None,
        })
        .expect("入队")
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("提交回执")
        .expect("终态落盘");
    storage::MobileTaskDeliveryStore::new(path.to_owned())
        .record(
            scope,
            &key,
            &storage::TaskDeliveryContent::Available {
                review: workspace::review::ProjectReview {
                    base_commit: None,
                    files: vec![],
                    conflicted: false,
                },
            },
        )
        .expect("冻结成果")
}
