use std::{sync::mpsc, time::Duration};
use storage::{
    DesktopContentKind, DesktopContentTabStore, DesktopWorkspaceStore, NewDesktopContentTab,
};

/// 场景：跨空间打开成果时，后台工作空间状态写入正持有 SQLite 写锁。
/// 构造：真实数据库与一个工作空间，另一连接以 IMMEDIATE 事务持有写锁。
/// 执行：后台调用公开 open_or_select，观察其等待，再释放锁并读取保存结果。
/// 预期：短暂竞争不会使成果打开失败；释放后恰好新增一个选中的冻结版本标签。
#[test]
fn opening_frozen_artifact_waits_for_concurrent_writer() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let workspace = DesktopWorkspaceStore::new(db.clone())
        .create("成果空间")
        .expect("应创建工作空间");
    let tabs = DesktopContentTabStore::new(db.clone());
    let mut writer = rusqlite::Connection::open(&db).expect("应打开第二连接");
    let transaction = writer
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .expect("应取得写锁");
    let request = NewDesktopContentTab {
        workspace_id: workspace.id.clone(),
        source_area_id: None,
        source_project_name: "成果项目".into(),
        kind: DesktopContentKind::File,
        resource: "gamma.txt".into(),
        version: Some("frozen-v1".into()),
        title: "gamma.txt".into(),
    };
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        started_tx.send(()).expect("主线程应等待启动");
        result_tx
            .send(tabs.open_or_select(request))
            .expect("主线程应保留结果接收端");
    });
    started_rx.recv().expect("后台任务应启动");
    // 250ms is shorter than SQLite's 5s busy timeout; no successful write is possible
    // while this transaction owns the writer reservation.
    let early = result_rx.recv_timeout(Duration::from_millis(250));
    transaction.commit().expect("应释放写锁");
    let waited = matches!(early, Err(mpsc::RecvTimeoutError::Timeout));
    let result = match early {
        Ok(result) => result,
        Err(_) => result_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("释放锁后应完成打开"),
    };
    worker.join().expect("后台任务应正常结束");
    assert!(waited, "短暂写锁应等待而不是立即返回错误：{result:?}");
    let opened = result.expect("竞争结束后应打开冻结成果");
    assert!(opened.selected, "新打开的成果应选中");
    assert_eq!(
        opened.version.as_deref(),
        Some("frozen-v1"),
        "应保留冻结版本"
    );
    let restored = DesktopContentTabStore::new(db)
        .list(&workspace.id)
        .expect("应读取已保存标签");
    assert_eq!(restored, vec![opened], "只保存一个目标成果，不丢失或重复");
}
