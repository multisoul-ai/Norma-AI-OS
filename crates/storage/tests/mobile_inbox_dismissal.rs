use std::collections::HashSet;
use storage::{InboxDismissal, InboxItemKind, MobileCommandStore, NewMobileCommand};

const PROJECT: &str = "project-1";

fn command(device: &str, operation: &str) -> NewMobileCommand {
    NewMobileCommand {
        device_id: device.into(),
        operation_id: operation.into(),
        workspace_id: "desktop:workspace-1:area-1".into(),
        route_id: "route-1".into(),
        owner_id: "owner-1".into(),
        request_hash: "a".repeat(64),
    }
}

/// 场景：原操作重复以及另一设备重复放弃同一事项都成功，但保留首次决定。
/// 数据构造：手机a/op-1与手机b/op-2共享64位键；重复操作位于另一个workspace/area，第三操作共享设备a但有新operation。
/// 执行过程：首次放弃 → 原操作重放 → 手机b放弃/重放 → 手机a新操作放弃。
/// 预期结果：全部返回同一记录；各自回执成功，集合只有1键，不改首次操作。
#[test]
fn repeated_dismissals_preserve_first_record_and_succeed_each_operation() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let store = MobileCommandStore::new(path.clone());
    let source = command("phone-a", "op-1");
    let key = "b".repeat(64);
    store.reserve(&source).expect("预留首次操作");
    let first = store
        .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &key)
        .expect("首次放弃");
    let store = MobileCommandStore::new(path);
    assert_eq!(
        store
            .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &key)
            .expect("原操作重复"),
        first,
        "原操作重放保留首次记录"
    );
    for mut source in [command("phone-b", "op-2"), command("phone-a", "op-3")] {
        source.workspace_id = "desktop:workspace-2:area-2".into();
        store.reserve(&source).expect("预留重复放弃操作");
        for _ in 0..2 {
            assert_eq!(
                store
                    .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &key)
                    .expect("重复放弃成功"),
                first,
                "另一操作及其重放返回首次记录"
            );
        }
        let receipt = store
            .get(
                &source.device_id,
                &source.operation_id,
                &source.workspace_id,
                &source.owner_id,
            )
            .expect("查询当前回执")
            .expect("当前操作存在");
        assert_eq!(receipt.state, "succeeded", "每个重复操作各自成功");
        assert_eq!(receipt.result, Some(first.result()), "重复操作关联首次结果");
        assert!(receipt.error.is_none(), "重复放弃不产生错误");
    }
    assert_eq!(
        store.inbox_dismissed_keys(PROJECT).expect("过滤集合"),
        HashSet::from([key.clone()]),
        "多次放弃只存1键"
    );
    assert!(
        store
            .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Review, &key)
            .is_err(),
        "原操作不能改变类型"
    );
    assert!(
        store
            .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &"c".repeat(64))
            .is_err(),
        "原操作不能绑定新事项"
    );
    assert_eq!(
        store.inbox_dismissed_keys(PROJECT).expect("拒绝后集合"),
        HashSet::from([key]),
        "被拒绝的新键不能隐藏"
    );
}

