use super::*;
use workspace::WorkspaceSource;

/// R-1: RecentStore 以 SQLite 持久化最近项目，并按路径去重和时间倒序。
///
/// 数据构造（含关键数值的推导过程）：
///   first path = temp/alpha-workspace，真实目录
///   second path = temp/beta-workspace，真实目录
///   record count = first 打开 2 次 + second 打开 1 次，但按 path 去重后应为 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. RecentStore::new(app.sqlite)
///   2. record_opened(first, Open) → SQLite upsert 最近项目记录
///   3. record_opened(second, Open) → 写入第二条记录
///   4. record_opened(first, Clone) → first 去重更新为最新记录
///   5. load() → 按 last_opened_at_unix 倒序读取
///
/// 预期结果：
///   - 正断言：最终 records.len() == 2，第一条是 first
///   - 正断言：first 的 source 更新为 Clone
///   - 负断言：重复打开 first 不应产生第三条记录
#[test]
fn test_record_opened_persists_sqlite_and_dedupes_by_path() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let first = temp.path().join("alpha-workspace");
    let second = temp.path().join("beta-workspace");
    std::fs::create_dir(&first).expect("test should create first workspace");
    std::fs::create_dir(&second).expect("test should create second workspace");
    let store = RecentStore::new(temp.path().join("app.sqlite"));

    store
        .record_opened(&first, WorkspaceSource::Open)
        .expect("first open 应写入 SQLite");
    store
        .record_opened(&second, WorkspaceSource::Open)
        .expect("second open 应写入 SQLite");
    store
        .record_opened(&first, WorkspaceSource::Clone)
        .expect("reopen first 应去重更新 SQLite");

    let recent = store.load().expect("recent records 应可从 SQLite 读取");
    let first_canonical = first
        .canonicalize()
        .expect("first workspace should canonicalize");

    assert_eq!(
        recent.records.len(),
        2,
        "三次打开两个路径后应只保留 2 条最近项目记录"
    );
    assert_eq!(
        recent.records[0].path, first_canonical,
        "first 重开后应成为最新的第一条记录"
    );
    assert_eq!(
        recent.records[0].source,
        WorkspaceSource::Clone,
        "first 最新记录应保存 Clone source"
    );
    assert_ne!(recent.records.len(), 3, "重复打开 first 不应追加第三条记录");
}

/// R-2: save/load 往返保留固定时间和 Clone 来源。
///
/// 数据构造（含关键数值的推导过程）：
///   record.path = /tmp/saved-workspace
///   last_opened_at_unix = 42（固定时间，避免依赖系统时钟）
///   source = Clone（覆盖非 Open 分支）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 RecentWorkspaces{records:[record]}
///   2. save() 删除旧表内容并插入 record
///   3. load() 从 SQLite 读回 record
///
/// 预期结果：
///   - 正断言：读回 records 与写入 records 完全一致
///   - 负断言：source 不应退化为 Open
#[test]
fn test_save_and_load_round_trips_recent_records() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let store = RecentStore::new(temp.path().join("app.sqlite"));
    let record = WorkspaceRecord::new(
        PathBuf::from("/tmp/saved-workspace"),
        42,
        WorkspaceSource::Clone,
    );
    let recent = RecentWorkspaces {
        records: vec![record.clone()],
    };

    store.save(&recent).expect("save 应成功写入 SQLite");
    let loaded = store.load().expect("load 应成功读回 SQLite");

    assert_eq!(
        loaded.records,
        vec![record],
        "SQLite save/load 应保留完整最近项目记录"
    );
    assert_ne!(
        loaded.records[0].source,
        WorkspaceSource::Open,
        "Clone source 不应在 SQLite 往返后退化为 Open"
    );
}

/// R-3: 删除最近项目必须从 SQLite 持久化表移除精确路径，且不影响其它项目。
///
/// 数据构造（含关键数值的推导过程）：
///   alpha path = temp/alpha-workspace，真实目录
///   beta path  = temp/beta-workspace，真实目录
///   初始记录数 = 2；删除目标数 = 1；预期持久化剩余数 = 2 - 1 = 1
///   beta 目录在删除 Recent 记录前被移除，因此 canonicalize(beta) 会失败
///
/// 执行过程（逐步说明系统如何处理）：
///   1. record_opened(alpha) 与 record_opened(beta) 写入 app.sqlite。
///   2. 删除 beta 目录，再以已存的 canonical path 从 recent_workspaces 删除 beta。
///   3. 重新 load()，确认删除跨 store 调用仍已持久化。
///
/// 预期结果：
///   - 正断言：即使 beta 文件夹已不存在，第一次 remove 仍返回 true，重新读取只保留 alpha。
///   - 负断言：beta 不应残留在 SQLite，第二次 remove 应返回 false。
#[test]
fn test_remove_deletes_exact_recent_workspace_from_sqlite() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let alpha = temp.path().join("alpha-workspace");
    let beta = temp.path().join("beta-workspace");
    std::fs::create_dir(&alpha).expect("test should create alpha workspace");
    std::fs::create_dir(&beta).expect("test should create beta workspace");
    let store = RecentStore::new(temp.path().join("app.sqlite"));

    store
        .record_opened(&alpha, WorkspaceSource::Open)
        .expect("alpha 应写入 SQLite recent_workspaces");
    store
        .record_opened(&beta, WorkspaceSource::Open)
        .expect("beta 应写入 SQLite recent_workspaces");
    let alpha_canonical = alpha
        .canonicalize()
        .expect("alpha workspace should canonicalize");
    let beta_canonical = beta
        .canonicalize()
        .expect("beta workspace should canonicalize before it is removed");
    std::fs::remove_dir(&beta)
        .expect("test should remove beta workspace to model a stale recent path");

    let removed = store
        .remove(&beta_canonical)
        .expect("删除已不存在的 beta Recent 记录仍应成功执行 SQLite DELETE");
    let removed_again = store
        .remove(&beta_canonical)
        .expect("重复删除 beta 应成功返回删除计数");
    let loaded = store
        .load()
        .expect("删除后应可重新读取 SQLite recent_workspaces");

    assert!(removed, "第一次删除存在的 beta 应报告已删除");
    assert_eq!(
        loaded.records.len(),
        1,
        "从两条记录删除 beta 后 SQLite 中应只保留一条"
    );
    assert_eq!(
        loaded.records[0].path, alpha_canonical,
        "删除 beta 不应影响 alpha 的持久化记录"
    );
    assert!(
        !loaded
            .records
            .iter()
            .any(|record| record.path == beta_canonical),
        "beta 删除后不应残留在 SQLite 重新加载结果中"
    );
    assert!(
        !removed_again,
        "重复删除已经不存在的 beta 应返回 false，不能误报删除成功"
    );
}
