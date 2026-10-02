use super::*;

/// DWS-01：首页搜索只筛选桌面工作空间，且不会把旧最近项目当成空间。
///
/// 数据构造（含关键数值的推导过程）：
///   新目录 = 2 个桌面空间；旧最近项目 = 1 个目录；搜索 "des" 后匹配数 = 1。
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在独立测试数据库创建 Design 和 Operations 两个桌面工作空间。
///   2. 另建一个旧项目目录并写入 recent_workspaces。
///   3. 重新加载桌面状态，设置搜索为 "des" 并筛选。
///
/// 预期结果：
///   - 正断言：仅 Design 匹配，目录总数为两个桌面工作空间。
///   - 负断言：旧项目不在桌面目录中，Operations 不匹配搜索。
#[test]
fn desktop_workspace_search_excludes_legacy_recent_projects() {
    let temp = tempfile::tempdir().expect("测试应创建隔离目录");
    let db = temp.path().join("app.sqlite");
    let store = storage::DesktopWorkspaceStore::new(db.clone());
    let design = store.create("Design").expect("应创建 Design");
    store.create("Operations").expect("应创建 Operations");
    let old = temp.path().join("old-project");
    std::fs::create_dir(&old).expect("测试应创建旧项目目录");
    storage::RecentStore::new(db)
        .record_opened(&old, workspace::WorkspaceSource::Open)
        .expect("应记录旧项目");

    let mut state = DesktopWorkspaceUiState::load(store);
    state.search_query = "des".into();
    let filtered = state.filtered();

    assert_eq!(state.workspaces.len(), 2, "桌面目录应只含两个新工作空间");
    assert_eq!(filtered.len(), 1, "搜索 des 应只匹配一个工作空间");
    assert_eq!(filtered[0].id, design.id, "搜索结果应是 Design");
    assert!(
        !state
            .workspaces
            .iter()
            .any(|item| item.name == "old-project"),
        "旧最近项目不得自动成为桌面工作空间"
    );
    assert!(
        !filtered.iter().any(|item| item.name == "Operations"),
        "不匹配的 Operations 不得进入搜索结果"
    );
}