/// 场景：同项目跨工作空间共享放弃集合，不同项目与新事项保持隔离。
/// 数据构造：项目1在scope A/B分别放弃a/b键，项目2在A放弃a键，项目3为空。
/// 执行过程：逐个预留和放弃 → 按项目读取集合 → 校验无效project_id。
/// 预期结果：项目1有a/b且不按scope分裂；项目2只有a，项目3为空；无效项目报错。
#[test]
fn dismissed_key_sets_are_per_project_across_workspace_bindings() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let store = MobileCommandStore::new(dir.path().join("app.sqlite"));
    for (operation, workspace, project, kind, key) in [
        (
            "op-a",
            "desktop:workspace-1:area-1",
            PROJECT,
            InboxItemKind::Failed,
            "a".repeat(64),
        ),
        (
            "op-b",
            "desktop:workspace-2:area-2",
            PROJECT,
            InboxItemKind::Approval,
            "b".repeat(64),
        ),
        (
            "op-c",
            "desktop:workspace-1:area-1",
            "project-2",
            InboxItemKind::Review,
            "a".repeat(64),
        ),
    ] {
        let mut source = command("phone-a", operation);
        source.workspace_id = workspace.into();
        store.reserve(&source).expect("预留");
        store
            .dismiss_inbox_item(&source, project, kind, &key)
            .expect("放弃");
    }
    assert_eq!(
        store.inbox_dismissed_keys(PROJECT).expect("共享项目集合"),
        HashSet::from(["a".repeat(64), "b".repeat(64)]),
        "同项目在A/B作用域放弃的事项一起过滤"
    );
    assert_eq!(
        store
            .inbox_dismissed_keys("project-2")
            .expect("另一项目集合"),
        HashSet::from(["a".repeat(64)]),
        "另一项目只有自身记录"
    );
    assert!(
        store
            .inbox_dismissed_keys("project-3")
            .expect("空项目集合")
            .is_empty(),
        "空项目不继承同名事项键"
    );
    assert!(
        store
            .inbox_dismissed_keys("desktop:workspace-1:area-1")
            .is_err(),
        "scope不能冒充项目ID"
    );
}

/// 场景：没有预留、身份变化、无效键以及已拒绝的操作不能放弃事项。
/// 数据构造：有效源与64位键；逐一改变workspace/route/owner/hash，键取63/65位、非hex和大写。
/// 执行过程：未预留调用 → 预留 → 改身份/坏键 → 拒绝回执 → 再尝试放弃。
/// 预期结果：所有无效调用失败；过滤集合始终空；原回执不被改成成功。
#[test]
fn invalid_or_changed_commands_cannot_dismiss_items() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let store = MobileCommandStore::new(dir.path().join("app.sqlite"));
    let source = command("phone-a", "op-1");
    let key = "b".repeat(64);
    assert!(
        store
            .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &key)
            .is_err(),
        "未预留不能放弃"
    );
    store.reserve(&source).expect("预留");
    assert!(
        store
            .dismiss_inbox_item(
                &source,
                "desktop:workspace-1:area-1",
                InboxItemKind::Failed,
                &key
            )
            .is_err(),
        "活动scope不能冒充project_id"
    );
    for field in ["workspace", "route", "owner", "hash"] {
        let mut changed = source.clone();
        match field {
            "workspace" => changed.workspace_id = "desktop:workspace-2:area-1".into(),
            "route" => changed.route_id = "route-2".into(),
            "owner" => changed.owner_id = "owner-2".into(),
            _ => changed.request_hash = "c".repeat(64),
        }
        assert!(
            store
                .dismiss_inbox_item(&changed, PROJECT, InboxItemKind::Failed, &key)
                .is_err(),
            "预留身份{field}必须匹配"
        );
    }
    for bad in [
        "".into(),
        "a".repeat(63),
        "a".repeat(65),
        "g".repeat(64),
        "A".repeat(64),
    ] {
        assert!(
            store
                .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &bad)
                .is_err(),
            "坏键不能进入集合：{bad}"
        );
    }
    assert!(
        store
            .inbox_dismissed_keys(PROJECT)
            .expect("拒绝后集合")
            .is_empty(),
        "坏请求不能留下放弃记录"
    );
    assert_eq!(
        store
            .get(
                &source.device_id,
                &source.operation_id,
                &source.workspace_id,
                &source.owner_id
            )
            .expect("回执")
            .expect("原操作")
            .state,
        "accepted",
        "无效输入不改变预留状态"
    );
    store
        .finish(&source, Err("route_expired".into()))
        .expect("拒绝操作");
    assert!(
        store
            .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &key)
            .is_err(),
        "已拒绝的回执不可覆盖"
    );
    assert!(
        store
            .inbox_dismissed_keys(PROJECT)
            .expect("最终集合")
            .is_empty(),
        "已拒绝操作不隐藏事项"
    );
}

