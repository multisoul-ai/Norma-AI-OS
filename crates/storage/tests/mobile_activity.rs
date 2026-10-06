use storage::{
    AppStore, MobileActivityRegistration, MobileActivityStore, MobilePermission, PushEnvironment,
};

/// 实时活动独立注册：版本冲突与撤权不能覆盖新 token 或恢复订阅。
///
/// 数据构造：隔离 AppDB；1 台只读手机；32 字节 token 编码为 64 位 hex；0→1→2 为注册版本。
/// 执行过程：首次登记活动 → 换 token → 旧版本更新 → 撤权后读取。
/// 预期结果：正断言新 token 保留；负断言旧更新与撤权后的读取不可成功。
#[test]
fn activity_registration_is_independent_and_revision_guarded() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppStore::new(dir.path().join("app.sqlite"));
    let access = app.mobile_access_store();
    let grant = access
        .register_all_projects("phone-a", "手机", &"a".repeat(64), MobilePermission::Read)
        .unwrap();
    let store = MobileActivityStore::new(app.path().to_owned());
    let mut registration = MobileActivityRegistration {
        desktop_workspace_id: "space-a".into(),
        generation: "generation-a".into(),
        environment: PushEnvironment::Sandbox,
        auto_start: true,
        push_to_start_token: Some("b".repeat(64)),
        activity_token: None,
        activity_id: None,
        cycle_id: None,
        dismissed: false,
    };
    let first = store
        .replace(&grant, 0, Some(&registration))
        .unwrap()
        .unwrap();
    assert_eq!(
        first.revision, 1,
        "独立实时活动注册应成功，无需普通通知注册"
    );
    registration.push_to_start_token = Some("c".repeat(64));
    let second = store
        .replace(&grant, 1, Some(&registration))
        .unwrap()
        .unwrap();
    assert_eq!(second.revision, 2, "更新 token 应推进版本");
    assert!(
        store
            .replace(&grant, 1, Some(&registration))
            .unwrap()
            .is_none(),
        "迟到更新必须被拒绝"
    );
    assert_eq!(
        store
            .get(&grant)
            .unwrap()
            .unwrap()
            .registration
            .unwrap()
            .push_to_start_token,
        Some("c".repeat(64)),
        "当前 token 必须保留"
    );
    access.revoke(&grant.device_id).unwrap();
    assert!(
        store.get(&grant).unwrap().is_none(),
        "撤权后不得继续获取活动状态"
    );
    assert!(
        store
            .replace(&grant, 2, Some(&registration))
            .unwrap()
            .is_none(),
        "旧授权不能恢复活动"
    );
}

/// 启动预约与 token 恢复：未知发送不能重启，迟到周期不能覆写，结束并空闲后允许新周期。
/// 数据构造：32字节 token→64位hex；首次预约100秒，发送水位160秒；cycle-a→cycle-b。
/// 执行过程：保存启动预约→重开store→拒绝旧cycle→登记当前token→结束且观察空闲→登记新cycle。
/// 预期结果：预约跨重启保留；旧cycle不变；换token清水位；只有空闲后的新周期可登记。
#[test]
fn activity_start_reservation_and_token_recovery_are_durable() {
    let dir = tempfile::tempdir().unwrap();
    let app = AppStore::new(dir.path().join("app.sqlite"));
    let grant = app
        .mobile_access_store()
        .register_all_projects("phone", "手机", &"a".repeat(64), MobilePermission::Read)
        .unwrap();
    let store = MobileActivityStore::new(app.path().to_owned());
    let mut registration = MobileActivityRegistration {
        desktop_workspace_id: "space".into(),
        generation: "generation".into(),
        environment: PushEnvironment::Sandbox,
        auto_start: true,
        push_to_start_token: Some("b".repeat(64)),
        activity_token: None,
        activity_id: None,
        cycle_id: None,
        dismissed: false,
    };
    let initial = store
        .replace(&grant, 0, Some(&registration))
        .unwrap()
        .unwrap();
    let mut journal = storage::ActivityJournal {
        cycle_id: Some("cycle-a".into()),
        started_at: Some(100),
        last_sent_at: 160,
        ..Default::default()
    };
    assert!(
        store
            .update_journal(&grant, initial.revision, &journal)
            .unwrap(),
        "HTTP前必须保存启动预约"
    );
    let reopened = MobileActivityStore::new(app.path().to_owned());
    assert_eq!(
        reopened.get(&grant).unwrap().unwrap().journal.started_at,
        Some(100),
        "重启不能丢失已尝试启动的周期"
    );
    registration.activity_token = Some("c".repeat(64));
    registration.activity_id = Some("activity-a".into());
    registration.cycle_id = Some("cycle-old".into());
    assert!(
        store.replace(&grant, 1, Some(&registration)).is_err(),
        "迟到token不能覆盖当前周期"
    );
    registration.cycle_id = Some("cycle-a".into());
    let ready = store
        .replace(&grant, 1, Some(&registration))
        .unwrap()
        .unwrap();
    assert_eq!(ready.journal.last_sent_at, 0, "新token必须立即修复过期状态");
    journal.ended_at = Some(200);
    journal.quiet_seen = false;
    assert!(
        store
            .update_journal(&grant, ready.revision, &journal)
            .unwrap(),
        "结束保存成功"
    );
    registration.cycle_id = Some("cycle-b".into());
    registration.activity_id = Some("activity-b".into());
    registration.activity_token = Some("d".repeat(64));
    assert!(
        store.replace(&grant, 2, Some(&registration)).is_err(),
        "八小时结束后连续忙碌不能自动循环"
    );
    journal.quiet_seen = true;
    assert!(
        store.update_journal(&grant, 2, &journal).unwrap(),
        "真正空闲允许下个工作周期"
    );
    let next = store
        .replace(&grant, 2, Some(&registration))
        .unwrap()
        .unwrap();
    assert_eq!(
        next.journal.cycle_id.as_deref(),
        Some("cycle-b"),
        "空闲后的新本地活动应绑定新周期"
    );
    assert!(
        next.journal.ended_at.is_none(),
        "新周期不能继承上次结束标记"
    );
    assert!(
        !store
            .invalidate(&grant, next.revision, "c".repeat(64).as_str())
            .unwrap(),
        "旧token失效不能清除新token"
    );
    assert_eq!(
        store
            .get(&grant)
            .unwrap()
            .unwrap()
            .registration
            .unwrap()
            .activity_token,
        Some("d".repeat(64)),
        "当前token必须保留"
    );
}
