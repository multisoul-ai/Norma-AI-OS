use storage::{MobileAccessStore, MobilePermission, MobileProjectAccess, WorkspaceIdentityStore};

fn scope(project: &str, access: MobilePermission) -> MobileProjectAccess {
    MobileProjectAccess {
        workspace_id: project.into(),
        access,
    }
}

/// 旧数据库升级后仍保持逐项目授权，且新增全项目列不会反向扩权。
///
/// 数据构造（含关键数值的推导过程）：
///   旧版记录 1 条，只读项目 A 1 个；旧表 8 列，不含 all_projects_access。
///   升级后再注册项目 B 1 个，总注册项目数 = 1 + 1 = 2。
///
/// 执行过程：
///   1. 直接建立旧版 mobile_devices 表和手机授权记录。
///   2. MobileAccessStore::list 触发迁移，读取旧授权。
///   3. 注册项目 B 后再次读取旧授权。
///
/// 预期结果：旧设备只能读 A，不能控制 A，也不能读新增 B；模式为逐项目。
#[test]
fn legacy_mobile_grant_migrates_without_all_project_access() {
    let dir = tempfile::tempdir().expect("隔离旧版授权数据库");
    let path = dir.path().join("app.sqlite");
    let db = rusqlite::Connection::open(&path).expect("创建旧版数据库");
    db.execute_batch(
        "CREATE TABLE mobile_devices (
            device_id TEXT PRIMARY KEY NOT NULL,device_name TEXT NOT NULL,token_hash TEXT NOT NULL,
            permissions_json TEXT NOT NULL,revision INTEGER NOT NULL CHECK(revision>0),
            revoked INTEGER NOT NULL CHECK(revoked IN (0,1)),created_at_unix INTEGER NOT NULL,updated_at_unix INTEGER NOT NULL
         );
         INSERT INTO mobile_devices VALUES
         ('old-phone','旧 iPhone','hash','[{\"workspace_id\":\"project-a\",\"access\":\"read\"}]',1,0,1,1);",
    ).expect("写入旧版逐项目授权");
    drop(db);

    let store = MobileAccessStore::new(path.clone());
    let before = store.list().expect("迁移并读取旧授权");
    assert_eq!(before.len(), 1, "旧设备记录必须保留一条");
    assert_eq!(
        before[0].all_projects_access, None,
        "旧记录不能自动获得全项目模式"
    );
    assert!(
        before[0].allows("project-a", false),
        "原项目 A 只读授权必须保留"
    );
    assert!(
        !before[0].allows("project-a", true),
        "原项目 A 不应被升级为控制权限"
    );

    let project_b = dir.path().join("project-b");
    std::fs::create_dir(&project_b).expect("创建新项目 B 目录");
    let b = WorkspaceIdentityStore::new(path)
        .resolve_or_create(&project_b, "项目 B")
        .expect("注册新项目 B");
    let after = store.list().expect("升级后重新读取旧授权");
    assert!(
        !after[0].allows(&b.workspace_id, false),
        "旧逐项目授权不能覆盖新项目 B"
    );
    assert_eq!(after[0].projects.len(), 1, "旧授权项目清单必须仍只有 A");
}