/// 场景：回执更新失败使首次放弃与重复操作整体回滚，不留下半次成功。
/// 数据构造：64位键；SQLite触发器仅拒绝当前操作的回执UPDATE。
/// 执行过程：预留 → 注入故障 → 放弃 → 查集合/回执 → 去掉故障 → 成功 → 另一设备重复故障。
/// 预期结果：首次失败无记录且回执accepted；重复失败保留首次记录，新回执仍accepted。
#[test]
fn receipt_failure_rolls_back_new_dismissal_and_preserves_existing_one() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let store = MobileCommandStore::new(path.clone());
    let key = "b".repeat(64);
    let source = command("phone-a", "op-1");
    store.reserve(&source).expect("预留");
    let connection = rusqlite::Connection::open(&path).expect("故障注入连接");
    connection.execute_batch("CREATE TRIGGER fail_dismissal_receipt BEFORE UPDATE ON mobile_commands BEGIN SELECT RAISE(ABORT,'test receipt failure'); END;").expect("注入回执故障");
    assert!(
        store
            .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &key)
            .is_err(),
        "回执失败时放弃必须失败"
    );
    assert!(
        store
            .inbox_dismissed_keys(PROJECT)
            .expect("回滚集合")
            .is_empty(),
        "回滚不能留下放弃记录"
    );
    assert_eq!(
        store
            .get(
                &source.device_id,
                &source.operation_id,
                &source.workspace_id,
                &source.owner_id
            )
            .expect("回执")
            .expect("原操作")
            .state,
        "accepted",
        "回滚保留预留状态"
    );
    connection
        .execute_batch("DROP TRIGGER fail_dismissal_receipt")
        .expect("撤销故障");
    let first = store
        .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &key)
        .expect("恢复后放弃");
    let second = command("phone-b", "op-2");
    store.reserve(&second).expect("预留第二操作");
    connection.execute_batch("CREATE TRIGGER fail_dismissal_receipt BEFORE UPDATE ON mobile_commands BEGIN SELECT RAISE(ABORT,'test receipt failure'); END;").expect("重复故障");
    assert!(
        store
            .dismiss_inbox_item(&second, PROJECT, InboxItemKind::Failed, &key)
            .is_err(),
        "另一操作回执失败不可伪造成功"
    );
    assert_eq!(
        store.inbox_dismissed_keys(PROJECT).expect("首次记录集合"),
        HashSet::from([key.clone()]),
        "失败重复操作不删除首次记录"
    );
    assert_eq!(
        store
            .get(
                &second.device_id,
                &second.operation_id,
                &second.workspace_id,
                &second.owner_id
            )
            .expect("第二回执")
            .expect("第二操作")
            .state,
        "accepted",
        "第二回执回滚为预留"
    );
    connection
        .execute_batch("DROP TRIGGER fail_dismissal_receipt")
        .expect("撤销重复故障");
    assert_eq!(
        store
            .dismiss_inbox_item(&second, PROJECT, InboxItemKind::Failed, &key)
            .expect("重复恢复"),
        first,
        "故障恢复后返回首次记录"
    );
}

