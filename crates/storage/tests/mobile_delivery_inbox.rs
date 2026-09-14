use std::time::Duration;
use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, MobileCommandStore, MobileTaskDeliveryStore,
    MobileTurnKey, MobileTurnOutcome, MobileTurnState, MobileTurnStore, NewMobileCommand,
    TaskDeliveryContent,
};
use workspace::review::{FileChange, ProjectReview};

/// 待验收列表只含完整且未验收的成功成果，总数不受20条分页限制，也不扫描成果正文。
///
/// 数据构造：project-a的22份完整成果中验收最新1份，剩21=首页20+尾页1；
///   再加失败、缺起点、冲突各1份及project-b同身份1份，均不计入a的待验收数。
/// 执行过程：正式后台writer保存终态及冻结成果→验收最新→读首页/尾页/单卡/他项目→核对SQL索引。
/// 预期结果：总数21，分页保留turn-20到turn-0；排除已验收/失败/不可用/冲突/他项目；
///   统计查询使用部分索引，不能为计数读取大体积data_json。
#[test]
fn pending_deliveries_count_and_pages_are_scoped_and_indexed() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let db = dir.path().join("app.sqlite");
    let deliveries = MobileTaskDeliveryStore::new(db.clone());
    let persist = |workspace: &str, turn: &str, state, content| {
        let writer = BackgroundApprovalAudit::new(
            ApprovalAuditStore::new(dir.path().join(format!("{workspace}-audit.sqlite")))
                .with_turn_store(MobileTurnStore::for_workspace(db.clone(), workspace.into())),
        )
        .expect("正式终态writer");
        let key = MobileTurnKey {
            node_id: 7,
            profile: "Codex".into(),
            run_id: "run".into(),
            session_id: "session".into(),
            turn_id: turn.into(),
        };
        // 5s为现有后台writer的提交等待预算；必须观察实际commit。
        writer
            .record_turn(MobileTurnOutcome {
                summary: None,
                key: key.clone(),
                state,
            })
            .expect("入队")
            .recv_timeout(Duration::from_secs(5))
            .expect("回执")
            .expect("提交");
        deliveries
            .record(workspace, &key, &content)
            .expect("冻结成果")
    };
    let available = TaskDeliveryContent::Available {
        review: ProjectReview {
            base_commit: None,
            conflicted: false,
            files: vec![],
        },
    };
    for index in 0..22 {
        persist(
            "project-a",
            &format!("turn-{index}"),
            MobileTurnState::Completed,
            available.clone(),
        );
    }
    persist(
        "project-a",
        "failed",
        MobileTurnState::Failed,
        available.clone(),
    );
    persist(
        "project-a",
        "missing",
        MobileTurnState::Completed,
        TaskDeliveryContent::Unavailable {
            reason: "baseline_missing".into(),
        },
    );
    persist(
        "project-a",
        "conflicted",
        MobileTurnState::Completed,
        TaskDeliveryContent::Available {
            review: ProjectReview {
                base_commit: None,
                conflicted: true,
                files: vec![FileChange {
                    path: "conflict.rs".into(),
                    status: "UU".into(),
                    before: None,
                    after: None,
                    index: None,
                    diff: None,
                    staged_diff: None,
                }],
            },
        },
    );
    let foreign = persist(
        "project-b",
        "turn-20",
        MobileTurnState::Completed,
        available,
    );
    let accepted = deliveries
        .list("project-a", None, None)
        .expect("记录")
        .into_iter()
        .find(|d| d.outcome.key.turn_id == "turn-21")
        .expect("最新完成回合");
    let commands = MobileCommandStore::new(db.clone());
    let command = NewMobileCommand {
        device_id: "phone".into(),
        operation_id: "accept".into(),
        workspace_id: "project-a".into(),
        route_id: "route".into(),
        owner_id: "owner".into(),
        request_hash: "a".repeat(64),
    };
    commands.reserve(&command).expect("预留");
    commands
        .accept_delivery(&command, &accepted.outcome.key, &accepted.fingerprint)
        .expect("正式验收");
    let (first, count) = deliveries
        .page("project-a", None, None, true)
        .expect("待验收首页");
    assert_eq!(count, 21, "总数必须包含首页之外的那一份成果");
    assert_eq!(first.len(), 20, "列表仍是有界元数据页");
    assert_eq!(
        first[0].outcome.key.turn_id, "turn-20",
        "不能显示已验收的turn-21"
    );
    assert_eq!(first[19].outcome.key.turn_id, "turn-1", "最早成果留到尾页");
    assert!(
        !first.iter().any(|d| d.outcome.key.turn_id == "failed"),
        "失败不能当作待验收成功成果"
    );
    assert!(
        !first.iter().any(|d| d.outcome.key.turn_id == "missing"),
        "没有可信内容不能验收"
    );
    assert!(
        !first.iter().any(|d| d.outcome.key.turn_id == "conflicted"),
        "存在冲突的成果不能验收"
    );
    assert!(
        !first.iter().any(|d| d.fingerprint == foreign.fingerprint),
        "另一项目不能混入"
    );
    let (older, total) = deliveries
        .page("project-a", None, Some(first[19].sequence), true)
        .expect("尾页");
    assert_eq!(total, 21, "翻页游标不能缩小项目待验收总数");
    assert_eq!(older.len(), 1, "尾页只有遗漏的第一份成果");
    assert_eq!(
        older[0].outcome.key.turn_id, "turn-0",
        "最早的未验收成果必须可达"
    );
    assert_eq!(
        deliveries
            .page("project-a", Some(8), None, true)
            .expect("他卡")
            .1,
        0,
        "节点范围不可扩大"
    );
    assert_eq!(
        deliveries
            .page("project-b", None, None, true)
            .expect("他项目")
            .1,
        1,
        "同身份按项目隔离"
    );
    let conn = rusqlite::Connection::open(db).expect("只读检查查询计划");
    let mut q = conn.prepare("EXPLAIN QUERY PLAN SELECT count(*) FROM mobile_task_deliveries d LEFT JOIN mobile_delivery_acceptances a ON a.workspace_id=d.workspace_id AND a.run_id=d.run_id AND a.turn_id=d.turn_id WHERE d.workspace_id='project-a' AND json_extract(data_json,'$.content.status')='available' AND json_extract(data_json,'$.outcome.state')='completed' AND json_extract(data_json,'$.content.review.conflicted')=0 AND a.run_id IS NULL").expect("统计计划");
    let plan = q
        .query_map([], |r| r.get::<_, String>(3))
        .expect("计划")
        .collect::<Result<Vec<_>, _>>()
        .expect("完整计划")
        .join("\n");
    assert!(
        plan.contains("mobile_task_delivery_inbox"),
        "统计必须走不含文件正文的部分索引：{plan}"
    );
    assert!(!plan.contains("SCAN d"), "不能扫描成果正文表：{plan}");
}
