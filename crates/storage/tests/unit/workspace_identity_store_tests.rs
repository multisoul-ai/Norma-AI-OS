use super::WorkspaceIdentityStore;

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
