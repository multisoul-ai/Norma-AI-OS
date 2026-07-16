use super::*;

/// R-1: record_opened_at 生成显示名、按最近时间倒序，并对同一路径去重。
///
/// 数据构造（含关键数值的推导过程）：
///   first path  = /tmp/alpha-project，opened_at=10
///   second path = /tmp/beta-project，opened_at=20
///   reopen first opened_at=30，因此排序应变为 first(30), second(20)
///   max count = 50，本测试只放 2 条，不触发截断
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 记录 first@10 → records=[first]
///   2. 记录 second@20 → records=[second, first]
///   3. 再记录 first@30 → 删除旧 first，再插入新 first，并按时间倒序
///
/// 预期结果：
///   - 正断言：最终只有 2 条，first 被去重且在第一位
///   - 正断言：display_name 来自路径 basename
///   - 负断言：列表中不应存在旧 first@10 记录
#[test]
fn test_record_opened_sorts_by_latest_and_dedupes_path() {
    let mut recent = RecentWorkspaces::default();
    let first = PathBuf::from("/tmp/alpha-project");
    let second = PathBuf::from("/tmp/beta-project");

    recent.record_opened_at(first.clone(), WorkspaceSource::Open, 10);
    recent.record_opened_at(second.clone(), WorkspaceSource::Open, 20);
    recent.record_opened_at(first.clone(), WorkspaceSource::Open, 30);

    assert_eq!(
        recent.records.len(),
        2,
        "重复打开 first 后应按 path 去重，只保留 first 与 second 两条"
    );
    assert_eq!(
        recent.records[0].path, first,
        "first 以 opened_at=30 重开后应排在第一位"
    );
    assert_eq!(
        recent.records[0].display_name, "alpha-project",
        "display_name 应取路径 basename alpha-project"
    );
    assert_eq!(
        recent.records[1].path, second,
        "second 应作为较早打开项目排在第二位"
    );
    assert!(
        !recent.records.iter().any(|record| {
            record.path.as_path() == std::path::Path::new("/tmp/alpha-project")
                && record.last_opened_at_unix == 10
        }),
        "旧 first@10 记录不应继续存在，否则最近项目会重复显示"
    );
}

/// R-2: 最近项目超过 50 条时只保留最新 50 条。
///
/// 数据构造（含关键数值的推导过程）：
///   MAX_RECENT_WORKSPACES = 50
///   插入总数 = MAX + 2 = 52
///   opened_at 使用 0..51，最新两条为 51 与 50，最旧两条为 0 与 1
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次插入 52 个不同路径
///   2. 每次插入后 sort_and_truncate，最终只保留 opened_at 51..2
///   3. 查询第一条、最后一条与被裁剪的路径
///
/// 预期结果：
///   - 正断言：records.len() == 50
///   - 正断言：第一条是 opened_at=51，最后一条是 opened_at=2
///   - 负断言：opened_at=0 的最旧项目不应存在
#[test]
fn test_recent_workspaces_retains_latest_fifty_records() {
    let mut recent = RecentWorkspaces::default();

    for index in 0..(MAX_RECENT_WORKSPACES + 2) {
        recent.record_opened_at(
            PathBuf::from(format!("/tmp/project-{index:02}")),
            WorkspaceSource::Open,
            index as u64,
        );
    }

    assert_eq!(
        recent.records.len(),
        MAX_RECENT_WORKSPACES,
        "最近项目应最多保留 50 条，避免启动页列表无限膨胀"
    );
    assert_eq!(
        recent.records[0].last_opened_at_unix, 51,
        "最新 opened_at=51 的项目应位于第一条"
    );
    assert_eq!(
        recent.records[MAX_RECENT_WORKSPACES - 1].last_opened_at_unix,
        2,
        "保留 50 条时最后一条应是 opened_at=2"
    );
    assert!(
        !recent
            .records
            .iter()
            .any(|record| record.last_opened_at_unix == 0),
        "opened_at=0 的最旧项目应被截断，不应继续显示"
    );
}

/// R-3: 用户删除一个 Recent projects 项目时，仅移除该路径且保留其余排序记录。
///
/// 数据构造（含关键数值的推导过程）：
///   alpha path = /tmp/alpha-project，opened_at=30，因此初始排序第 1
///   beta path  = /tmp/beta-project，opened_at=20，因此初始排序第 2
///   gamma path = /tmp/gamma-project，opened_at=10，因此初始排序第 3
///   删除目标数 = 1；预期剩余数 = 3 - 1 = 2
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 依次记录 alpha、beta、gamma，得到按时间倒序的三个最近项目。
///   2. remove_path(beta) 从 RecentWorkspaces 中移除精确匹配的路径。
///   3. 查询删除结果与剩余项目顺序。
///
/// 预期结果：
///   - 正断言：remove_path 返回 true，alpha 与 gamma 仍存在且保持相对顺序。
///   - 负断言：beta 不应继续存在，重复删除 beta 应返回 false。
#[test]
fn test_remove_path_removes_only_the_requested_recent_workspace() {
    let mut recent = RecentWorkspaces::default();
    let alpha = PathBuf::from("/tmp/alpha-project");
    let beta = PathBuf::from("/tmp/beta-project");
    let gamma = PathBuf::from("/tmp/gamma-project");

    recent.record_opened_at(alpha.clone(), WorkspaceSource::Open, 30);
    recent.record_opened_at(beta.clone(), WorkspaceSource::Open, 20);
    recent.record_opened_at(gamma.clone(), WorkspaceSource::Open, 10);

    let removed = recent.remove_path(&beta);
    let removed_again = recent.remove_path(&beta);

    assert!(removed, "删除存在的 beta 最近项目应返回 true");
    assert_eq!(
        recent.records.len(),
        2,
        "删除 1 条后 RecentWorkspaces 应从 3 条精确变为 2 条"
    );
    assert_eq!(
        recent.records[0].path, alpha,
        "删除 beta 后较新的 alpha 应继续保留在第一位"
    );
    assert_eq!(
        recent.records[1].path, gamma,
        "删除 beta 后 gamma 应继续保留且成为第二位"
    );
    assert!(
        !recent.records.iter().any(|record| record.path == beta),
        "beta 已删除后不应继续出现在 RecentWorkspaces"
    );
    assert!(
        !removed_again,
        "重复删除不存在的 beta 应返回 false，不能误报再次删除成功"
    );
}