/// 场景：公共记录的序列化、回执形状与验证边界保持明确且拒绝未知字段。
/// 数据构造：三种kind、有效scope、64位键与固定100秒时间；边界0与i64::MAX+1非法。
/// 执行过程：序列化/反序列化 → 比较固定结果 → 修改时间/键/身份 → 添加未知字段。
/// 预期结果：kind是snake_case；只有有效记录通过，回执仅含project_id/kind/item_key/时间。
#[test]
fn dismissal_model_has_validated_identity_and_stable_result() {
    let saved = InboxDismissal {
        project_id: PROJECT.into(),
        workspace_id: "desktop:workspace-1:area-1".into(),
        route_id: "route-1".into(),
        kind: InboxItemKind::Failed,
        item_key: "b".repeat(64),
        device_id: "phone-a".into(),
        operation_id: "op-1".into(),
        dismissed_at_unix: 100,
    };
    saved.validate().expect("完整记录有效");
    assert_eq!(
        saved.result(),
        serde_json::json!({"project_id":"project-1","kind":"failed","item_key":"b".repeat(64),"dismissed_at_unix":100}),
        "回执是稳定的事项身份和时间"
    );
    for (kind, name) in [
        (InboxItemKind::Approval, "approval"),
        (InboxItemKind::Failed, "failed"),
        (InboxItemKind::Review, "review"),
    ] {
        assert_eq!(
            serde_json::to_value(kind).expect("序列化kind"),
            serde_json::json!(name),
            "kind使用snake_case"
        );
    }
    let raw = serde_json::to_value(&saved).expect("序列化记录");
    assert_eq!(
        serde_json::from_value::<InboxDismissal>(raw.clone()).expect("反序列化记录"),
        saved,
        "记录序列化可往返"
    );
    for timestamp in [0, i64::MAX as u64 + 1] {
        let mut bad = saved.clone();
        bad.dismissed_at_unix = timestamp;
        assert!(bad.validate().is_err(), "非法时间不通过验证");
    }
    for field in [
        "project_id",
        "workspace_id",
        "route_id",
        "device_id",
        "operation_id",
        "item_key",
    ] {
        let mut bad = raw.clone();
        bad[field] = serde_json::json!("");
        let bad: InboxDismissal = serde_json::from_value(bad).expect("空字段仍能解码");
        assert!(bad.validate().is_err(), "空{field}不能成为保存记录");
    }
    let mut unknown = raw;
    unknown["unexpected"] = serde_json::json!(true);
    assert!(
        serde_json::from_value::<InboxDismissal>(unknown).is_err(),
        "未知字段不能被静默忽略"
    );
}

/// 场景：放弃仅持久化事项键，与原操作的成功回执原子提交，重开仍可过滤。
/// 数据构造：desktop作用域、手机a/op-1、64位小写hex键；初始过滤集合为空。
/// 执行过程：首次读取触发迁移 → 预留 → 放弃 → 重开读取记录集合与回执。
/// 预期结果：保存完整身份与非零时间；集合只含目标键；回执成功且无错误。
#[test]
fn dismissal_persists_with_success_receipt_and_migrates_on_read() {
    let dir = tempfile::tempdir().expect("隔离数据库");
    let path = dir.path().join("app.sqlite");
    let store = MobileCommandStore::new(path.clone());
    let source = command("phone-a", "op-1");
    let key = "b".repeat(64);
    assert!(
        store
            .inbox_dismissed_keys(PROJECT)
            .expect("首次读取迁移")
            .is_empty(),
        "迁移不能伪造放弃记录"
    );
    store.reserve(&source).expect("预留操作");
    let saved = store
        .dismiss_inbox_item(&source, PROJECT, InboxItemKind::Failed, &key)
        .expect("放弃事项");
    assert_eq!(saved.project_id, PROJECT, "记录跨工作空间共享的项目身份");
    assert_eq!(saved.workspace_id, source.workspace_id, "记录活动作用域");
    assert_eq!(saved.route_id, source.route_id, "记录路由");
    assert_eq!(saved.kind, InboxItemKind::Failed, "记录失败类型");
    assert_eq!(saved.item_key, key, "记录准确事项键");
    assert_eq!(saved.device_id, source.device_id, "记录首次设备");
    assert_eq!(saved.operation_id, source.operation_id, "记录首次操作");
    assert!(saved.dismissed_at_unix > 0, "记录真实放弃时间");
    saved.validate().expect("保存的契约必须有效");
    let reopened = MobileCommandStore::new(path);
    assert_eq!(
        reopened.inbox_dismissed_keys(PROJECT).expect("重开读集合"),
        HashSet::from([key]),
        "集合只含已放弃键"
    );
    let receipt = reopened
        .get(
            &source.device_id,
            &source.operation_id,
            &source.workspace_id,
            "owner-2",
        )
        .expect("读回执")
        .expect("回执存在");
    assert_eq!(receipt.state, "succeeded", "放弃和成功回执一起提交");
    assert_eq!(receipt.result, Some(saved.result()), "回执关联准确放弃记录");
    assert!(receipt.error.is_none(), "成功回执无错误");
}
