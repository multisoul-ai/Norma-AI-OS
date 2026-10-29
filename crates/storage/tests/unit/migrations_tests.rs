use super::*;

/// 场景：多个后台连接同时为旧表增加同一列，不能重复 ALTER。
/// 数据构造：旧表无 outcome；一个连接暂持写锁，4 个迁移连接各自读到缺列。
/// 执行过程：等 4 个连接进入写锁等待后释放锁，再收取全部迁移结果。
/// 预期结果：4 次迁移都成功，仅新增一列，原记录保持不变。
#[test]
fn concurrent_column_migration_rechecks_after_write_lock() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};
    static WAITERS: AtomicUsize = AtomicUsize::new(0);
    WAITERS.store(0, Ordering::SeqCst);
    let directory = tempfile::tempdir().expect("隔离迁移目录");
    let path = directory.path().join("columns.sqlite");
    let owner = Connection::open(&path).expect("旧连接");
    owner.execute_batch("CREATE TABLE legacy(id INTEGER PRIMARY KEY); INSERT INTO legacy VALUES(7); BEGIN IMMEDIATE;").expect("旧表与暂持写锁");
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || {
                let connection = Connection::open(path).expect("迁移连接");
                connection
                    .busy_handler(Some(|attempt| {
                        if attempt == 0 {
                            WAITERS.fetch_add(1, Ordering::SeqCst);
                        }
                        std::thread::sleep(Duration::from_millis(1));
                        attempt < 2_000
                    }))
                    .expect("锁等待观察器");
                ensure_column(&connection, "legacy", "outcome", "outcome TEXT")
            })
        })
        .collect();
    let deadline = Instant::now() + Duration::from_secs(2);
    while WAITERS.load(Ordering::SeqCst) < 4 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    let waiting = WAITERS.load(Ordering::SeqCst);
    owner.execute_batch("COMMIT").expect("释放写锁");
    let outcomes: Vec<_> = workers
        .into_iter()
        .map(|worker| worker.join().expect("迁移线程"))
        .collect();
    assert_eq!(waiting, 4, "四个连接均需先观察旧结构并进入写锁等待");
    assert!(
        outcomes.iter().all(Result::is_ok),
        "锁内应重新检查列，不能重复 ALTER：{outcomes:?}"
    );
    let columns: i64 = owner
        .query_row(
            "SELECT count(*) FROM pragma_table_info('legacy') WHERE name='outcome'",
            [],
            |row| row.get(0),
        )
        .expect("列数");
    assert_eq!(columns, 1, "新增列只存在一次");
    let unchanged: i64 = owner
        .query_row(
            "SELECT count(*) FROM legacy WHERE id=7 AND outcome IS NULL",
            [],
            |row| row.get(0),
        )
        .expect("原记录");
    assert_eq!(unchanged, 1, "迁移不得删除或更改原记录");
}

/// M-1: App DB 与 Project DB migration 都可以重复执行。
///
/// 数据构造（含关键数值的推导过程）：
///   app_db = temp/app.sqlite，初始不存在
///   project_db = temp/project.sqlite，初始不存在
///   migration count = 2，第二次执行应命中 CREATE TABLE IF NOT EXISTS
///
/// 执行过程（逐步说明系统如何处理）：
///   1. open_app_connection(app_db) → 创建 app 表并设置 user_version=1
///   2. 再次 open_app_connection(app_db) → 不应重复建表失败
///   3. open_project_connection(project_db) → 创建 project 表并设置 user_version=1
///   4. 再次 open_project_connection(project_db) → 不应重复建表失败
///
/// 预期结果：
///   - 正断言：两个 sqlite 文件都被创建
///   - 正断言：两个库的 user_version 都是 1
///   - 负断言：重复 migration 不应返回错误
#[test]
fn test_app_and_project_migrations_are_idempotent() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let app_db = temp.path().join("app.sqlite");
    let project_db = temp.path().join("project.sqlite");

    let first_app = open_app_connection(&app_db);
    let second_app = open_app_connection(&app_db);
    let first_project = open_project_connection(&project_db);
    let second_project = open_project_connection(&project_db);

    assert!(
        first_app.is_ok() && second_app.is_ok(),
        "app migration 重复执行应成功，实际: {:?} / {:?}",
        first_app.err(),
        second_app.err()
    );
    assert!(
        first_project.is_ok() && second_project.is_ok(),
        "project migration 重复执行应成功，实际: {:?} / {:?}",
        first_project.err(),
        second_project.err()
    );
    assert!(app_db.is_file(), "app migration 应创建 app.sqlite 文件");
    assert!(
        project_db.is_file(),
        "project migration 应创建 project.sqlite 文件"
    );

    let app_version: i64 = open_app_connection(&app_db)
        .expect("app db should reopen")
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("app user_version should be readable");
    let project_version: i64 = open_project_connection(&project_db)
        .expect("project db should reopen")
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("project user_version should be readable");

    assert_eq!(app_version, 1, "app db user_version 应为 1");
    assert_eq!(project_version, 1, "project db user_version 应为 1");

    let voice_history_count: i64 = open_project_connection(&project_db)
        .expect("project db should reopen")
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'voice_history'",
            [],
            |row| row.get(0),
        )
        .expect("voice_history table existence should be readable");
    assert_eq!(
        voice_history_count, 1,
        "project migration 应创建 voice_history 表用于语音审计历史"
    );
}