/// 用户在 Mac 明确授权本机所有当前及未来项目后，新项目即时可访问；旧配对仍只认原项目。
///
/// 数据构造（含关键数值的推导过程）：
///   2 个真实注册目录 = 首次授权前 1 个 + 授权后新增 1 个；2 台手机分别为全项目与旧逐项目模式。
///   每个 64 字符 hex token = 32 字节设备凭据；初始 revision 1，改回逐项目后为 2。
///
/// 执行过程：
///   1. 注册项目 A，并由 Mac 为新手机明确授予全项目控制；旧手机只授予 A。
///   2. 注册项目 B 并重新鉴权 → 读取动态授权清单；尝试任意未注册项目。
///   3. 把新手机改回逐项目只读 → 重新鉴权并撤销。
///
/// 预期结果：新手机可控制 A/B 但不能控制未注册 ID；旧手机不能看到 B；降权、撤销后均不能沿用旧权限。
#[test]
fn explicit_all_project_grant_includes_future_registered_projects_only() {
    let dir = tempfile::tempdir().expect("隔离 Mac 授权数据库");
    let path = dir.path().join("app.sqlite");
    let identities = WorkspaceIdentityStore::new(path.clone());
    let project_a = dir.path().join("project-a");
    std::fs::create_dir(&project_a).expect("创建项目 A 目录");
    let a = identities
        .resolve_or_create(&project_a, "项目 A")
        .expect("注册项目 A");
    let store = MobileAccessStore::new(path);
    let full_token = "a".repeat(64);
    let legacy_token = "b".repeat(64);
    let full = store
        .register_all_projects(
            "phone-full",
            "新 iPhone",
            &full_token,
            MobilePermission::Control,
        )
        .expect("Mac 明确授权全项目控制");
    let legacy = store
        .register(
            "phone-legacy",
            "旧 iPhone",
            &legacy_token,
            &[scope(&a.workspace_id, MobilePermission::Read)],
        )
        .expect("旧设备保持逐项目授权");
    assert_eq!(
        full.all_projects_access,
        Some(MobilePermission::Control),
        "新授权必须明确记录全项目控制模式"
    );
    assert_eq!(legacy.all_projects_access, None, "旧逐项目授权不能静默升级");
    assert!(
        full.allows(&a.workspace_id, true),
        "全项目授权应控制已注册项目 A"
    );
    assert!(
        !full.allows("not-registered", true),
        "全项目模式不能授权任意未注册字符串"
    );

    let project_b = dir.path().join("project-b");
    std::fs::create_dir(&project_b).expect("创建项目 B 目录");
    let b = identities
        .resolve_or_create(&project_b, "项目 B")
        .expect("授权后注册项目 B");
    let current = store
        .authenticate("phone-full", &full_token)
        .expect("重新鉴权")
        .expect("全项目手机有效");
    let old = store
        .authenticate("phone-legacy", &legacy_token)
        .expect("旧设备鉴权")
        .expect("旧设备有效");
    assert!(
        current.allows(&b.workspace_id, true),
        "全项目授权应即时覆盖随后注册的项目 B"
    );
    assert_eq!(
        current.projects.len(),
        2,
        "当前目录只有 A+B 两项，手机授权摘要也只能有两项"
    );
    assert!(
        !old.allows(&b.workspace_id, false),
        "旧逐项目手机不能自动读取新项目 B"
    );

    let downgraded = store
        .replace_permissions(
            "phone-full",
            1,
            &[scope(&a.workspace_id, MobilePermission::Read)],
        )
        .expect("Mac 明确改回逐项目只读");
    assert_eq!(
        downgraded.revision, 2,
        "授权模式变化必须增加修订号，使旧排队命令失效"
    );
    assert_eq!(
        downgraded.all_projects_access, None,
        "改回逐项目后不能残留全项目模式"
    );
    assert!(
        downgraded.allows(&a.workspace_id, false),
        "保留的项目 A 仍可读取"
    );
    assert!(
        !downgraded.allows(&a.workspace_id, true),
        "降权后不能继续控制 A"
    );
    assert!(
        !downgraded.allows(&b.workspace_id, false),
        "改回逐项目后 B 不可见"
    );
    store.revoke("phone-full").expect("撤销新手机");
    assert!(
        store
            .authenticate("phone-full", &full_token)
            .expect("撤销后鉴权")
            .is_none(),
        "撤销后不能恢复旧全项目权限"
    );
}

