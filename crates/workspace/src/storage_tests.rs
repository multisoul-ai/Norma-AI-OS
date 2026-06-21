use super::*;
use crate::{WorkspaceRecord, WorkspaceSource};

/// S-1: 缺失 JSON 文件时 load 返回空最近项目列表。
///
/// 数据构造（含关键数值的推导过程）：
///   temp dir = 新建临时目录
///   json path = temp/recent-workspaces.json
///   文件存在性 = false，因此没有任何可解析记录
///
/// 执行过程（逐步说明系统如何处理）：
///   1. RecentStorage::new(json path)
///   2. load() 检查 path.exists() 为 false
///   3. 返回 RecentWorkspaces::default()
///
/// 预期结果：
///   - 正断言：load 成功且 records 为空
///   - 负断言：缺失文件不应返回 Json 或 Io 错误
#[test]
fn test_load_missing_recent_json_returns_empty_list() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let storage = RecentStorage::new(temp.path().join("recent-workspaces.json"));

    let recent = storage
        .load()
        .expect("缺失 recent-workspaces.json 应视为空列表而不是错误");

    assert!(recent.records.is_empty(), "缺失 JSON 时最近项目列表应为空");
    assert_eq!(recent.records.len(), 0, "缺失 JSON 不应构造任何占位记录");
}

/// S-2: record_opened 写入 JSON，并在重复打开时去重更新。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace path = temp/my-workspace，真实创建目录，因此 path.is_dir() = true
///   storage path = temp/state/recent-workspaces.json，父目录初始不存在
///   连续 record_opened 同一路径 2 次，按 path 去重后 records.len() 应仍为 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 创建 workspace 目录
///   2. record_opened(workspace, Open) → canonicalize、load empty、插入、创建父目录并写 JSON
///   3. 再 record_opened(workspace, Open) → 删除旧记录并写入新记录
///   4. load() 读取磁盘 JSON 验证持久化状态
///
/// 预期结果：
///   - 正断言：JSON 文件被创建，records.len() == 1
///   - 正断言：记录 path 是 canonical workspace path，display_name 是 my-workspace
///   - 负断言：重复打开同一路径不应产生第二条记录
#[test]
fn test_record_opened_persists_json_and_dedupes_reopened_path() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let workspace_path = temp.path().join("my-workspace");
    std::fs::create_dir(&workspace_path).expect("test should create workspace dir");
    let storage_path = temp.path().join("state").join("recent-workspaces.json");
    let storage = RecentStorage::new(storage_path.clone());

    storage
        .record_opened(&workspace_path, WorkspaceSource::Open)
        .expect("第一次 record_opened 应成功写入 JSON");
    storage
        .record_opened(&workspace_path, WorkspaceSource::Open)
        .expect("第二次 record_opened 应去重并成功写入 JSON");

    let recent = storage.load().expect("写入后的 JSON 应可重新读取");
    let canonical_path = workspace_path
        .canonicalize()
        .expect("测试目录应可 canonicalize");
    assert!(
        storage_path.is_file(),
        "record_opened 应创建 recent-workspaces.json 文件"
    );
    assert_eq!(
        recent.records.len(),
        1,
        "同一路径打开两次后应去重，只保留 1 条记录"
    );
    assert_eq!(
        recent.records[0].path, canonical_path,
        "持久化记录应保存 canonical workspace path"
    );
    assert_eq!(
        recent.records[0].display_name, "my-workspace",
        "display_name 应取 workspace 目录 basename"
    );
    assert_ne!(
        recent.records.len(),
        2,
        "重复打开同一路径不应产生第二条最近项目记录"
    );
}

/// S-3: 损坏 JSON 返回可处理的 Json 错误，不 panic 也不误报为空列表。
///
/// 数据构造（含关键数值的推导过程）：
///   json bytes = b"{not-valid-json"，缺少合法 key/value 与闭合结构
///   文件存在性 = true，因此 load 会进入 serde_json 解析路径
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 写入损坏 JSON
///   2. load() 读取 bytes 并调用 serde_json::from_slice
///   3. serde_json 返回错误，转换为 WorkspaceStorageError::Json
///
/// 预期结果：
///   - 正断言：load 返回 Json 错误
///   - 负断言：损坏 JSON 不应被当成空列表吞掉
#[test]
fn test_load_corrupt_recent_json_returns_json_error() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let storage_path = temp.path().join("recent-workspaces.json");
    std::fs::write(&storage_path, b"{not-valid-json").expect("test should write corrupt JSON");
    let storage = RecentStorage::new(storage_path);

    let result = storage.load();

    assert!(
        matches!(result, Err(WorkspaceStorageError::Json(_))),
        "损坏 JSON 应返回 WorkspaceStorageError::Json，实际: {:?}",
        result
    );
    assert!(
        !matches!(result, Ok(RecentWorkspaces { records }) if records.is_empty()),
        "损坏 JSON 不应被静默当作空列表，否则用户数据损坏会被隐藏"
    );
}

/// S-4: save 写出的 JSON 可被 load 还原为相同记录。
///
/// 数据构造（含关键数值的推导过程）：
///   record.path = /tmp/saved-workspace
///   last_opened_at_unix = 42（固定时间便于断言）
///   source = Clone（覆盖非 Open 分支的序列化）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 构造 RecentWorkspaces{records:[record]}
///   2. save() 写入 JSON
///   3. load() 解析 JSON
///
/// 预期结果：
///   - 正断言：还原后的记录与原记录一致
///   - 负断言：source 不应退化成 Open
#[test]
fn test_save_and_load_round_trips_recent_records() {
    let temp = tempfile::tempdir().expect("test should create temp dir");
    let storage = RecentStorage::new(temp.path().join("recent-workspaces.json"));
    let record = WorkspaceRecord::new(
        PathBuf::from("/tmp/saved-workspace"),
        42,
        WorkspaceSource::Clone,
    );
    let recent = RecentWorkspaces {
        records: vec![record.clone()],
    };

    storage.save(&recent).expect("save 应成功写入 JSON");
    let loaded = storage.load().expect("load 应成功还原 JSON");

    assert_eq!(
        loaded.records,
        vec![record],
        "save 后再 load 应还原完全相同的记录"
    );
    assert_ne!(
        loaded.records[0].source,
        WorkspaceSource::Open,
        "Clone source 不应在序列化往返后退化为 Open"
    );
}
