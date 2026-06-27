//! Unit tests for session_snapshot logic.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

use crate::session_snapshot::{
    build_resume_command, build_terminal_restore_command, find_agent_session_id_inner,
};

fn write_jsonl_file(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.jsonl"));
    fs::write(&path, b"{}").unwrap();
    path
}

/// SS-1: find_agent_session_id_inner 在 ~/.claude/projects/<encoded>/ 只有一个 .jsonl 时返回其 stem。
///
/// 数据构造：
///   workspace_root = <tmp-dir>/myproject（由 tempdir 生成）
///   encoded_name = strip leading '/', replace '/' with '-'
///   projects_dir 中 1 个 .jsonl: "deadbeef-1234.jsonl"
///
/// 执行过程：
///   1. 创建 home/<encoded>/deadbeef-1234.jsonl
///   2. 调用 find_agent_session_id_inner(workspace, home)
///
/// 预期结果：
///   - Some("deadbeef-1234")
#[test]
fn test_find_session_single_file() {
    let home = tempdir().unwrap();
    let workspace = tempdir().unwrap();

    let encoded = encode_path_for_claude(workspace.path().to_str().unwrap());
    let projects_dir = home.path().join(".claude").join("projects").join(encoded);
    fs::create_dir_all(&projects_dir).unwrap();
    write_jsonl_file(&projects_dir, "deadbeef-1234");

    let result = find_agent_session_id_inner(workspace.path(), home.path());
    assert_eq!(
        result.as_deref(),
        Some("deadbeef-1234"),
        "单文件时应返回该文件的 stem"
    );
}

/// SS-2: find_agent_session_id_inner 有多个 .jsonl 时返回最新修改的那个 stem。
///
/// 数据构造：
///   older file: "old-session.jsonl"（mtime = 2秒前，通过 filetime::set_file_mtime 设置）
///   newer file: "new-session.jsonl"（mtime = 当前系统时间，默认创建时间）
///
/// 执行过程：
///   1. 写 old-session.jsonl，将其 mtime 设为 2 秒前
///   2. 写 new-session.jsonl（mtime = 当前）
///   3. 调用 find_agent_session_id_inner
///
/// 预期结果：
///   - Some("new-session")，即 mtime 最新的文件 stem
#[test]
fn test_find_session_returns_most_recent() {
    let home = tempdir().unwrap();
    let workspace = tempdir().unwrap();

    let encoded = encode_path_for_claude(workspace.path().to_str().unwrap());
    let projects_dir = home.path().join(".claude").join("projects").join(encoded);
    fs::create_dir_all(&projects_dir).unwrap();

    let old_path = write_jsonl_file(&projects_dir, "old-session");
    // Set mtime to 2 seconds ago so it is clearly older than new-session
    let past = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 2;
    filetime::set_file_mtime(
        &old_path,
        filetime::FileTime::from_unix_time(past as i64, 0),
    )
    .unwrap();

    write_jsonl_file(&projects_dir, "new-session");

    let result = find_agent_session_id_inner(workspace.path(), home.path());
    assert_eq!(
        result.as_deref(),
        Some("new-session"),
        "应返回 mtime 最新的文件 stem，而非 old-session"
    );
}

/// SS-3: workspace_root 对应的 projects 目录不存在时返回 None。
///
/// 数据构造：
///   home 和 workspace 均为 tempdir，但不创建 projects 目录
///
/// 预期结果：
///   - None
#[test]
fn test_find_session_no_projects_dir_returns_none() {
    let home = tempdir().unwrap();
    let workspace = tempdir().unwrap();
    // Do NOT create any projects directory

    let result = find_agent_session_id_inner(workspace.path(), home.path());
    assert!(
        result.is_none(),
        "projects 目录不存在时应返回 None，实际: {:?}",
        result
    );
}

/// SS-4: projects 目录存在但无 .jsonl 文件时返回 None。
///
/// 数据构造：
///   projects_dir 存在，但只包含一个 other.txt（非 .jsonl）
///
/// 预期结果：
///   - None
#[test]
fn test_find_session_empty_dir_returns_none() {
    let home = tempdir().unwrap();
    let workspace = tempdir().unwrap();

    let encoded = encode_path_for_claude(workspace.path().to_str().unwrap());
    let projects_dir = home.path().join(".claude").join("projects").join(encoded);
    fs::create_dir_all(&projects_dir).unwrap();
    // Write a non-.jsonl file to make sure it's ignored
    fs::write(projects_dir.join("other.txt"), b"").unwrap();

    let result = find_agent_session_id_inner(workspace.path(), home.path());
    assert!(
        result.is_none(),
        "无 .jsonl 文件时应返回 None（other.txt 应被忽略），实际: {:?}",
        result
    );
}

/// SS-5: build_resume_command 生成的 PtyCommand 参数中包含 "--resume abc123"。
///
/// 数据构造：
///   shell = "/bin/zsh"
///   session_id = "abc123"
///   hook_env = None
///
/// 执行过程：
///   build_resume_command("/bin/zsh", "abc123", None)
///   → PtyCommand { program="/bin/zsh", args=["-ic", "exec claude ... --resume abc123"] }
///
/// 预期结果：
///   - args 拼接后包含子字符串 "--resume abc123"
///   - args 不为空（确保命令结构存在）
#[test]
fn test_build_resume_command_contains_resume_flag() {
    let cmd = build_resume_command("/bin/zsh", "abc123", None);
    let args_str = cmd.args.join(" ");
    assert!(
        args_str.contains("--resume abc123"),
        "resume command 的参数应包含 --resume abc123，实际: {args_str}"
    );
    assert!(
        !cmd.args.is_empty(),
        "build_resume_command 应生成非空 args 列表"
    );
}

/// SS-6: build_terminal_restore_command 生成的 PtyCommand cwd 为指定路径。
///
/// 数据构造：
///   shell = "/bin/zsh"
///   cwd = "/tmp/myproject"
///
/// 执行过程：
///   build_terminal_restore_command("/bin/zsh", "/tmp/myproject")
///   → PtyCommand { cwd: Some(PathBuf::from("/tmp/myproject")), ... }
///
/// 预期结果：
///   - cmd.cwd == Some(Path::new("/tmp/myproject"))
///   - cmd.cwd 不为 None（cwd 未设置则 restore 无效）
#[test]
fn test_build_terminal_restore_command_sets_cwd() {
    let cmd = build_terminal_restore_command("/bin/zsh", "/tmp/myproject");
    assert_eq!(
        cmd.cwd.as_deref(),
        Some(std::path::Path::new("/tmp/myproject")),
        "terminal restore command 的 cwd 应为指定路径 /tmp/myproject"
    );
    assert!(
        cmd.cwd.is_some(),
        "build_terminal_restore_command 必须设置 cwd，否则 restore 后终端仍从默认目录启动"
    );
}

// Helper: encode workspace path the same way Claude does.
fn encode_path_for_claude(path: &str) -> String {
    path.trim_start_matches('/').replace('/', "-")
}
