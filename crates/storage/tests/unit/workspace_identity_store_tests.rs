use super::WorkspaceIdentityStore;

/// 手机授权候选项目来自本 App 数据库稳定身份，不能把另一电脑的项目混入。
///
/// 数据构造：A/B 两个隔离 App 数据库；A 登记 Alpha/Beta，B 仅登记 Foreign。
/// 执行过程：列空 A → 登记三个身份 → 重开 A 并列出 → 查 B。
/// 预期结果：A 只列 Alpha/Beta 的真实 ID/路径；Foreign 不出现在 A，重开不创建新身份。
#[test]
fn list_returns_only_persisted_identities_for_this_app() {
    let temp = tempfile::tempdir().expect("隔离目录");
    let a = WorkspaceIdentityStore::new(temp.path().join("a.sqlite"));
    let b = WorkspaceIdentityStore::new(temp.path().join("b.sqlite"));
    assert!(
        a.list().expect("空表").is_empty(),
        "无已登记项目时不得伪造候选项"
    );
    let alpha = a.resolve_or_create(temp.path(), "Alpha").expect("Alpha");
    let beta_path = temp.path().join("beta");
    std::fs::create_dir(&beta_path).expect("Beta 目录");
    let beta = a.resolve_or_create(&beta_path, "Beta").expect("Beta");
    let foreign = b
        .resolve_or_create(temp.path(), "Foreign")
        .expect("Foreign");
    let listed = WorkspaceIdentityStore::new(temp.path().join("a.sqlite"))
        .list()
        .expect("重开列表");
    assert_eq!(
        listed,
        vec![alpha, beta],
        "必须保留精确 ID/路径且按名称稳定排序"
    );
    assert!(
        !listed
            .iter()
            .any(|p| p.workspace_id == foreign.workspace_id),
        "不能泄漏另一 App 数据库项目"
    );
    assert_eq!(
        b.list().expect("B 列表"),
        vec![foreign],
        "读取 A 不得改动 B"
    );
}

/// Workspace 稳定身份：同一路径重复解析必须复用 ID，不同路径必须隔离。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace roots = alpha(1) + beta(1) = 2 个 canonical root
///   alpha resolves  = first(1) + repeated(1) = 2 次
///   expected IDs    = alpha(1) + beta(1) = 2 个稳定 ID
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 为 alpha 首次解析本机 Workspace 身份。
///   2. 用同一 alpha root 再次解析，但更新展示名称。
///   3. 为 beta 解析独立身份。
///
/// 预期结果：
///   - 正断言：alpha 两次解析返回同一 Workspace ID。
///   - 正断言：重复解析会更新 alpha 的展示名称。
///   - 正断言：beta 得到不同 Workspace ID。
///   - 负断言：两个 Workspace 不得因同一数据库而共享身份。
#[test]
fn resolve_or_create_reuses_canonical_root_and_separates_workspaces() {
    let temp = tempfile::tempdir().expect("测试应创建隔离临时目录");
    let app_db = temp.path().join("app.sqlite");
    let alpha = temp.path().join("alpha");
    let beta = temp.path().join("beta");
    std::fs::create_dir_all(&alpha).expect("测试应创建 alpha Workspace");
    std::fs::create_dir_all(&beta).expect("测试应创建 beta Workspace");
    let store = WorkspaceIdentityStore::new(app_db);

    let alpha_first = store
        .resolve_or_create(&alpha, "Alpha")
        .expect("alpha 首次解析应创建 WorkspaceIdentity");
    let alpha_repeated = store
        .resolve_or_create(&alpha, "Alpha Renamed")
        .expect("alpha 重复解析应复用 WorkspaceIdentity");
    let beta_identity = store
        .resolve_or_create(&beta, "Beta")
        .expect("beta 应创建独立 WorkspaceIdentity");

    assert_eq!(
        alpha_first.workspace_id, alpha_repeated.workspace_id,
        "同一 canonical root 必须复用稳定 Workspace ID"
    );
    assert_eq!(
        alpha_repeated.display_name, "Alpha Renamed",
        "重复解析应更新用户可见名称而不更换身份"
    );
    assert_ne!(
        alpha_first.workspace_id, beta_identity.workspace_id,
        "不同 canonical root 必须获得不同 Workspace ID"
    );
    assert_ne!(
        alpha_first.canonical_root, beta_identity.canonical_root,
        "不同 Workspace 的 canonical root 不得混写"
    );
}
