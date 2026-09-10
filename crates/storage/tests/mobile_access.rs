use storage::{MobileAccessStore, MobilePermission, MobileProjectAccess};

fn scope(project: &str, access: MobilePermission) -> MobileProjectAccess {
    MobileProjectAccess {
        workspace_id: project.into(),
        access,
    }
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

/// 手机服务的 Keychain account 跟随 App 数据库，不随 IP、窗口或重新打开变化。
///
/// 数据构造：两个临时 app.sqlite，分别代表独立安装；只生成非秘密的 Keychain 定位名。
/// 执行过程：A 创建账号 → 重新打开 A → B 创建账号 → 破坏 A 中的账号记录 → 再次读取。
/// 预期结果：A 账号稳定、B 不相同；损坏值明确失败，不生成替代值掩盖旧机器身份丢失。
#[test]
fn keychain_account_is_stable_and_isolated_per_installation() {
    let dir = tempfile::tempdir().expect("临时安装");
    let a = MobileAccessStore::new(dir.path().join("a.sqlite"));
    let account = a.keychain_account().expect("创建账号");
    assert_eq!(
        MobileAccessStore::new(dir.path().join("a.sqlite"))
            .keychain_account()
            .expect("重开"),
        account,
        "账号必须跨重启保持稳定"
    );
    assert_ne!(
        MobileAccessStore::new(dir.path().join("b.sqlite"))
            .keychain_account()
            .expect("B"),
        account,
        "独立安装不能共享 Keychain 账号"
    );
    let db = rusqlite::Connection::open(dir.path().join("a.sqlite")).expect("测试数据库");
    db.execute(
        "UPDATE settings SET value='bad/account' WHERE key='mobile.keychain_account'",
        [],
    )
    .expect("仅破坏测试值");
    assert!(a.keychain_account().is_err(), "损坏账号不得被覆盖重建");
}
