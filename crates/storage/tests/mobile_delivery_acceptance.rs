use std::time::Duration;
use storage::{
    ApprovalAuditStore, BackgroundApprovalAudit, MobileCommandStore, MobileTaskDeliveryStore,
    MobileTurnKey, MobileTurnOutcome, MobileTurnState, MobileTurnStore, NewMobileCommand,
    TaskDeliveryContent,
};
use workspace::review::{FileChange, ProjectReview};

/// 验收仅绑定准确且可用的已完成交付，与操作回执原子落盘；重复和其他手机不能覆盖。
///
/// 数据构造：节点7、同run/session的completed/failed及不可用/冲突/回滚/竞争回合；普通成果零文件，冲突成果含一份UU文件，设备a/b各独立操作。
/// 执行过程：终态fixture/交付 → 错指纹/失败拒绝 → 准确验收/历史标记/重复拒绝 → 重开 → 注入回执更新失败 → 事务全回滚 → 不可用/冲突成果拒绝 → 两写线程同时竞争 → 注入摘要验收指纹损坏。
/// 预期结果：仅可用completed保存验收与succeeded回执，历史列表关联同一验收；回执写失败不留半次决定；失败/错指纹/不可用无验收，另一手机不能改写决定。
#[test]
fn acceptance_is_atomic_exact_and_never_transfers_to_another_turn() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let db = dir.path().join("app.sqlite");
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(dir.path().join("audit.sqlite"))
            .with_turn_store(MobileTurnStore::for_workspace(db.clone(), "project".into())),
    )
    .expect("真实审计落盘管线");
    let key = |turn: &str| MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run".into(),
        session_id: "session".into(),
        turn_id: turn.into(),
    };
    //5s是现有本地writer提交预算，等待commit而不是仅检查入队。
    writer
        .record_turn(MobileTurnOutcome {
            key: key("done"),
            state: MobileTurnState::Completed,
        })
        .expect("入队完成")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执")
        .expect("落盘");
    writer
        .record_turn(MobileTurnOutcome {
            key: key("failed"),
            state: MobileTurnState::Failed,
        })
        .expect("入队失败")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执")
        .expect("落盘");
    let deliveries = MobileTaskDeliveryStore::new(db.clone());
    let content = TaskDeliveryContent::Available {
        review: ProjectReview {
            base_commit: None,
            conflicted: false,
            files: vec![],
        },
    };
    let done = deliveries
        .record("project", &key("done"), &content)
        .expect("完整空成果");
    let failed = deliveries
        .record("project", &key("failed"), &content)
        .expect("失败回合仍可查看");
    let command = |device: &str, op: &str| NewMobileCommand {
        device_id: device.into(),
        operation_id: op.into(),
        workspace_id: "project".into(),
        route_id: "route".into(),
        owner_id: "owner".into(),
        request_hash: "a".repeat(64),
    };
    let store = MobileCommandStore::new(db.clone());
    let source = command("phone-a", "accept");
    store.reserve(&source).expect("预留唯一操作");
    assert!(
        store
            .accept_delivery(&source, &key("done"), &"b".repeat(64))
            .is_err(),
        "错内容指纹不可验收"
    );
    assert!(
        store
            .accept_delivery(&source, &key("failed"), &failed.fingerprint)
            .is_err(),
        "失败回合不能验收"
    );
    assert!(
        store
            .delivery_acceptance("project", &key("failed"))
            .expect("查询失败回合")
            .is_none(),
        "失败回合不留下验收占位"
    );
    let accepted = store
        .accept_delivery(&source, &key("done"), &done.fingerprint)
        .expect("通过验收");
    assert_eq!(accepted.key, key("done"), "验收必须保存精确回合");
    assert_eq!(
        accepted.fingerprint, done.fingerprint,
        "验收固定当前所见文件内容"
    );
    let history = deliveries.list("project", Some(7), None).expect("历史摘要");
    let done_summary = history
        .iter()
        .find(|item| item.outcome.key == key("done"))
        .expect("已完成摘要");
    assert_eq!(
        done_summary.acceptance.as_ref(),
        Some(&accepted),
        "历史列表必须关联准确的已持久化验收"
    );
    let failed_summary = history
        .iter()
        .find(|item| item.outcome.key == key("failed"))
        .expect("失败摘要");
    assert!(
        failed_summary.acceptance.is_none(),
        "相同节点的失败回合不能继承已验收标记"
    );
    assert!(
        deliveries
            .list("another-project", Some(7), None)
            .expect("跨项目历史")
            .is_empty(),
        "验收关联不能扩大项目读取范围"
    );
    let receipt = store
        .get("phone-a", "accept", "project", "owner")
        .expect("读回执")
        .expect("原请求");
    assert_eq!(
        receipt.state, "succeeded",
        "保存验收和操作成功必须在同一事务"
    );
    assert_eq!(
        receipt.result.as_ref().expect("结果")["fingerprint"],
        done.fingerprint,
        "丢失响应后可证明验收了哪份内容"
    );
    assert_eq!(
        store
            .accept_delivery(&source, &key("done"), &done.fingerprint)
            .expect("重放"),
        accepted,
        "同一请求不得改写时间或验收人"
    );
    let other = command("phone-b", "accept");
    store.reserve(&other).expect("另一手机请求");
    assert!(
        store
            .accept_delivery(&other, &key("done"), &done.fingerprint)
            .is_err(),
        "另一手机不可重复验收覆盖原始决定"
    );
    let reopened = MobileCommandStore::new(db.clone());
    assert_eq!(
        reopened
            .delivery_acceptance("project", &key("done"))
            .expect("重开")
            .expect("验收"),
        accepted,
        "重启后保留准确决定"
    );
    assert!(
        reopened
            .delivery_acceptance("another-project", &key("done"))
            .expect("其他项目")
            .is_none(),
        "项目之间不能读到验收"
    );
    assert!(
        reopened
            .delivery_acceptance("project", &key("new-turn"))
            .expect("后续任务")
            .is_none(),
        "后续任务不能继承旧验收标记"
    );
    //相同审计管线再造一份完整成果，注入回执更新失败以验证事务回滚。
    writer
        .record_turn(MobileTurnOutcome {
            key: key("rollback"),
            state: MobileTurnState::Completed,
        })
        .expect("入队")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执")
        .expect("提交");
    let rollback = deliveries
        .record("project", &key("rollback"), &content)
        .expect("回滚测试成果");
    let source = command("phone-a", "rollback");
    store.reserve(&source).expect("预留");
    let connection = rusqlite::Connection::open(&db).expect("隔离故障注入");
    connection.execute_batch("CREATE TRIGGER fail_acceptance_receipt BEFORE UPDATE ON mobile_commands WHEN NEW.operation_id='rollback' BEGIN SELECT RAISE(ABORT,'test receipt failure'); END;").expect("仅测试事务故障");
    assert!(
        store
            .accept_delivery(&source, &key("rollback"), &rollback.fingerprint)
            .is_err(),
        "回执更新失败时整次验收失败"
    );
    assert!(
        store
            .delivery_acceptance("project", &key("rollback"))
            .expect("查验收")
            .is_none(),
        "不能留下有决定但无成功回执的半次提交"
    );
    assert_eq!(
        store
            .get("phone-a", "rollback", "project", "owner")
            .expect("查询")
            .expect("预留")
            .state,
        "accepted",
        "事务回滚后原操作仍待执行，不伪造失败完成"
    );
    connection
        .execute_batch("DROP TRIGGER fail_acceptance_receipt")
        .expect("移除测试故障");
    let restored = store
        .accept_delivery(&source, &key("rollback"), &rollback.fingerprint)
        .expect("无副作用回滚后允许原请求完成");
    assert_eq!(restored.operation_id, "rollback", "回滚不能生成另一操作ID");
    writer
        .record_turn(MobileTurnOutcome {
            key: key("unavailable"),
            state: MobileTurnState::Completed,
        })
        .expect("入队")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执")
        .expect("提交");
    let unavailable = deliveries
        .record(
            "project",
            &key("unavailable"),
            &TaskDeliveryContent::Unavailable {
                reason: "capture_expired".into(),
            },
        )
        .expect("无法捕获也保留终态");
    let source = command("phone-a", "unavailable");
    store.reserve(&source).expect("预留");
    assert!(
        store
            .accept_delivery(&source, &key("unavailable"), &unavailable.fingerprint)
            .is_err(),
        "completed不足以验收，没有可读成果必须拒绝"
    );
    assert!(
        store
            .delivery_acceptance("project", &key("unavailable"))
            .expect("查询")
            .is_none(),
        "不可用成果不出现已验收标记"
    );
    writer
        .record_turn(MobileTurnOutcome {
            key: key("conflict"),
            state: MobileTurnState::Completed,
        })
        .expect("入队")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执")
        .expect("提交");
    let conflict = deliveries
        .record(
            "project",
            &key("conflict"),
            &TaskDeliveryContent::Available {
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
        )
        .expect("冲突fixture");
    let source = command("phone-a", "conflict");
    store.reserve(&source).expect("预留");
    assert!(
        store
            .accept_delivery(&source, &key("conflict"), &conflict.fingerprint)
            .is_err(),
        "未解决冲突禁止验收"
    );
    assert!(
        store
            .delivery_acceptance("project", &key("conflict"))
            .expect("查询")
            .is_none(),
        "冲突成果不记录成功决定"
    );
    writer
        .record_turn(MobileTurnOutcome {
            key: key("race"),
            state: MobileTurnState::Completed,
        })
        .expect("入队")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执")
        .expect("提交");
    let race = deliveries
        .record("project", &key("race"), &content)
        .expect("并发测试成果");
    let a = command("phone-a", "race");
    let b = command("phone-b", "race");
    store.reserve(&a).expect("预留手机A");
    store.reserve(&b).expect("预留手机B");
    //3个参与者=两个SQLite写线程+测试线程；同步开始，检查真实事务竞争而非顺序调用。
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let left = {
        let barrier = barrier.clone();
        let store = store.clone();
        let key = key("race");
        let fingerprint = race.fingerprint.clone();
        std::thread::spawn(move || {
            barrier.wait();
            store.accept_delivery(&a, &key, &fingerprint)
        })
    };
    let right = {
        let barrier = barrier.clone();
        let store = store.clone();
        let key = key("race");
        let fingerprint = race.fingerprint.clone();
        std::thread::spawn(move || {
            barrier.wait();
            store.accept_delivery(&b, &key, &fingerprint)
        })
    };
    barrier.wait();
    let left = left.join().expect("手机A线程");
    let right = right.join().expect("手机B线程");
    assert_eq!(
        usize::from(left.is_ok()) + usize::from(right.is_ok()),
        1,
        "两台手机同时决定，只能一份事务提交"
    );
    let (winner, loser) = match (left, right) {
        (Ok(accepted), Err(storage::StorageError::InvalidMobileCommand(code))) => {
            assert_eq!(code, "delivery_accepted", "败者明确因已验收被拒绝");
            (accepted, "phone-b")
        }
        (Err(storage::StorageError::InvalidMobileCommand(code)), Ok(accepted)) => {
            assert_eq!(code, "delivery_accepted", "败者明确因已验收被拒绝");
            (accepted, "phone-a")
        }
        _ => panic!("并发必须恰有一份验收和一个不可覆盖的拒绝"),
    };
    assert_eq!(
        store
            .get(loser, "race", "project", "owner")
            .expect("败者")
            .expect("原预留")
            .state,
        "accepted",
        "败者不能产生第二份成功回执"
    );
    assert_eq!(
        store
            .get(&winner.device_id, "race", "project", "owner")
            .expect("胜者")
            .expect("原预留")
            .state,
        "succeeded",
        "唯一胜者必须同时保存成功回执"
    );
    assert_eq!(
        store
            .delivery_acceptance("project", &key("race"))
            .expect("保存的决定")
            .expect("存在"),
        winner,
        "落盘记录不得由败者覆盖"
    );
    connection.execute("UPDATE mobile_delivery_acceptances SET acceptance_json=json_set(acceptance_json,'$.fingerprint',?1) WHERE workspace_id='project' AND turn_id='done'", [&"b".repeat(64)]).expect("仅修改隔离fixture的验收指纹");
    assert!(
        deliveries.list("project", Some(7), None).is_err(),
        "列表关联必须复核指纹，损坏记录不能显示为已验收"
    );
}
