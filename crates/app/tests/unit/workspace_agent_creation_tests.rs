use super::check_scope;
use crate::mobile::workspace_chat::WorkspaceArea;
use norma::mobile::WorkspaceAgentCreateRequest;
use storage::{AppStore, MobilePermission};

fn fixture() -> (
    tempfile::TempDir,
    AppStore,
    Vec<WorkspaceArea>,
    WorkspaceAgentCreateRequest,
) {
    let directory = tempfile::tempdir().expect("隔离创建目录");
    let project = directory.path().join("project");
    std::fs::create_dir(&project).expect("真实项目目录");
    let store = AppStore::new(directory.path().join("app.sqlite"));
    let catalog = store.desktop_workspace_store();
    let space = catalog.create("目标空间").expect("真实空间");
    let area = catalog.add_project(&space.id, &project).expect("真实区域");
    let project_id = store
        .workspace_identity_store()
        .resolve_or_create(&area.root, "项目")
        .expect("真实项目身份")
        .workspace_id;
    store
        .mobile_access_store()
        .register_all_projects(
            "phone",
            "iPhone",
            &"a".repeat(64),
            MobilePermission::Control,
        )
        .expect("当前和未来项目授权");
    let request = WorkspaceAgentCreateRequest {
        desktop_workspace_id: space.id.clone(),
        area_id: area.id.clone(),
        operation_id: "operation".into(),
        expected_revision: catalog.configuration_revision(&space.id).expect("配置版本"),
        profile: "Codex".into(),
    };
    (
        directory,
        store,
        vec![WorkspaceArea { area, project_id }],
        request,
    )
}

/// 场景：一次性控制当前和未来项目的 iPhone 可以在后来新增的真实区域创建 Agent。
/// 数据构造：1 个真实空间及区域；全项目 control 授权的逐项目列表为空；64 字符测试凭据沿用校验最低要求。
/// 执行过程：生成目录与授权 → 复核空间身份、版本、权限和目录 → 再使用错误凭据检查。
/// 预期结果：真实授权通过；错误凭据返回 unauthorized；不得要求用户重新逐项目授权。
#[test]
fn full_project_control_includes_new_agent_creation() {
    let (_directory, store, areas, request) = fixture();
    let correct = check_scope(
        store.path(),
        &areas,
        "phone",
        &"a".repeat(64),
        &request,
        true,
    );
    assert_eq!(
        correct,
        Ok(()),
        "全项目控制必须支持快捷创建，不能只检查旧逐项目列表"
    );
    let wrong = check_scope(
        store.path(),
        &areas,
        "phone",
        &"b".repeat(64),
        &request,
        true,
    );
    assert_eq!(
        wrong,
        Err("unauthorized".into()),
        "全项目范围不能绕过设备凭据检查"
    );
}

/// 场景：创建准备期间配置变化或控制权限撤销，应返回原始原因而非成功。
/// 数据构造：1 个精确目标；过期版本=当前版本+1；准备完成后撤销同一 phone。
/// 执行过程：检验过期版本 → 还原请求 → 撤销设备 → 在进程准备前再次核验。
/// 预期结果：版本冲突为 configuration_conflict；撤销为 unauthorized；原区域仍在，不误删项目。
#[test]
fn creation_rechecks_conflicts_and_revocation_before_launch() {
    let (_directory, store, areas, mut request) = fixture();
    // 未来版本 = 当前版本 + 1，模拟手机持有无法对应桌面配置的版本。
    request.expected_revision += 1;
    assert_eq!(
        check_scope(
            store.path(),
            &areas,
            "phone",
            &"a".repeat(64),
            &request,
            true
        ),
        Err("configuration_conflict".into()),
        "过期配置不能创建卡片"
    );
    request.expected_revision -= 1;
    store
        .mobile_access_store()
        .revoke("phone")
        .expect("撤销控制");
    assert_eq!(
        check_scope(
            store.path(),
            &areas,
            "phone",
            &"a".repeat(64),
            &request,
            false
        ),
        Err("unauthorized".into()),
        "启动前必须重新确认权限"
    );
    assert_eq!(
        store
            .desktop_workspace_store()
            .list_projects(&request.desktop_workspace_id)
            .expect("目录")[0]
            .id,
        request.area_id,
        "失败不能删掉已有项目区域"
    );
    assert_ne!(request.area_id, "other", "必须保留原区域身份");
}

/// 场景：准备期间撤销授权时保留具体失败原因，并阻止进程启动。
/// 数据构造：1 个授权空间与项目；计数初值 0；合法启动增加到 1；撤权后不再增加。
/// 执行过程：合法范围尝试启动 → 撤销 phone → 再尝试 → 比较次数与原始错误。
/// 预期结果：合法启动成功；撤权返回 unauthorized 而非 agent_start_failed；不得启动第二个进程。
#[test]
fn revoked_creation_preserves_reason_and_never_spawns() {
    let (_directory, store, areas, request) = fixture();
    let calls = std::cell::Cell::new(0);
    let allowed = super::launch_checked(
        store.path(),
        &areas,
        "phone",
        &"a".repeat(64),
        &request,
        || {
            calls.set(calls.get() + 1);
            Ok(())
        },
    );
    assert_eq!(allowed, Ok(()), "有效授权允许后台准备进程");
    assert_eq!(calls.get(), 1, "有效请求仅启动一次");
    store
        .mobile_access_store()
        .revoke("phone")
        .expect("撤销 phone");
    let denied = super::launch_checked(
        store.path(),
        &areas,
        "phone",
        &"a".repeat(64),
        &request,
        || {
            calls.set(calls.get() + 1);
            Ok(())
        },
    );
    assert_eq!(
        denied,
        Err("unauthorized".into()),
        "撤权不能误报工具未安装或启动失败"
    );
    assert_ne!(
        denied,
        Err("agent_start_failed".into()),
        "手机需要知道授权已撤销"
    );
    assert_eq!(calls.get(), 1, "权限撤销后不得实际启动第二次");
}
