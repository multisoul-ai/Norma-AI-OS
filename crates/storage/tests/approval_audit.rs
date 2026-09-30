use serde_json::json;
use storage::{ApprovalAuditEntry, ApprovalAuditStore, BackgroundApprovalAudit};

fn entry(sequence: u64) -> ApprovalAuditEntry {
    ApprovalAuditEntry {
        run_id: "run-a".into(),
        sequence,
        node_id: 7,
        provider: "codex".into(),
        request_id: "run-a:1".into(),
        payload: json!({"state":if sequence==1 {"pending"} else {"chosen"}}),
    }
}

/// 审批审计落入指定项目数据库，重复写幂等，冲突整批回滚且后台回执反映真实提交。
///
/// 数据构造：项目 A/B 两个独立 SQLite，run-a 的 sequence 1/2；批次最多 256，列表读取上限为 256。
/// 执行过程：后台写两事件并等回执 → 重开读取 → 重试同一批次 → 冲突批含新事件与改写旧事件 → 查另一项目。
/// 预期结果：原始两行保持不变；冲突新行不存在，另一项目无记录；非法数据库路径返回错误而非成功。
#[test]
fn approval_audit_is_durable_idempotent_and_project_scoped() {
    let dir = tempfile::tempdir().expect("应创建私有测试目录");
    let store = ApprovalAuditStore::new(dir.path().join("a.sqlite"));
    let writer = BackgroundApprovalAudit::new(store.clone()).expect("应启动后台写入");
    let batch = vec![entry(1), entry(2)];
    writer
        .append(batch.clone())
        .expect("应入队")
        .recv_timeout(std::time::Duration::from_secs(5))
        .expect("本地 5s 内应有提交回执")
        .expect("应实际提交");
    drop(writer);
    let reopened = ApprovalAuditStore::new(dir.path().join("a.sqlite"));
    assert_eq!(
        reopened.list_run("run-a", 0, 256).expect("应重读"),
        batch,
        "重开后必须保留完整事件及顺序"
    );
    reopened.append(&batch).expect("同一数据重试应幂等");
    let mut conflict = entry(1);
    conflict.payload = json!({"state":"accept-for-session"});
    assert!(
        reopened.append(&[entry(3), conflict]).is_err(),
        "同一身份不同内容不可覆盖历史"
    );
    assert_eq!(
        reopened.list_run("run-a", 0, 256).expect("应读原始数据"),
        batch,
        "冲突批次的新行必须回滚且旧行保持"
    );
    assert_eq!(
        reopened.list_run("run-a", 1, 1).expect("应可分页"),
        vec![entry(2)],
        "cursor 必须严格排除已读事件"
    );
    assert!(
        ApprovalAuditStore::new(dir.path().join("b.sqlite"))
            .list_run("run-a", 0, 256)
            .expect("另一项目应可查询")
            .is_empty(),
        "同名运行不跨项目泄漏"
    );
    assert!(
        reopened.append(&[entry(0)]).is_err(),
        "序号从 1 开始，非法输入不能入库"
    );
    assert!(
        BackgroundApprovalAudit::new(ApprovalAuditStore::new(dir.path().to_path_buf())).is_err(),
        "不能把目录当数据库并伪造写成功"
    );
}