/// 读取手机授权不能写数据库，也不能等待无关写事务释放写锁。
///
/// 数据构造：一个只读设备；64个hex字符=32字节测试token；
///   32次鉴权对应2MiB对话按64KiB分片：2×1024/64=32，每片都需重新核验权限。
/// 执行过程：
///   1. 注册设备，另一连接记录data_version，再连续鉴权32次并记录耗时。
///   2. 确认鉴权没有提交写入；持有BEGIN IMMEDIATE写锁时再鉴权。
///   3. 释放写锁并撤销设备，重新鉴权。
///
/// 预期结果：只读权限存在、控制权限不存在；data_version不变；
///   无关写锁不阻塞鉴权，撤销后不能继续使用旧权限。
#[test]
fn authentication_does_not_write_or_wait_for_a_writer() {
    let dir = tempfile::tempdir().expect("隔离数据库目录");
    let path = dir.path().join("app.sqlite");
    let store = MobileAccessStore::new(path.clone());
    let token = "a".repeat(64);
    store
        .register(
            "phone-a",
            "iPhone",
            &token,
            &[scope("project-a", MobilePermission::Read)],
        )
        .expect("注册只读设备");
    let observer = rusqlite::Connection::open(path).expect("独立观察连接");
    let version = || {
        observer
            .query_row("PRAGMA data_version", [], |row| row.get::<_, i64>(0))
            .expect("读取提交版本")
    };
    let before = version();
    let started = std::time::Instant::now();
    for _ in 0..32 {
        std::hint::black_box(store.authenticate("phone-a", &token).expect("逐片重新鉴权"));
    }
    eprintln!(
        "MOBILE_AUTH_32_READS_MS={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
    let after = version();
    observer
        .execute_batch("BEGIN IMMEDIATE")
        .expect("模拟无关写事务");
    let blocked_at = std::time::Instant::now();
    let result = store.authenticate("phone-a", &token);
    eprintln!(
        "MOBILE_AUTH_WITH_WRITER_MS={:.3} DB_CHANGED={}",
        blocked_at.elapsed().as_secs_f64() * 1000.0,
        after != before
    );
    let grant = result
        .expect("已有schema的鉴权不应申请写锁")
        .expect("已提交的设备授权应仍可读");
    assert_eq!(
        after, before,
        "只读鉴权不应重复写入schema版本或制造磁盘提交"
    );
    assert!(
        grant.allows("project-a", false),
        "写锁期间仍应读到已提交的只读权限"
    );
    assert!(
        !grant.allows("project-a", true),
        "只读设备不能因优化获得控制权限"
    );
    observer.execute_batch("ROLLBACK").expect("释放写锁");
    store.revoke("phone-a").expect("撤销设备");
    assert!(
        store
            .authenticate("phone-a", &token)
            .expect("重新查询权限")
            .is_none(),
        "优化不得缓存已撤销权限"
    );
}

/// 手机授权按设备和项目持久化，撤销一台设备不能影响另一台，也不能由迟到更新恢复旧权限。
///
/// 数据构造：两台手机 A/B，各一个 64 hex 字符测试 token（32 字节、256 bit 格式）；A 可读项目一并控制项目二。
/// 执行过程：Mac 注册 → 正确/错误身份认证 → A 从 revision 1 降权到 2 → 旧 revision 更新 → 撤销到 3 → 重开。
/// 预期结果：正确设备只获精确项目权限；旧版本与覆盖注册失败；撤销持久化、B 保留；数据库只保存 token 哈希。
#[test]
fn device_grants_are_durable_scoped_and_revocable() {
    let dir = tempfile::tempdir().expect("测试目录");
    let path = dir.path().join("app.sqlite");
    let store = MobileAccessStore::new(path.clone());
    let token_a = "a".repeat(64);
    let token_b = "b".repeat(64);
    let a = store
        .register(
            "phone-a",
            "Alan 的 iPhone",
            &token_a,
            &[
                scope("project-one", MobilePermission::Read),
                scope("project-two", MobilePermission::Control),
            ],
        )
        .expect("Mac 授权 A");
    store
        .register(
            "phone-b",
            "iPhone B",
            &token_b,
            &[scope("project-one", MobilePermission::Control)],
        )
        .expect("Mac 授权 B");
    assert_eq!(a.revision, 1, "首次授权从 revision 1 开始");
    let a = store
        .authenticate("phone-a", &token_a)
        .expect("读取")
        .expect("正确凭据");
    assert!(a.allows("project-one", false), "A 可以读项目一");
    assert!(!a.allows("project-one", true), "A 不可控制项目一");
    assert!(a.allows("project-two", true), "A 可以控制项目二");
    assert!(!a.allows("another-project", false), "未授权项目不可读取");
    assert!(
        store
            .authenticate("phone-b", &token_a)
            .expect("错设备")
            .is_none(),
        "A 的 token 不能冒充 B"
    );
    assert!(
        store
            .authenticate("phone-a", &token_b)
            .expect("错凭据")
            .is_none(),
        "B 的 token 不能冒充 A"
    );
    assert!(
        store
            .register("phone-a", "替换设备", &token_b, &[])
            .is_err(),
        "再次注册不能覆盖已存在设备"
    );
    let a = store
        .replace_permissions(
            "phone-a",
            1,
            &[scope("project-two", MobilePermission::Read)],
        )
        .expect("Mac 降权");
    assert_eq!(a.revision, 2, "权限修改必须换版本，供排队请求复核");
    assert!(a.allows("project-two", false), "降权后可读项目二");
    assert!(!a.allows("project-two", true), "不能保留旧控制权限");
    assert!(!a.allows("project-one", false), "已移除项目一不可读取");
    assert!(
        store
            .replace_permissions(
                "phone-a",
                1,
                &[scope("project-two", MobilePermission::Control)]
            )
            .is_err(),
        "迟到的 revision 1 更新不可提权"
    );
    let revoked = store.revoke("phone-a").expect("撤销");
    assert_eq!(revoked.revision, 3, "撤销也必须推进权限版本");
    assert!(revoked.revoked, "记录准确撤销状态");
    assert!(
        !revoked.allows("project-two", false),
        "旧授权对象的撤销记录也不允许读取"
    );
    assert_eq!(
        store.revoke("phone-a").expect("重复撤销").revision,
        3,
        "重复撤销保持幂等"
    );
    let reopened = MobileAccessStore::new(path.clone());
    assert!(
        reopened
            .authenticate("phone-a", &token_a)
            .expect("重新打开")
            .is_none(),
        "重启后不能复活已撤销授权"
    );
    assert!(
        reopened
            .authenticate("phone-b", &token_b)
            .expect("B")
            .expect("B 仍有效")
            .allows("project-one", true),
        "撤销 A 不能影响 B"
    );
    assert!(
        reopened
            .replace_permissions(
                "phone-a",
                3,
                &[scope("project-one", MobilePermission::Control)]
            )
            .is_err(),
        "撤销后不能以修改权限的方式复活 token"
    );
    assert_eq!(
        reopened.list().expect("设备列表").len(),
        2,
        "保留两台设备记录及撤销结果"
    );
    let db = rusqlite::Connection::open(&path).expect("只检查测试数据库");
    let hash: String = db
        .query_row(
            "SELECT token_hash FROM mobile_devices WHERE device_id='phone-a'",
            [],
            |r| r.get(0),
        )
        .expect("哈希字段");
    assert_ne!(hash, token_a, "数据库不得保存可直接使用的设备 token");
    assert!(
        !format!("{a:?}").contains(&token_a),
        "设备信息和调试输出不得携带 token"
    );
    assert!(
        MobileAccessStore::new(dir.path().join("other-mac.sqlite"))
            .authenticate("phone-b", &token_b)
            .expect("另一电脑")
            .is_none(),
        "不同电脑的同名设备不能自动授权"
    );
}

/// 设备名、凭据、项目范围和版本都在持久化边界校验，坏记录不能自动扩大权限。
///
/// 数据构造：63 hex 字符错误凭据、重复项目范围、一个正确注册后被测试改成未知 access 的记录。
/// 执行过程：依次注册非法输入 → 确认无记录 → 正确注册 → 注入损坏 JSON 范围 → 认证。
/// 预期结果：所有非法注册均失败且无部分写入；损坏授权返回错误，不能退化为全项目控制。
#[test]
fn invalid_or_corrupt_grants_fail_closed() {
    let dir = tempfile::tempdir().expect("测试目录");
    let path = dir.path().join("app.sqlite");
    let store = MobileAccessStore::new(path.clone());
    let token = "a".repeat(64);
    // 63 = 合法长度 64 - 1，避免测试仅靠空字符串偶然拒绝。
    assert!(
        store
            .register("phone-a", "iPhone", &"a".repeat(63), &[])
            .is_err(),
        "短 token 不能持久化"
    );
    assert!(
        store
            .register("phone-a", "iPhone\n伪造提示", &token, &[])
            .is_err(),
        "设备名不能注入控制字符"
    );
    assert!(
        store
            .register(
                "phone-a",
                "iPhone",
                &token,
                &[
                    scope("project-a", MobilePermission::Read),
                    scope("project-a", MobilePermission::Control)
                ]
            )
            .is_err(),
        "重复范围不能通过合并意外提权"
    );
    assert!(
        store.list().expect("列表").is_empty(),
        "非法输入不能留下部分设备记录"
    );
    store
        .register(
            "phone-a",
            "iPhone",
            &token,
            &[scope("project-a", MobilePermission::Read)],
        )
        .expect("有效设备");
    let db = rusqlite::Connection::open(path).expect("测试数据库");
    db.execute("UPDATE mobile_devices SET permissions_json='[{\"workspace_id\":\"project-a\",\"access\":\"root\"}]'",[]).expect("构造坏记录");
    assert!(
        store.authenticate("phone-a", &token).is_err(),
        "未知权限不能默认成为可控制"
    );
}
