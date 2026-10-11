use norma::workspace_dispatch::*;

// 场景：授权空空间的完整目录需要显式覆盖声明；构造零区域目录后改变登记状态；
// 调用共享完整性接口，预期未完成登记不能被 schema 存在或空数组误判完整。
#[test]
fn completeness_requires_registration_and_authorized_coverage() {
    let mut directory = WorkspaceDirectory {
        workspace_id: "space".into(),
        revision: 0,
        catalog_fingerprint: "catalog".into(),
        authorization_revision: "desktop".into(),
        authorized_area_ids: vec![],
        covered_area_ids: vec![],
        missing_area_ids: vec![],
        registration_complete: false,
        projects: vec![],
        areas: vec![],
        roles: vec![],
        call_keys: vec![],
    };
    assert!(!directory.is_complete(), "空角色名单不等于登记已经完成");
    directory.registration_complete = true;
    assert!(directory.is_complete(), "完整登记的空空间允许准确零候选");
    directory.authorized_area_ids.push("unread".into());
    assert!(
        !directory.is_complete(),
        "授权区域未覆盖即使缺失数组为空也不完整"
    );
}
