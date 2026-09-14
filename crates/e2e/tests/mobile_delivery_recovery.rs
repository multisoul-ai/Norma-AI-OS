use app::{LiveSnapshotRequest, MobileAccess, MobileServer};
use norma::mobile::ServerIdentity;
use serde_json::json;
use std::time::Duration;
use storage::{
    AppStore, ApprovalAuditStore, BackgroundApprovalAudit, MobileCommandStore, MobilePermission,
    MobileProjectAccess, MobileTaskBaselineStore, MobileTaskDeliveryStore, MobileTurnKey,
    MobileTurnOutcome, MobileTurnState, MobileTurnStore, NewMobileCommand, TaskBaseline,
    TaskDeliveryContent,
};
use workspace::review::ProjectReview;

#[path = "support/mobile.rs"]
mod support;

/// 完成事件已提交而成果未冻结：新路由只恢复明确不可用记录，绝不读取重启后的文件。
///
/// 数据构造：旧路由a/b缺成果、frozen已有成果；current属于新路由；unknown没有终态；
///   desktop没有手机起点；mismatch的session不同；foreign属于另一项目，共8轮。
/// 执行过程：正式预留/起点/终态writer构造崩溃临界点 → 新服务与路由 → 未授权/过期路由读取 →
///   首先查a详情 → b的迟到终态commit → 列出全部/待验收 → 重开成果store → 重复查询。
/// 预期结果：a/b为window_closed且准确保留completed；frozen原文/指纹不变；
///   其他5轮不生成成果，未经授权不写恢复记录；不可用成果不进入待验收，无后续文件正文。
#[test]
fn expired_route_delivery_recovery_preserves_only_durable_evidence() {
    let dir = tempfile::tempdir().expect("隔离临界点数据库");
    let db = dir.path().join("app.sqlite");
    let store = AppStore::new(db.clone());
    let workspace = store
        .workspace_identity_store()
        .resolve_or_create(dir.path(), "恢复项目")
        .expect("项目身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register(
            "reader",
            "reader",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: workspace.clone(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读项目授权");
    let key = |turn: &str| MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-before-restart".into(),
        session_id: "session-before-restart".into(),
        turn_id: turn.into(),
    };
    let review = ProjectReview {
        base_commit: None,
        conflicted: false,
        files: vec![],
    };
    let seed = |project: &str, turn: &str, route: &str, baseline: bool, completed: bool| {
        let source = NewMobileCommand {
            device_id: "reader".into(),
            operation_id: format!("operation-{turn}"),
            workspace_id: project.into(),
            route_id: route.into(),
            owner_id: "previous-server".into(),
            request_hash: "b".repeat(64),
        };
        MobileCommandStore::new(db.clone())
            .reserve(&source)
            .expect("正式预留");
        if baseline {
            MobileTaskBaselineStore::new(db.clone())
                .record(
                    &source,
                    &key(turn),
                    &TaskBaseline::Available {
                        review: review.clone(),
                    },
                )
                .expect("绑定发送前起点");
        }
        if completed {
            let writer = BackgroundApprovalAudit::new(
                ApprovalAuditStore::new(dir.path().join(format!("{turn}.sqlite")))
                    .with_turn_store(MobileTurnStore::for_workspace(db.clone(), project.into())),
            )
            .expect("正式终态writer");
            let mut turn_key = key(turn);
            if turn == "mismatch" {
                turn_key.session_id = "another-session".into();
            }
            // 5s沿用后台审计commit预算，只接受落盘回执，不以入队成功代替。
            writer
                .record_turn(MobileTurnOutcome {
                    key: turn_key,
                    state: MobileTurnState::Completed,
                    summary: Some(format!("durable {turn}")),
                })
                .expect("终态入队")
                .recv_timeout(Duration::from_secs(5))
                .expect("终态回执")
                .expect("终态commit");
        }
    };
    seed(&workspace, "a", "old-route", true, true);
    seed(&workspace, "b", "old-route", true, false);
    seed(&workspace, "frozen", "old-route", true, true);
    seed(&workspace, "current", "new-route", true, true);
    seed(&workspace, "unknown", "old-route", true, false);
    seed(&workspace, "desktop", "old-route", false, true);
    seed(&workspace, "mismatch", "old-route", true, true);
    seed("foreign-project", "foreign", "old-route", true, true);
    let deliveries = MobileTaskDeliveryStore::new(db.clone());
    let frozen = deliveries
        .record(
            &workspace,
            &key("frozen"),
            &TaskDeliveryContent::Available {
                review: review.clone(),
            },
        )
        .expect("崩溃前已冻结成果");
    std::fs::write(dir.path().join("result.txt"), "unrelated later edit")
        .expect("模拟重启后的无关编辑");
    let (snapshots, requests) = async_channel::bounded::<LiveSnapshotRequest>(1);
    let owner = std::thread::spawn(move || {
        while let Ok(mut call) = requests.recv_blocking() {
            call.projects[0].state = "open".into();
            call.projects[0].route_id = Some("new-route".into());
            call.projects[0].card_count = Some(0);
            call.projects[0].cards = Some(vec![]);
            let _ = call.reply.send(call.projects);
        }
    });
    let identity = ServerIdentity::generate().expect("随机服务身份");
    let machine = identity.machine_id();
    let cert = identity.certificate_der().to_vec();
    let server = MobileServer::start_with_snapshots(
        MobileAccess::with_identity_for_e2e(store, identity),
        vec!["127.0.0.1:0".parse().expect("回环")],
        Some(snapshots.clone()),
    )
    .expect("真实TLS服务，无项目文件采集适配器");
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let detail = json!({"type":"project_review","device_id":"reader","token":"a".repeat(64),
        "request":{"workspace_id":workspace,"route_id":"new-route","delivery_key":key("a")}});
    let mut unauthorized = detail.clone();
    unauthorized["token"] = json!("c".repeat(64));
    let denied = support::exchange(&mut socket, &machine, "denied", unauthorized);
    assert!(!denied["error"].is_null(), "错误凭据必须拒绝");
    assert!(
        deliveries
            .get(&workspace, &key("a"))
            .expect("未授权后查询")
            .is_none(),
        "未经授权不得触发持久恢复"
    );
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let mut expired = detail.clone();
    expired["request"]["route_id"] = json!("expired-client-route");
    assert_eq!(
        support::exchange(&mut socket, &machine, "expired", expired)["error"],
        "route_expired",
        "正确凭据也不能以过期路由判定哪些任务已结束"
    );
    assert!(
        deliveries
            .get(&workspace, &key("a"))
            .expect("过期路由后查询")
            .is_none(),
        "过期快照不能触发成果恢复写入"
    );
    // Project review is a one-response connection, including failures.
    let mut socket = support::connect(server.addresses()[0], cert.clone());
    let restored = support::exchange(&mut socket, &machine, "detail", detail.clone());
    assert_eq!(
        restored["error"], "delivery_unavailable",
        "旧路由已完成任务必须明确说明成果不可恢复，不能消失为not_found"
    );
    seed(&workspace, "b", "old-route", true, true);
    let mut socket = support::connect(server.addresses()[0], cert);
    let list = |pending: bool| {
        json!({"type":"task_deliveries","device_id":"reader","token":"a".repeat(64),
        "pending_only":pending,"request":{"workspace_id":workspace,"route_id":"new-route"}})
    };
    let page = support::exchange(&mut socket, &machine, "all", list(false));
    let items = page["result"]["deliveries"]["items"]
        .as_array()
        .expect("正式成果列表");
    assert_eq!(
        items.len(),
        3,
        "仅a/b不可用和原frozen三份成果，其他5轮不得冒领"
    );
    let a = items
        .iter()
        .find(|item| item["outcome"]["key"]["turn_id"] == "a")
        .expect("原任务a仍可见");
    let b = items
        .iter()
        .find(|item| item["outcome"]["key"]["turn_id"] == "b")
        .expect("原任务b仍可见");
    assert_eq!(a["status"], "window_closed", "a不能伪装成可查看正文");
    assert_eq!(b["status"], "window_closed", "b同样必须恢复明确不可用状态");
    assert_eq!(
        a["outcome"]["state"], "completed",
        "真实完成事件不能因缺成果变成执行失败"
    );
    assert_eq!(a["outcome"]["summary"], "durable a", "保留原Provider摘要");
    assert_eq!(a["file_count"], 0, "不可用记录不能含重启后文件");
    assert!(a["acceptance"].is_null(), "恢复不能自动验收");
    let inbox = support::exchange(&mut socket, &machine, "inbox", list(true));
    assert_eq!(
        inbox["result"]["deliveries"]["pending_count"], 1,
        "只有原frozen可供验收"
    );
    assert_eq!(
        inbox["result"]["deliveries"]["items"][0]["outcome"]["key"]["turn_id"], "frozen",
        "不可用记录不进待验收队列"
    );
    let reopened = MobileTaskDeliveryStore::new(db);
    assert_eq!(
        reopened.get(&workspace, &key("frozen")).expect("重开读取"),
        Some(frozen),
        "原成果正文、指纹和时间不可改写"
    );
    assert!(
        reopened
            .get(&workspace, &key("current"))
            .expect("新路由")
            .is_none(),
        "新路由仍在收集的任务不能提前判为不可用"
    );
    assert!(
        reopened
            .get(&workspace, &key("unknown"))
            .expect("未知任务")
            .is_none(),
        "没有真实完成事件不能伪造成果"
    );
    assert!(
        reopened
            .get(&workspace, &key("desktop"))
            .expect("桌面任务")
            .is_none(),
        "没有手机来源不能捏造关联"
    );
    assert!(
        reopened
            .get(&workspace, &key("mismatch"))
            .expect("错误session")
            .is_none(),
        "不能只匹配run和turn冒领另一session"
    );
    assert!(
        reopened
            .get("foreign-project", &key("foreign"))
            .expect("外部项目")
            .is_none(),
        "只能恢复已授权项目"
    );
    assert_eq!(
        support::exchange(&mut socket, &machine, "again", detail)["error"],
        "delivery_unavailable",
        "重复查询仍返回准确不可用状态"
    );
    assert_eq!(
        reopened
            .get(&workspace, &key("a"))
            .expect("恢复持久化")
            .expect("存在")
            .fingerprint,
        a["fingerprint"],
        "再次读取不生成新成果"
    );
    snapshots.close();
    server.shutdown();
    owner.join().expect("回收快照线程");
}
