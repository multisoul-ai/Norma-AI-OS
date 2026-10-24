use storage::{DatabasePaths, NodeSession, NodeSessionKind, ProjectStore};

/// 场景：新项目的数据库只创建在全局数据目录，重开仍使用同一存档。
/// 数据构造：一个空项目、独立 data 目录；初始化次数为首次(1)+重开(1)=2。
/// 执行过程：通过正式初始化入口打开两次，检查存档位置及项目目录内容。
/// 预期结果：全局 projects 目录有可重用数据库，项目目录保持空且不存在 .multisoul。
#[test]
fn opening_project_keeps_database_outside_the_repository() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    let data = temp.path().join("data");
    std::fs::create_dir(&root).expect("应创建空项目");
    let paths = DatabasePaths::isolated_test(&data);

    let first =
        ProjectStore::initialize_for_database_paths(&paths, &root).expect("应初始化全局项目数据库");
    let reopened =
        ProjectStore::initialize_for_database_paths(&paths, &root).expect("应重开同一项目数据库");

    assert!(
        first.path().starts_with(data.join("projects")),
        "项目数据库必须位于全局 projects 目录，实际为 {:?}",
        first.path()
    );
    assert!(first.path().is_file(), "全局数据库必须实际创建");
    assert_eq!(first.path(), reopened.path(), "重开项目必须复用原存档");
    assert_eq!(
        std::fs::read_dir(&root).expect("应读取项目目录").count(),
        0,
        "打开项目不得生成任何本地数据库或状态目录"
    );
    assert!(!root.join(".multisoul").exists(), "不得创建旧项目存储目录");
}

/// 场景：首次打开旧项目时迁移全部已提交数据，包括仍在 WAL 中的会话。
/// 数据构造：旧 Test 库中一个 Agent 会话，WAL 连接保持打开；新全局库初始不存在。
/// 执行过程：初始化全局项目库，公开接口重读会话；更新新库后再次初始化。
/// 预期结果：WAL 中的会话可恢复；旧库保留且不被改写；重开不以旧值覆盖新值。
#[test]
fn legacy_database_is_imported_once_including_committed_wal_data() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let legacy = ProjectStore::new(root.join(".multisoul/project.test.sqlite"));
    legacy.migrate().expect("应构造旧项目数据库");
    let keeper = rusqlite::Connection::open(legacy.path()).expect("应打开 WAL 连接");
    keeper
        .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
        .expect("应保持已提交数据在 WAL 中");
    let mut session = NodeSession {
        node_id: 7,
        kind: NodeSessionKind::Agent,
        cwd: Some(root.to_string_lossy().into_owned()),
        session_id: Some("legacy-session".into()),
    };
    keeper.execute(
        "INSERT INTO node_sessions (node_id, kind, cwd, session_id, updated_at_unix) VALUES (?1, 'agent', ?2, ?3, 1)",
        rusqlite::params![session.node_id, session.cwd, session.session_id],
    ).expect("应在保持打开的连接中提交旧会话，避免夹具关闭写连接时自动 checkpoint");
    assert!(
        legacy
            .path()
            .with_file_name("project.test.sqlite-wal")
            .metadata()
            .expect("夹具必须实际保留 WAL 文件")
            .len()
            > 0,
        "迁移测试必须覆盖非空 WAL，不能退化为普通文件复制"
    );
    let original = std::fs::read(legacy.path()).expect("应读取旧库原始字节");

    let migrated =
        ProjectStore::initialize_for_database_paths(&paths, &root).expect("应迁移旧项目数据库");
    assert_ne!(migrated.path(), legacy.path(), "迁移后的库必须位于全局目录");
    assert_eq!(
        migrated
            .node_session_store()
            .load_sessions()
            .expect("应重读迁移会话"),
        vec![session.clone()],
        "应完整恢复已提交的 WAL 会话"
    );
    assert_eq!(
        std::fs::read(legacy.path()).expect("旧库应保留"),
        original,
        "迁移不得修改旧库"
    );

    session.session_id = Some("new-session".into());
    migrated
        .node_session_store()
        .save_session(&session)
        .expect("应更新全局会话");
    let reopened =
        ProjectStore::initialize_for_database_paths(&paths, &root).expect("应重开已迁移项目");
    assert_eq!(
        reopened
            .node_session_store()
            .load_sessions()
            .expect("应读取新会话"),
        vec![session],
        "重复初始化不得覆盖全局库的新数据"
    );
    drop(keeper);
}

