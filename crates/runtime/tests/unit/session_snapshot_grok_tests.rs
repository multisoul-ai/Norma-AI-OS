use super::find_grok_session_id_inner;
use filetime::{set_file_mtime, FileTime};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

/// Grok Build 会话发现：只在当前 workspace 的 URL 编码目录中选择最近活动的 session。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace = `/Users/alan/Norma 项目`
///   encodeURIComponent(workspace) = `%2FUsers%2Falan%2FNorma%20%E9%A1%B9%E7%9B%AE`
///   当前 workspace session 数 = 2：`grok-old` + `grok-new`
///   foreign workspace session 数 = 1：`grok-foreign`
///   old updates.jsonl mtime = now - 2s；new updates.jsonl mtime = now
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在隔离 GROK_HOME 下创建当前 workspace 的 old/new session 目录；
///   2. 把 old 的活动文件时间回拨 2 秒，并在另一个 workspace 创建更新文件；
///   3. 调用 `find_grok_session_id_inner`，只扫描当前 workspace 的一级 session 目录；
///   4. 比较每个 session 的已知活动文件 mtime，选择最近的 UUID 目录名。
///
/// 预期结果：
///   - 正断言：返回当前 workspace 最近活动的 `grok-new`；
///   - 负断言：不得返回更旧的 `grok-old`；
///   - 负断言：不得跨 workspace 返回更新的 `grok-foreign`。
#[test]
fn grok_session_discovery_is_workspace_scoped_and_uses_latest_activity() {
    let grok_home = tempdir().expect("应创建隔离 GROK_HOME");
    let workspace = Path::new("/Users/alan/Norma 项目");
    let workspace_sessions = grok_home
        .path()
        .join("sessions")
        .join("%2FUsers%2Falan%2FNorma%20%E9%A1%B9%E7%9B%AE");
    let old_dir = workspace_sessions.join("grok-old");
    let new_dir = workspace_sessions.join("grok-new");
    fs::create_dir_all(&old_dir).expect("应创建旧 Grok session 目录");
    fs::create_dir_all(&new_dir).expect("应创建新 Grok session 目录");
    let old_updates = old_dir.join("updates.jsonl");
    let new_updates = new_dir.join("updates.jsonl");
    fs::write(&old_updates, b"{}\n").expect("应写入旧 Grok updates");
    fs::write(&new_updates, b"{}\n").expect("应写入新 Grok updates");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间应晚于 UNIX_EPOCH")
        .as_secs();
    // old = now - 2s；new = now，因此 new 的活动时间严格更晚。
    set_file_mtime(&old_updates, FileTime::from_unix_time((now - 2) as i64, 0))
        .expect("应回拨旧 Grok session 活动时间");
    set_file_mtime(&new_updates, FileTime::from_unix_time(now as i64, 0))
        .expect("应设置新 Grok session 活动时间");

    let foreign_dir = grok_home
        .path()
        .join("sessions")
        .join("%2FUsers%2Falan%2Fforeign")
        .join("grok-foreign");
    fs::create_dir_all(&foreign_dir).expect("应创建 foreign Grok session 目录");
    let foreign_updates = foreign_dir.join("updates.jsonl");
    fs::write(&foreign_updates, b"{}\n").expect("应写入 foreign Grok updates");
    // foreign = now + 2s，若实现错误地跨 workspace 扫描，它会错误胜出。
    set_file_mtime(
        &foreign_updates,
        FileTime::from_unix_time((now + 2) as i64, 0),
    )
    .expect("应设置 foreign Grok session 活动时间");

    let result = find_grok_session_id_inner(workspace, grok_home.path());

    assert_eq!(
        result.as_deref(),
        Some("grok-new"),
        "Grok discovery 应返回当前 workspace 最近活动的 session"
    );
    assert_ne!(
        result.as_deref(),
        Some("grok-old"),
        "Grok discovery 不得返回当前 workspace 更旧的 session"
    );
    assert_ne!(
        result.as_deref(),
        Some("grok-foreign"),
        "Grok discovery 不得跨 workspace 误绑更新的 foreign session"
    );
}
