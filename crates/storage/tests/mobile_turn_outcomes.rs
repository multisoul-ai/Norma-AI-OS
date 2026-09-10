use std::time::Duration;
use storage::{
    ApprovalAuditEntry, ApprovalAuditStore, BackgroundApprovalAudit, MobileTurnKey,
    MobileTurnOutcome, MobileTurnState, MobileTurnStore,
};

fn key(turn: &str) -> MobileTurnKey {
    MobileTurnKey {
        node_id: 7,
        profile: "Codex".into(),
        run_id: "run-a".into(),
        session_id: "session-a".into(),
        turn_id: turn.into(),
    }
}

/// 具体回合的完成证据与审批共用后台写入，旧任务不随下一回合变化，冲突不能改写。
///
/// 数据构造：同run/session的turn-a完成、turn-b失败；另一项目数据库没有记录。
/// 执行过程：同后台writer提交审批与两次终态 → 重开按精确身份查询 → 重试/冲突 → 跨项目/错节点。
/// 预期结果：a仍完成、b失败；审批原样保存，错节点/会话/项目无结果，冲突保留原始终态。
#[test]
fn turn_outcomes_are_durable_exact_and_immutable() {
    let dir = tempfile::tempdir().expect("临时项目");
    let path = dir.path().join("project.sqlite");
    let audit = ApprovalAuditStore::new(path.clone());
    let writer = BackgroundApprovalAudit::new(audit.clone()).expect("既有后台writer");
    let approval = ApprovalAuditEntry {
        run_id: "run-a".into(),
        sequence: 1,
        node_id: 7,
        provider: "codex".into(),
        request_id: "approval-a".into(),
        payload: serde_json::json!({"state":"pending"}),
    };
    // 5s=现有审计本地落盘上限；等待真实commit，不以入队代替持久化。
    let budget = Duration::from_secs(5);
    writer
        .append(vec![approval.clone()])
        .expect("审批入队")
        .recv_timeout(budget)
        .expect("审批回执")
        .expect("审批提交");
    let completed = MobileTurnOutcome {
        key: key("turn-a"),
        state: MobileTurnState::Completed,
    };
    writer
        .record_turn(completed.clone())
        .expect("a入队")
        .recv_timeout(budget)
        .expect("a回执")
        .expect("a提交");
    let failed = MobileTurnOutcome {
        key: key("turn-b"),
        state: MobileTurnState::Failed,
    };
    writer
        .record_turn(failed.clone())
        .expect("b入队")
        .recv_timeout(budget)
        .expect("b回执")
        .expect("b提交");
    writer
        .record_turn(completed.clone())
        .expect("重复入队")
        .recv_timeout(budget)
        .expect("重复回执")
        .expect("同结果幂等");
    writer
        .record_turn(MobileTurnOutcome {
            key: key("turn-a"),
            state: MobileTurnState::Failed,
        })
        .expect("冲突入队")
        .recv_timeout(budget)
        .expect("冲突回执")
        .expect_err("同回合不同结果不能覆盖");
    drop(writer);
    let store = MobileTurnStore::new(path);
    assert_eq!(
        store.get(&key("turn-a")).expect("a查询"),
        Some(completed),
        "后续失败不能改写旧回合完成"
    );
    assert_eq!(
        store.get(&key("turn-b")).expect("b查询"),
        Some(failed),
        "b必须保留自己的失败"
    );
    assert_eq!(
        audit.list_run("run-a", 0, 1).expect("审批查询"),
        vec![approval],
        "终态不占用审批sequence或污染审批表"
    );
    let mut wrong = key("turn-a");
    wrong.node_id = 8;
    assert!(
        store.get(&wrong).expect("错节点查询").is_none(),
        "同run/turn不能改派节点"
    );
    wrong = key("turn-a");
    wrong.session_id = "other-session".into();
    assert!(
        store.get(&wrong).expect("错会话查询").is_none(),
        "不能认领另一会话结果"
    );
    assert!(
        MobileTurnStore::new(dir.path().join("other.sqlite"))
            .get(&key("turn-a"))
            .expect("其他项目")
            .is_none(),
        "项目之间不能共享结果"
    );
    assert!(
        store.get(&key("unobserved")).expect("未观察回合").is_none(),
        "没有事件不能推断完成"
    );
}

/// AppDB中的终态按显式项目分区，后台writer可在窗口结束后保存已收到的证据。
///
/// 数据构造：项目审批库与共享AppDB分离；A/B使用相同run/turn但只有A有事件。
/// 执行过程：配置A终态store → 写入后释放writer → 重开AppDB按A/B查询 → 验证审批库无镜像。
/// 预期结果：A可恢复完成；B无记录，项目库无重复影子数据，AppDB不应被创建项目表。
#[test]
fn turn_outcomes_in_app_database_remain_workspace_scoped() {
    let dir = tempfile::tempdir().expect("临时目录");
    let app_path = dir.path().join("app.sqlite");
    let project_path = dir.path().join("project.sqlite");
    let store = MobileTurnStore::for_workspace(app_path.clone(), "workspace-a".into());
    let writer = BackgroundApprovalAudit::new(
        ApprovalAuditStore::new(project_path.clone()).with_turn_store(store.clone()),
    )
    .expect("配置共享后台存储");
    let outcome = MobileTurnOutcome {
        key: key("turn-a"),
        state: MobileTurnState::Completed,
    };
    // 5s=既有本地SQLite回执上限，不以线程退出或排队成功替代commit。
    writer
        .record_turn(outcome.clone())
        .expect("入队")
        .recv_timeout(Duration::from_secs(5))
        .expect("回执")
        .expect("提交");
    drop(writer);
    let page = store.after(0, 1).expect("读取有界终态事件");
    assert_eq!(page.len(), 1, "事件页必须包含A已提交的终态");
    assert_eq!(page[0].outcome, outcome, "事件不能由卡片显示状态推断");
    assert!(
        page[0].recorded_at_unix > 0,
        "事件需要真实提交时间以限制推送有效期"
    );
    assert_eq!(
        store.latest_sequence().expect("游标"),
        page[0].sequence,
        "水位必须覆盖最后提交"
    );
    assert!(
        store.after(page[0].sequence, 1).expect("续页").is_empty(),
        "续页不能重放旧终态"
    );
    assert!(store.after(0, 0).is_err(), "零页长不能产生不前进的游标");
    assert!(
        MobileTurnStore::for_workspace(app_path.clone(), "workspace-b".into())
            .after(0, 1)
            .expect("B事件")
            .is_empty(),
        "事件分页不得跨项目"
    );
    assert_eq!(
        store.get(&key("turn-a")).expect("A恢复"),
        Some(outcome),
        "窗口生命周期不能抹掉已存终态"
    );
    assert!(
        MobileTurnStore::for_workspace(app_path.clone(), "workspace-b".into())
            .get(&key("turn-a"))
            .expect("B查询")
            .is_none(),
        "相同回合ID不能跨项目读取"
    );
    assert!(
        MobileTurnStore::new(project_path)
            .get(&key("turn-a"))
            .expect("项目库查询")
            .is_none(),
        "AppDB配置不能再写项目库影子数据"
    );
    let connection = rusqlite::Connection::open(app_path).expect("检查AppDB");
    let project_tables: i64 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='canvas_snapshots'",
            [],
            |r| r.get(0),
        )
        .expect("读取表目录");
    assert_eq!(project_tables, 0, "查询AppDB不能误跑项目migration");
}