/// 场景：同名项目、路径别名和正式/开发环境不会误用彼此的项目数据。
/// 数据构造：两个名为 project 的目录，各保存 node_id=7；甲还有一个符号链接别名。
/// 执行过程：同一全局目录初始化甲乙，另用 Production 目录初始化甲，再通过别名打开甲。
/// 预期结果：甲的别名复用会话，乙与 Production 不包含甲的会话；所有项目目录保持空。
#[cfg(unix)]
#[test]
fn project_identity_and_database_mode_remain_isolated() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root_a = temp.path().join("alpha/project");
    let root_b = temp.path().join("beta/project");
    std::fs::create_dir_all(&root_a).expect("应创建甲项目");
    std::fs::create_dir_all(&root_b).expect("应创建乙项目");
    let alias = temp.path().join("alias");
    std::os::unix::fs::symlink(&root_a, &alias).expect("应创建路径别名");
    let test = DatabasePaths::local_test_for_home(temp.path());
    let production = DatabasePaths::production_for_home(temp.path());
    let alpha = ProjectStore::initialize_for_database_paths(&test, &root_a).expect("应初始化甲");
    let beta = ProjectStore::initialize_for_database_paths(&test, &root_b).expect("应初始化乙");
    let prod =
        ProjectStore::initialize_for_database_paths(&production, &root_a).expect("应初始化正式库");
    let session = NodeSession {
        node_id: 7,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("alpha".into()),
    };
    alpha
        .node_session_store()
        .save_session(&session)
        .expect("应保存甲会话");
    let aliased =
        ProjectStore::initialize_for_database_paths(&test, &alias).expect("应通过别名打开甲");

    assert_eq!(
        alpha.path(),
        aliased.path(),
        "同一真实目录不得因符号链接分叉存档"
    );
    assert_eq!(
        aliased
            .node_session_store()
            .load_sessions()
            .expect("应读取甲会话"),
        vec![session],
        "别名应恢复甲的会话"
    );
    assert_ne!(alpha.path(), beta.path(), "同名项目必须使用不同数据库");
    assert_ne!(alpha.path(), prod.path(), "Test 与 Production 必须物理隔离");
    assert!(
        beta.node_session_store()
            .load_sessions()
            .expect("应读取乙")
            .is_empty(),
        "乙不得混入甲的数据"
    );
    assert!(
        prod.node_session_store()
            .load_sessions()
            .expect("应读取正式库")
            .is_empty(),
        "正式库不得混入 Test 数据"
    );
    assert!(
        !root_a.join(".multisoul").exists() && !root_b.join(".multisoul").exists(),
        "隔离不能依赖在项目目录创建数据库"
    );
}

/// 场景：损坏的旧库迁移失败时不发布空库，修复来源后仍可重试。
/// 数据构造：旧 Test 文件含非 SQLite 内容，新全局库不存在。
/// 执行过程：初始化失败后检查源文件与目标，删除损坏夹具并以有效旧会话替换，再次初始化。
/// 预期结果：错误包含全局目标路径；失败后目标不存在、源文件保留；重试可恢复旧会话。
#[test]
fn failed_import_does_not_publish_an_empty_database_and_can_retry() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir_all(root.join(".multisoul")).expect("应创建旧项目目录");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let legacy = ProjectStore::new(root.join(".multisoul/project.test.sqlite"));
    std::fs::write(legacy.path(), b"broken database").expect("应构造损坏旧库");
    let target = paths.project_db_path(&root.canonicalize().expect("项目应存在"));

    let error = ProjectStore::initialize_for_database_paths(&paths, &root)
        .expect_err("损坏旧库必须明确报错");
    assert!(
        error
            .to_string()
            .contains(target.to_string_lossy().as_ref()),
        "错误必须指向全局目标路径"
    );
    assert!(
        !target.exists(),
        "失败不能留下空的正式目标，否则下次会跳过迁移"
    );
    assert_eq!(
        std::fs::read(legacy.path()).expect("旧库必须保留"),
        b"broken database",
        "迁移失败不得破坏来源"
    );
    std::fs::remove_file(legacy.path()).expect("仅删除测试构造的损坏文件");
    legacy.migrate().expect("应构造有效旧库");
    let session = NodeSession {
        node_id: 7,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("recovered".into()),
    };
    legacy
        .node_session_store()
        .save_session(&session)
        .expect("应保存旧会话");
    let retried =
        ProjectStore::initialize_for_database_paths(&paths, &root).expect("修复来源后应能重试");
    assert_eq!(
        retried
            .node_session_store()
            .load_sessions()
            .expect("应恢复会话"),
        vec![session],
        "重试必须导入完整旧数据"
    );
}

