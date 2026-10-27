use super::*;

/// 场景：报告已入队，取消时存储暂不可读，随后消费者全部释放。
/// 构造：隔离临时数据库；用非法文件暂代数据库，让实际 SQLite 路径失败。
/// 执行：取消并确认 flush 失败，释放 handle，再原子恢复原数据库。
/// 预期：宿主保留待保存状态并自动恢复为 Skipped，不依赖原调用者再次发起操作。
#[test]
fn report_delivery_recovers_after_write_failure_and_owner_release() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("app.db");
    let backup = dir.path().join("saved.db");
    let service = SharedContext::new(
        path.clone(),
        ContextScope {
            workspace_id: "w".into(),
            area_id: "a".into(),
            project: ContextProject::Personal,
            task_id: None,
            conversation_id: None,
        },
    )
    .unwrap();
    let runtime = super::super::super::agent_runtime::shared_tokio_runtime().unwrap();
    let record = runtime
        .block_on(service.run(None, |access| access.report("recover", "报告", vec![])))
        .unwrap();
    let delivery = service.report_delivery(record.reference.clone()).unwrap();
    delivery.set(ReportDelivery::Queued);
    runtime.block_on(delivery.flush()).unwrap();
    std::fs::rename(&path, &backup).unwrap();
    std::fs::write(&path, b"not a sqlite database").unwrap();
    delivery.cancel();
    let result = runtime.block_on(delivery.flush());
    assert!(result.is_err(), "保存失败不能报告 flush 成功");
    drop(delivery);
    // Replace atomically: a retry must never observe an absent path and create a new DB.
    std::fs::rename(&backup, &path).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let snapshot = runtime.block_on(service.snapshot()).unwrap();
        let status = snapshot.state.reports[&record.reference.event_id].delivery;
        if status == ReportDelivery::Skipped {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "释放全部消费者后仍应恢复已接受的取消状态，实际 {status:?}"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
