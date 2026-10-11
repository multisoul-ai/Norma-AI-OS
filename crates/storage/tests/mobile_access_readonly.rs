use storage::MobileAccessStore;

/// 查询授权的readonly入口不得创建缺失AppDB，也不得迁移空SQLite文件。
/// 数据构造：缺失文件路径和一个已有但无schema的SQLite；合法格式的测试credential。
/// 执行过程：两个路径各调用authenticate_readonly，检查文件/表。
/// 预期结果：缺grant返回None，缺文件不创建，已有文件user_version/table数仍为0。
#[test]
fn readonly_mobile_authorization_does_not_initialize_missing_databases() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("missing.sqlite");
    let token = "a".repeat(64);
    assert_eq!(
        MobileAccessStore::new(path.clone())
            .authenticate_readonly("phone-a", &token)
            .expect("缺授权应返回空"),
        None,
        "缺文件没有可用grant"
    );
    assert!(!path.exists(), "readonly不能创建AppDB");
    let connection = rusqlite::Connection::open(&path).expect("应构造无schema已有文件");
    assert_eq!(
        MobileAccessStore::new(path)
            .authenticate_readonly("phone-a", &token)
            .expect("无schema返回空"),
        None,
        "不能自动迁移并制造grant"
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .expect("检查table"),
        0,
        "只读授权不能创建任何表"
    );
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, u64>(0))
            .expect("检查schema版本"),
        0,
        "只读授权不能改schema版本"
    );
}
