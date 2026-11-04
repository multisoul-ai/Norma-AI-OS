use rusqlite::Connection;
use storage::migrations::migrate_app_db;

fn tables(connection: &Connection) -> Vec<String> {
    connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("读取实际schema")
        .query_map([], |row| row.get(0))
        .expect("枚举实际表")
        .collect::<rusqlite::Result<_>>()
        .expect("读取全部表名")
}

/// 冷库迁移中途失败不能发布半套schema，也不能丢失原数据；修好旧结构后必须能重试。
/// 构造：只有mobile_task_deliveries原表和一行保留文本，缺少末尾索引必需的workspace_id。
/// 执行：正式migrate_app_db失败→检查原schema/数据→重命名保留旧表→再次正式迁移。
/// 预期：失败后仍只有原表、原行不变；重试后完整schema可用且旧行继续保留。
#[test]
fn app_migration_failure_preserves_original_schema_and_can_retry() {
    let directory = tempfile::tempdir().expect("隔离迁移失败库");
    let connection = Connection::open(directory.path().join("app.sqlite")).expect("打开隔离库");
    connection
        .execute_batch(
            "CREATE TABLE mobile_task_deliveries(original_text TEXT NOT NULL);
        INSERT INTO mobile_task_deliveries VALUES('preserve-original');",
        )
        .expect("真实不兼容旧表");
    assert!(
        migrate_app_db(&connection).is_err(),
        "缺索引列必须真实触发迁移错误"
    );
    assert_eq!(
        tables(&connection),
        vec!["mobile_task_deliveries"],
        "失败迁移不能留下半套新增表或完成标记"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT original_text FROM mobile_task_deliveries",
                [],
                |r| r.get::<_, String>(0)
            )
            .expect("失败后原数据"),
        "preserve-original",
        "失败不得丢失或改写已有数据"
    );
    assert!(connection.is_autocommit(), "失败必须释放事务，不可遗留写锁");
    connection
        .execute_batch("ALTER TABLE mobile_task_deliveries RENAME TO preserved_delivery;")
        .expect("保留原表并修复fixture冲突，不能删数据");
    migrate_app_db(&connection).expect("失败清理后正式迁移可重试");
    assert!(
        tables(&connection)
            .iter()
            .any(|name| name == "mobile_task_deliveries"),
        "重试必须创建完整末尾表"
    );
    assert_eq!(
        connection
            .query_row("SELECT original_text FROM preserved_delivery", [], |r| {
                r.get::<_, String>(0)
            })
            .expect("重试后原数据"),
        "preserve-original",
        "成功重试也不能删掉保留数据"
    );
}

/// 旧user_version=1写在迁移链中段，不能据此跳过缺失的后半部分App schema。
/// 构造：只有一行私有旧数据和user_version=1，没有任何App表。
/// 执行：正式migrate_app_db后查询最后的mobile_task_deliveries与原行。
/// 预期：尾部表真实存在，原数据不变，兼容版本仍为1；不得用旧版本号假冒完整状态。
#[test]
fn user_version_one_does_not_skip_the_remaining_app_schema() {
    let connection = Connection::open_in_memory().expect("隔离旧库");
    connection
        .execute_batch(
            "PRAGMA user_version=1; CREATE TABLE original_data(value TEXT NOT NULL);
        INSERT INTO original_data VALUES('keep');",
        )
        .expect("旧版本但不完整的schema");
    migrate_app_db(&connection).expect("必须补齐整条App迁移链");
    assert!(
        tables(&connection)
            .iter()
            .any(|name| name == "mobile_task_deliveries"),
        "旧版本号不能跳过尾部表"
    );
    assert_eq!(
        connection
            .query_row("SELECT value FROM original_data", [], |r| r
                .get::<_, String>(0))
            .expect("原行仍存在"),
        "keep",
        "升级不改原数据"
    );
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
            .expect("既有兼容版本"),
        1,
        "不能靠修改原版本断言制造通过"
    );
}
