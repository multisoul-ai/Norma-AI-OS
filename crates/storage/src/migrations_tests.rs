use super::*;

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
}