/// 场景：调用者先构造子 store 查询，也必须在创建全局库前导入旧数据。
/// 数据构造：旧库有一个 Agent 会话；新全局库不存在。
/// 执行过程：for_database_paths → 子 store 读画布 → 正式初始化 → 读取会话。
/// 预期结果：构造入口不能抢先创建空库导致后续永久跳过旧数据，旧会话必须保留。
#[test]
fn constructing_a_project_store_cannot_bypass_legacy_import() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let legacy = ProjectStore::new(root.join(".multisoul/project.test.sqlite"));
    legacy.migrate().expect("应构造旧库");
    let session = NodeSession {
        node_id: 7,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("legacy-session".into()),
    };
    legacy
        .node_session_store()
        .save_session(&session)
        .expect("应保存旧会话");

    ProjectStore::for_database_paths(&paths, &root)
        .expect("应构造存储")
        .canvas_store()
        .load_snapshot()
        .expect("首次子 store 查询应成功");
    let initialized =
        ProjectStore::initialize_for_database_paths(&paths, &root).expect("正式初始化应成功");
    assert_eq!(
        initialized
            .node_session_store()
            .load_sessions()
            .expect("应恢复旧会话"),
        vec![session],
        "先查询子 store 不得使旧数据迁移被空库遮蔽"
    );
    assert!(legacy.path().is_file(), "原项目库应保留备份");
}

/// 场景：Test 首次打开只有 Production 旧库的项目，不能导入正式数据。
/// 数据构造：Production 旧库有一个会话，Test 旧库不存在；两个全局目录均不存在。
/// 执行过程：先初始化 Test 再初始化 Production，分别从公开接口读取会话。
/// 预期结果：Test 保持空且不创建项目内 Test 库；Production 可独立导入旧会话。
#[test]
fn test_mode_never_imports_the_production_legacy_database() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目");
    let legacy = ProjectStore::new(root.join(".multisoul/project.sqlite"));
    legacy.migrate().expect("应构造 Production 旧库");
    let session = NodeSession {
        node_id: 7,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("production-only".into()),
    };
    legacy
        .node_session_store()
        .save_session(&session)
        .expect("应保存正式会话");
    let test = ProjectStore::initialize_for_database_paths(
        &DatabasePaths::local_test_for_home(temp.path()),
        &root,
    )
    .expect("应初始化 Test");
    let production = ProjectStore::initialize_for_database_paths(
        &DatabasePaths::production_for_home(temp.path()),
        &root,
    )
    .expect("应迁移 Production");

    assert!(
        test.node_session_store()
            .load_sessions()
            .expect("应读取 Test")
            .is_empty(),
        "Test 不得导入 Production 旧会话"
    );
    assert_eq!(
        production
            .node_session_store()
            .load_sessions()
            .expect("应读取正式会话"),
        vec![session],
        "Production 必须独立导入自己的旧数据"
    );
    assert!(
        !root.join(".multisoul/project.test.sqlite").exists(),
        "不能在项目目录创建 Test 库"
    );
}

/// 场景：两个后台入口同时首次迁移同一旧项目，均复用同一份完整的全局库。
/// 数据构造：旧库含一个会话；并发入口数为窗口(1)+后台历史查询(1)=2。
/// 执行过程：栅栏同时放行两个初始化线程，再从两者返回的 store 读取会话。
/// 预期结果：目标路径相同，两个读者都看到完整会话，旧库仍存在。
#[test]
fn concurrent_initialization_publishes_one_complete_database() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let root = temp.path().join("project");
    std::fs::create_dir(&root).expect("应创建项目");
    let paths = DatabasePaths::isolated_test(&temp.path().join("data"));
    let legacy = ProjectStore::new(root.join(".multisoul/project.test.sqlite"));
    legacy.migrate().expect("应构造旧库");
    let session = NodeSession {
        node_id: 7,
        kind: NodeSessionKind::Agent,
        cwd: None,
        session_id: Some("shared".into()),
    };
    legacy
        .node_session_store()
        .save_session(&session)
        .expect("应保存旧会话");
    let barrier = std::sync::Barrier::new(2);
    let stores = std::thread::scope(|scope| {
        let workers = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    ProjectStore::initialize_for_database_paths(&paths, &root)
                        .expect("并发初始化应成功")
                })
            })
            .collect::<Vec<_>>();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("线程不应崩溃"))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        stores[0].path(),
        stores[1].path(),
        "并发入口必须复用同一目标"
    );
    for store in stores {
        assert_eq!(
            store
                .node_session_store()
                .load_sessions()
                .expect("应读取会话"),
            vec![session.clone()],
            "每个入口都必须恢复完整会话"
        );
    }
    assert!(legacy.path().is_file(), "并发迁移也必须保留旧库");
}
