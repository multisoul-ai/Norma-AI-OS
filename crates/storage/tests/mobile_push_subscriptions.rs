use storage::{
    AppStore, MobilePermission, MobileProjectAccess, MobilePushRegistration, MobilePushStore,
    PushEnvironment,
};

/// 手机提醒注册必须绑定当前授权，过期更新与迟到APNs失效响应不能覆盖新注册。
///
/// 数据构造：只读手机A和控制手机B；APNs token由32字节编码为64位hex；初始revision=0，
///   注册A=1，替换A=2，关闭A=3；相同毫秒内也依靠revision区分新旧注册。
/// 执行过程：空态 → A/B各自注册 → A换token → 旧更新/旧失效拒绝 → 关闭A → 撤销B。
/// 预期结果：只读也可收提醒；A更新不改变B；关提醒保留设备权限，撤销则清除B的push token。
#[test]
fn subscriptions_cannot_outlive_revocation_or_replace_newer_registration() {
    let dir = tempfile::tempdir().expect("隔离订阅数据库");
    let app = AppStore::new(dir.path().join("app.sqlite"));
    let access = app.mobile_access_store();
    let a = access
        .register(
            "phone-a",
            "A",
            &"a".repeat(64),
            &[MobileProjectAccess {
                workspace_id: "project-a".into(),
                access: MobilePermission::Read,
            }],
        )
        .expect("只读手机");
    let b = access
        .register(
            "phone-b",
            "B",
            &"b".repeat(64),
            &[MobileProjectAccess {
                workspace_id: "project-a".into(),
                access: MobilePermission::Control,
            }],
        )
        .expect("另一手机");
    let store = MobilePushStore::new(app.path().to_path_buf());
    let first = MobilePushRegistration {
        token: "c".repeat(64),
        environment: PushEnvironment::Sandbox,
    };
    let second = MobilePushRegistration {
        token: "d".repeat(64),
        environment: PushEnvironment::Production,
    };
    let empty = store.get(&a).expect("初始状态").expect("已授权");
    assert_eq!(empty.revision, 0, "没有订阅时版本应从零开始");
    assert!(empty.registration.is_none(), "配对不能自动启用通知");
    let registered_a = store
        .replace(&a, 0, Some(&first))
        .expect("A注册")
        .expect("首次更新成功");
    let registered_b = store
        .replace(&b, 0, Some(&first))
        .expect("B注册")
        .expect("独立更新");
    assert_eq!(registered_a.revision, 1, "只读手机同样可以启用提醒");
    assert!(
        registered_a.registered_at_ms.is_some(),
        "注册时间必须由Mac记录"
    );
    let updated = store
        .replace(&a, 1, Some(&second))
        .expect("A更新")
        .expect("当前版本可替换");
    assert_eq!(updated.revision, 2, "每次新注册必须有独立版本");
    assert!(
        store
            .replace(&a, 1, Some(&first))
            .expect("旧请求")
            .is_none(),
        "迟到旧token不能覆盖新token"
    );
    let registered_at = updated.registered_at_ms.expect("新注册时间");
    assert!(
        !store
            .invalidate("phone-a", 1, registered_at)
            .expect("旧APNs反馈"),
        "旧注册失效不能关闭新注册"
    );
    assert!(
        !store
            .invalidate("phone-a", 2, registered_at - 1)
            .expect("更早失效时间"),
        "失效发生在重新注册之前，应保留新注册"
    );
    assert_eq!(
        store.get(&a).expect("A现状").expect("A授权").registration,
        Some(second.clone()),
        "A必须保留新环境和token"
    );
    assert_eq!(
        store.get(&b).expect("B现状").expect("B授权"),
        registered_b,
        "A更新不能改变B的注册"
    );
    assert!(
        !format!("{updated:?}").contains(&second.token),
        "调试输出不得泄漏push token"
    );
    let off = store
        .replace(&a, 2, None)
        .expect("关闭A提醒")
        .expect("关闭成功");
    assert_eq!(off.revision, 3, "关闭也必须推进版本，阻止旧启用请求");
    assert!(off.registration.is_none(), "关闭后不得保留待投递token");
    assert!(
        access
            .authenticate("phone-a", &"a".repeat(64))
            .expect("A认证")
            .is_some(),
        "关闭提醒不应撤销手机控制权限"
    );
    assert!(
        store
            .invalidate(
                "phone-b",
                1,
                registered_b.registered_at_ms.expect("B注册时间")
            )
            .expect("当前token失效"),
        "当前注册的APNs失效必须停止投递"
    );
    let inactive_b = store.get(&b).expect("B失效状态").expect("B仍授权");
    assert_eq!(inactive_b.revision, 2, "注册1加一次失效应推进到2");
    assert!(
        inactive_b.registration.is_none(),
        "失效token必须从投递目标中移除"
    );
    assert_eq!(
        store
            .replace(&b, 2, Some(&first))
            .expect("B重新注册")
            .expect("允许重新注册")
            .revision,
        3,
        "重新注册从2推进到3"
    );
    access.revoke("phone-b").expect("撤销B");
    assert!(
        store.get(&b).expect("过期授权读取").is_none(),
        "旧授权快照不能读取已撤销订阅"
    );
    assert!(
        store
            .replace(&b, 1, Some(&second))
            .expect("过期授权写入")
            .is_none(),
        "撤销后不能重新启用提醒"
    );
    let db = rusqlite::Connection::open(app.path()).expect("核对落盘");
    let token: Option<String> = db
        .query_row(
            "SELECT token FROM mobile_push_subscriptions WHERE device_id='phone-b'",
            [],
            |row| row.get(0),
        )
        .expect("B订阅记录");
    assert!(token.is_none(), "撤销必须真正移除B的token，而非仅在UI隐藏");
    let invalid = MobilePushRegistration {
        token: "not-hex".into(),
        environment: PushEnvironment::Sandbox,
    };
    assert!(
        store.replace(&a, 3, Some(&invalid)).is_err(),
        "非法APNs token不能进入数据库"
    );
    assert!(
        store
            .get(&a)
            .expect("错误输入后状态")
            .expect("A仍授权")
            .registration
            .is_none(),
        "错误输入不能重新启用通知"
    );
}
