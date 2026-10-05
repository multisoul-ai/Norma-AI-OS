use storage::{MobileAccessStore, MobilePermission, MobileProjectAccess};

/// 已撤销手机经 Mac 再次批准后可以重配，旧范围与迟到权限更新不能复活。
///
/// 数据构造（含关键数值的推导过程）：
///   一个设备 ID，旧/新 token 各 64 个 hex 字符 = 32 字节；旧范围为全项目控制。
///   revision：初次注册 1 → 撤销 1+1=2 → 重配 2+1=3。
/// 执行过程：
///   1. 注册全项目控制设备并撤销，确认旧 token 无效。
///   2. Mac 用同一设备 ID、新 token 和项目 A 只读范围重新批准。
///   3. 重开数据库，用两种 token 鉴权，尝试旧修订号更新及覆盖有效设备。
/// 预期结果：只保留一个 revision=3 的有效设备；新 token 可读 A，旧 token、
///   控制权限、全项目模式、迟到更新和未经撤销的覆盖注册均不可用。
#[test]
fn desktop_reapproval_reuses_revoked_device_without_old_permissions() {
    let dir = tempfile::tempdir().expect("隔离配对数据库");
    let path = dir.path().join("app.sqlite");
    let store = MobileAccessStore::new(path.clone());
    let old_token = "a".repeat(64);
    let new_token = "b".repeat(64);
    store
        .register_all_projects(
            "phone-a",
            "Old iPhone",
            &old_token,
            MobilePermission::Control,
        )
        .expect("首次批准全项目控制");
    let revoked = store.revoke("phone-a").expect("撤销旧手机授权");
    assert_eq!(revoked.revision, 2, "撤销必须推进到修订号 2");
    assert!(
        store
            .authenticate("phone-a", &old_token)
            .expect("撤销后鉴权")
            .is_none(),
        "Mac 再次批准前旧凭据不得恢复"
    );
    let read_scope = [MobileProjectAccess {
        workspace_id: "project-a".into(),
        access: MobilePermission::Read,
    }];
    let approved = store
        .register("phone-a", "Repaired iPhone", &new_token, &read_scope)
        .expect("Mac 明确批准已撤销的同一设备 ID 应成功");
    assert_eq!(approved.revision, 3, "重配不能把修订号重置为 1");
    assert!(!approved.revoked, "新批准的设备应解除撤销状态");
    assert_eq!(
        approved.all_projects_access, None,
        "不得继承旧全项目控制模式"
    );
    assert_eq!(
        approved.device_name, "Repaired iPhone",
        "名称应来自新核对的申请"
    );
    let reopened = MobileAccessStore::new(path);
    let grant = reopened
        .authenticate("phone-a", &new_token)
        .expect("重开后鉴权")
        .expect("新凭据必须持久化");
    assert!(
        grant.allows("project-a", false),
        "新批准的 A 只读权限必须存在"
    );
    assert!(!grant.allows("project-a", true), "旧控制权限不得残留");
    assert!(!grant.allows("project-b", false), "未选中的 B 不得授权");
    assert!(
        reopened
            .authenticate("phone-a", &old_token)
            .expect("旧凭据鉴权")
            .is_none(),
        "替换凭据后旧 token 不得恢复"
    );
    assert!(
        reopened.replace_permissions("phone-a", 1, &[]).is_err(),
        "初次授权的迟到更新不得命中新批准"
    );
    assert!(
        reopened
            .register("phone-a", "Overwrite", &old_token, &[])
            .is_err(),
        "有效设备仍须先撤销，不得直接覆盖"
    );
    assert_eq!(
        reopened.list().expect("读取设备列表").len(),
        1,
        "重配不得创建重复设备"
    );
    assert!(
        reopened
            .authenticate("phone-a", &new_token)
            .expect("覆盖失败后鉴权")
            .is_some(),
        "被拒绝的覆盖不能破坏新授权"
    );
}
