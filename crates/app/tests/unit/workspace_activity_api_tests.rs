use super::*;

/// 场景：统一收件箱是设备级只读操作，允许无需工作空间的请求。
/// 数据构造：pending请求及夹带input/limit/target的三个非法版本。
/// 执行过程：解析最小请求，再分别加入控制参数、截断参数与目标。
/// 预期结果：pending走只读分支且无默认空间；拒绝所有未声明字段。
#[test]
fn pending_request_is_workspace_less_and_cannot_truncate_or_control() {
    let request: Request =
        serde_json::from_value(json!({"kind":"pending"})).expect("完整Mac收件箱");
    assert!(
        matches!(request.operation, Operation::Pending),
        "pending必须进入只读投影"
    );
    assert!(
        request.desktop_workspace_id.is_empty(),
        "不要求选择桌面空间"
    );
    for field in ["input", "limit", "target"] {
        let mut value = json!({"kind":"pending"});
        value[field] = json!("unexpected");
        assert!(
            serde_json::from_value::<Request>(value).is_err(),
            "pending必须拒绝{field}"
        );
    }
}

/// 工作空间活动 RPC：结构化工具审批与只读汇总共用身份，未知字段不能变成新权限。
/// 数据构造：1 个区域/卡片/回合/审批，决策只 allow/deny；32字节token编码64位hex。
/// 执行过程：解析汇总与审批请求 → 注入终端输入 → 查询旧版缺少的kind。
/// 预期结果：合法分支可解析；额外控制参数与未知操作均拒绝。
#[test]
fn workspace_activity_request_is_typed_and_rejects_extra_controls() {
    let read = json!({"desktop_workspace_id":"space","kind":"summary"});
    let request: Request = serde_json::from_value(read.clone()).expect("汇总请求");
    assert!(
        matches!(request.operation, Operation::Summary),
        "summary必须走只读分支"
    );
    let write = json!({"desktop_workspace_id":"space","kind":"decide","operation_id":"op","decision":"allow","target":{"area_id":"area","node_id":1,"profile":"Codex","run_id":"run","session_id":"session","turn_id":"turn","request_id":"request"}});
    let request: Request = serde_json::from_value(write).expect("审批请求");
    assert!(
        matches!(request.operation, Operation::Decide { .. }),
        "只有明确工具审批可以修改"
    );
    let mut extra = read;
    extra["input"] = json!("unsafe control");
    assert!(
        serde_json::from_value::<Request>(extra).is_err(),
        "只读请求不得夹带终端控制参数"
    );
    assert!(
        serde_json::from_value::<Request>(json!({"desktop_workspace_id":"space","kind":"execute"}))
            .is_err(),
        "首版活动不能开放任意执行命令"
    );
}

/// 设备自动登记与总览无需选空间，具体决策仍必须定位原空间。
/// 数据构造：summary/subscription/set_subscription三种设备操作；一个缺空间的decide。
/// 执行过程：解析设备操作→读取空范围标识→解析缺范围决策→拒绝。
/// 预期结果：设备级操作可解析，决策不能借默认范围控制任意项目。
#[test]
fn workspace_activity_device_scope_is_automatic_but_never_authorizes_decisions() {
    let read: Request = serde_json::from_value(json!({"kind":"summary"})).expect("自动Mac总览");
    assert!(
        read.desktop_workspace_id.is_empty(),
        "自动总览不要求指定工作空间"
    );
    assert!(
        serde_json::from_value::<Request>(json!({"kind":"subscription"})).is_ok(),
        "配对后可直接读登记"
    );
    assert!(
        serde_json::from_value::<Request>(
            json!({"kind":"set_subscription","expected_revision":0,"registration":null})
        )
        .is_ok(),
        "设备级登记不要求工作空间目录"
    );
    assert!(
        serde_json::from_value::<Request>(
            json!({"kind":"receipt","area_id":"area","operation_id":"op"})
        )
        .is_err(),
        "原回执仍必须定位工作空间"
    );
}

/// 自动总览按项目过滤：只读授权可查一个项目但不能泄漏另一个区域，配置修改使捕获失效。
/// 数据构造：隔离AppDB，1空间/2非Git项目；仅授权A，随后改为全部只读。
/// 执行过程：A单独授权→汇总只返回A→全部只读成功→控制请求拒绝→删除B→旧范围不再匹配。
/// 预期结果：仅返回获授权项目；只读不授予决策；配置变化必须拦截迟到数据。
#[test]
fn workspace_activity_authorization_never_expands_project_permissions() {
    let dir = tempfile::tempdir().expect("隔离目录");
    let app = storage::AppStore::new(dir.path().join("app.sqlite"));
    let catalog = DesktopWorkspaceStore::new(app.path().to_owned());
    let space = catalog.create("测试空间").expect("新空间");
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    std::fs::create_dir(&a).expect("项目A");
    std::fs::create_dir(&b).expect("项目B");
    let area_a = catalog.add_project(&space.id, &a).expect("添加A");
    let area_b = catalog.add_project(&space.id, &b).expect("添加B");
    let identity = app
        .workspace_identity_store()
        .resolve_or_create(&area_a.root, "A")
        .expect("授权用项目身份");
    let access = app.mobile_access_store();
    let grant = access
        .register(
            "phone",
            "手机",
            &"a".repeat(64),
            &[storage::MobileProjectAccess {
                workspace_id: identity.workspace_id,
                access: storage::MobilePermission::Read,
            }],
        )
        .expect("仅授权A");
    let visible = areas(app.path(), &space.id, &grant, false).expect("项目A可读");
    assert_eq!(visible.len(), 1, "局部授权应只呈现一个项目");
    assert_eq!(visible[0].area.id, area_a.id, "获授权的A必须可见");
    assert!(
        !visible.iter().any(|a| a.area.id == area_b.id),
        "B不能泄漏到默认总览"
    );
    let grant = access
        .replace_with_all_projects("phone", grant.revision, storage::MobilePermission::Read)
        .expect("全部只读");
    let authorized = areas(app.path(), &space.id, &grant, false).expect("完整只读范围");
    assert_eq!(authorized.len(), 2, "完整读取应包含两个项目身份");
    assert!(
        matches!(
            areas(app.path(), &space.id, &grant, true),
            Err("unauthorized")
        ),
        "只读授权不能批准工具或验收成果"
    );
    let captured = authorized.into_iter().map(|a| a.area).collect::<Vec<_>>();
    assert!(
        catalog
            .mobile_scope_current(&space.id, &captured)
            .expect("当前目录"),
        "捕获前范围匹配"
    );
    catalog
        .remove_project_catalog(&space.id, &area_b.id)
        .expect("移除B");
    assert!(
        !catalog
            .mobile_scope_current(&space.id, &captured)
            .expect("更新目录"),
        "配置变化后不能发送旧范围"
    );
}
