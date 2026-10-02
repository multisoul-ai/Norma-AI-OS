use storage::DesktopWorkspaceStore;

/// 工作空间名称：80 个 Unicode 字符合法，81 个非法，同名记录独立且重启读取不变。
///
/// 数据构造：边界名 = 80 个“名”（UTF-8 共 80×3=240 字节），超长名 = 边界名 + 1 个“名”。
/// 执行过程：验证首尾空白处理 → 创建两条相同边界名 → 拒绝第 81 字符 → 新 store 读取 → 改名。
/// 预期结果：按字符而非字节计数；两条 ID 不同且名称完整；非法创建不增记录，改名只修改目标。
#[test]
fn desktop_workspace_names_preserve_unicode_boundaries_and_identity() {
    let temp = tempfile::tempdir().expect("应创建隔离目录");
    let path = temp.path().join("app.sqlite");
    let store = DesktopWorkspaceStore::new(path.clone());
    let name = "名".repeat(80);
    let padded = format!("  {name}\n");
    assert_eq!(
        DesktopWorkspaceStore::validate_name(&padded).expect("80 个字符应合法"),
        name,
        "规则应 trim 并保留完整 Unicode 名称"
    );
    assert!(
        DesktopWorkspaceStore::validate_name(" \n ").is_err(),
        "空白不能通过 UI 共用校验"
    );
    let a = store.create(&padded).expect("应保存首条边界名");
    let b = store.create(&name).expect("允许同名独立空间");
    assert_ne!(a.id, b.id, "同名空间不能合并或复用 ID");
    assert!(
        store.create(&format!("{name}名")).is_err(),
        "第 81 个字符必须拒绝而非截断"
    );
    let reopened = DesktopWorkspaceStore::new(path);
    let saved = reopened.list().expect("新实例应读出持久化目录");
    assert_eq!(saved.len(), 2, "非法创建不能写入第三条记录");
    assert_eq!(saved[0].name, name, "最新空间应保留完整 80 个字符");
    assert_eq!(saved[1].name, name, "较早的同名空间也应保留完整名称");
    reopened
        .rename(&a.id, "发布验收")
        .expect("创建后仍应支持改名");
    let renamed = reopened.list().expect("应读到改名结果");
    assert_eq!(
        renamed
            .iter()
            .find(|item| item.id == a.id)
            .expect("A 应保留")
            .name,
        "发布验收",
        "A 应改为新名称"
    );
    assert_eq!(
        renamed
            .iter()
            .find(|item| item.id == b.id)
            .expect("B 应保留")
            .name,
        name,
        "B 不应因原来同名而被修改"
    );
}
