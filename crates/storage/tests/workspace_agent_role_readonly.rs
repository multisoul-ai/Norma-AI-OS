use agent::workspace_dispatch::WorkspaceCardRef;
use storage::{StorageError, WorkspaceAgentRoleStore, WorkspaceCallResolution};

fn card() -> WorkspaceCardRef {
    WorkspaceCardRef {
        workspace_id: "alpha".into(),
        area_id: "area-a".into(),
        node_id: 1,
        card_instance_id: "card-a".into(),
    }
}

/// 目录读取在不存在的 AppDB 与未登记的旧 schema 上不创建、迁移或写入。
/// 数据构造：missing/app.sqlite 从未创建；legacy.sqlite 仅有一张自定义旧表。
/// 执行过程：调用 snapshot/list/get/resolve 后检查缺文件仍缺失；读取旧 DB 前后逐字节比较。
/// 预期结果：返回 schema_present=false 的缺口，角色与键均空；两个路径均无查询副作用。
#[test]
fn directory_reads_never_create_or_migrate_database_files() {
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let missing = temp.path().join("missing/app.sqlite");
    let store = WorkspaceAgentRoleStore::new(missing.clone());
    let snapshot = store.snapshot("alpha").expect("缺失目录应可读为未登记");
    assert!(!snapshot.schema_present, "缺文件不能冒充完成名称登记");
    assert!(snapshot.roles.is_empty(), "缺文件没有已知角色");
    assert!(snapshot.call_keys.is_empty(), "缺文件没有可寻址呼叫键");
    assert!(
        store.list("alpha").expect("应可读列表").is_empty(),
        "缺文件列表应为空"
    );
    assert_eq!(
        store.get(&card()).expect("应读单卡"),
        None,
        "缺文件不能凭节点创建角色"
    );
    assert_eq!(
        store.resolve("alpha", "Alan").expect("应查名称"),
        WorkspaceCallResolution::Missing,
        "未登记名称不能寻址"
    );
    assert!(!missing.exists(), "只读查询不能创建 AppDB");
    assert!(
        !missing.parent().expect("应有父路径").exists(),
        "只读查询不能创建数据目录"
    );

    let legacy = temp.path().join("legacy.sqlite");
    {
        let connection = rusqlite::Connection::open(&legacy).expect("应构造旧数据库边界");
        connection
            .execute_batch("CREATE TABLE legacy_fixture(id INTEGER PRIMARY KEY)")
            .expect("应构造未迁移旧 schema");
    }
    let before = std::fs::read(&legacy).expect("应读取旧 DB 字节");
    let store = WorkspaceAgentRoleStore::new(legacy.clone());
    let snapshot = store.snapshot("alpha").expect("旧 DB 应报告缺登记 schema");
    assert!(!snapshot.schema_present, "旧 DB 不得被查询自动升级");
    assert_eq!(snapshot.revision, 0, "未登记 schema 不具有目录版本");
    assert_eq!(
        std::fs::read(&legacy).expect("应读取查询后 DB 字节"),
        before,
        "读取必须不迁移、不修改旧数据库"
    );
}

/// SQLite 写锁导致删除失败时，角色及当前/失效占用必须保持原样。
/// 数据构造：Alan 改 Bella，四键；另一独立 SQLite 连接持有 IMMEDIATE 写事务。
/// 执行过程：在锁未释放时调用已确认卡片删除接口，收到 SQLite 错误后 rollback 外部锁再读快照。
/// 预期结果：删除失败返回错误，原快照完整保留，旧名仍占用，新中文别名仍有效。
#[test]
fn failed_database_deletion_preserves_every_owned_key() {
    let temp = tempfile::tempdir().expect("应建立隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = WorkspaceAgentRoleStore::new(path.clone());
    store.create(&card(), "Alan", true).expect("应创建原角色");
    let role = store.rename(&card(), "Bella").expect("应产生旧键");
    let before = store.snapshot("alpha").expect("应取基线快照");
    let mut blocker = rusqlite::Connection::open(&path).expect("应打开外部 SQLite 边界");
    let lock = blocker
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .expect("应取得写锁");
    assert!(
        matches!(
            store.delete_after_card_removed(&card()),
            Err(StorageError::Sqlite(_))
        ),
        "有界写锁等待失败必须返回存储错误"
    );
    lock.rollback().expect("应释放外部写锁");
    assert_eq!(
        store.snapshot("alpha").expect("应读失败后状态"),
        before,
        "数据库删除失败不能释放任何占用或推进版本"
    );
    assert_eq!(
        store.resolve("alpha", "Alan").expect("应查旧主名"),
        WorkspaceCallResolution::Inactive(role.clone()),
        "失效主名仍应保留原归属"
    );
    assert_eq!(
        store.resolve("alpha", "贝拉").expect("应查当前别名"),
        WorkspaceCallResolution::Active(role),
        "当前别名仍应有效"
    );
}
