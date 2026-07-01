//! Unit tests for session_snapshot logic.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

use crate::session_snapshot::{
    build_agent_resume_command, build_terminal_restore_command, find_agent_session_id_inner,
    find_codex_session_id_inner,
};
use canvas::AgentProfile;

fn write_jsonl_file(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.jsonl"));
    fs::write(&path, b"{}").unwrap();
    path
}

/// SS-1: find_agent_session_id_inner 在 ~/.claude/projects/<encoded>/ 只有一个 .jsonl 时返回其 stem。
///
/// 数据构造：
///   workspace_root = <tmp-dir>/myproject（由 tempdir 生成）
///   encoded_name = replace every '/' with '-'
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

/// SS-4b: Claude Code 的 project 目录保留绝对路径开头的 `-`，应能找到会话文件。
///
/// 数据构造（含关键数值的推导过程）：
///   workspace_root = "/Users/alan/project"
///   Claude Code 实际编码 = path.replace("/", "-")
///   "/Users/alan/project" → "-Users-alan-project"
///   错误旧编码 = trim leading "/" 后 replace("/") → "Users-alan-project"
///   projects_dir 中 1 个 .jsonl: "real-claude-session.jsonl"
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 在 home/.claude/projects/-Users-alan-project 创建 session jsonl
///   2. 调用 find_agent_session_id_inner("/Users/alan/project", home)
///   3. 函数按 Claude Code 目录编码读取 jsonl，并返回文件 stem
///
/// 预期结果：
///   - 正断言：返回 real-claude-session，说明能读取真实 Claude Code 目录
///   - 负断言：不返回 None，否则恢复时 node_sessions.session_id 会为空
#[test]
fn test_find_session_uses_claude_absolute_path_encoding() {
    let home = tempdir().unwrap();
    let workspace = std::path::Path::new("/Users/alan/project");
    let projects_dir = home
        .path()
        .join(".claude")
        .join("projects")
        .join("-Users-alan-project");
    fs::create_dir_all(&projects_dir).unwrap();
    write_jsonl_file(&projects_dir, "real-claude-session");

    let result = find_agent_session_id_inner(workspace, home.path());

    assert_eq!(
        result.as_deref(),
        Some("real-claude-session"),
        "应按 Claude Code 真实编码 -Users-alan-project 找到 session jsonl"
    );
    assert!(
        result.is_some(),
        "真实 Claude Code 目录存在时不应返回 None，否则 Claude 卡片无法 --resume"
    );
}

/// SS-5: find_codex_session_id_inner 按 session_meta.cwd 匹配 workspace，并返回最新 session id。
///
/// 数据构造：
///   codex_home = temp/.codex
///   workspace = temp/workspace，真实目录，便于 canonicalize
///   older session id = 019f-old，mtime = 2 秒前，cwd 匹配 workspace
///   newer session id = 019f-new，mtime = 当前，cwd 匹配 workspace
///   foreign session id = 019f-foreign，cwd 指向另一个目录，不能被选中
///
/// 执行过程：
///   1. 创建三个 Codex JSONL session 文件
///   2. 调用 find_codex_session_id_inner(workspace, codex_home)
///   3. 根据 cwd 过滤后按 mtime 选择最新匹配文件
///
/// 预期结果：
///   - 正断言：返回 019f-new
///   - 负断言：不返回更旧的 019f-old 或 cwd 不匹配的 019f-foreign
#[test]
fn test_find_codex_session_returns_most_recent_matching_workspace() {
    let temp = tempdir().unwrap();
    let codex_home = temp.path().join(".codex");
    let sessions_dir = codex_home
        .join("sessions")
        .join("2026")
        .join("07")
        .join("01");
    fs::create_dir_all(&sessions_dir).unwrap();
    let workspace = temp.path().join("workspace");
    let foreign_workspace = temp.path().join("foreign");
    fs::create_dir(&workspace).unwrap();
    fs::create_dir(&foreign_workspace).unwrap();

    let old_path = write_codex_session_file(&sessions_dir, "old", "019f-old", &workspace);
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
    write_codex_session_file(&sessions_dir, "new", "019f-new", &workspace);
    write_codex_session_file(&sessions_dir, "foreign", "019f-foreign", &foreign_workspace);

    let result = find_codex_session_id_inner(&workspace, &codex_home);

    assert_eq!(
        result.as_deref(),
        Some("019f-new"),
        "应返回 cwd 匹配 workspace 且 mtime 最新的 Codex session id"
    );
    assert_ne!(
        result.as_deref(),
        Some("019f-old"),
        "不应返回更旧的 Codex session id"
    );
    assert_ne!(
        result.as_deref(),
        Some("019f-foreign"),
        "不应返回 cwd 不匹配 workspace 的 Codex session id"
    );
}

/// SS-6: build_agent_resume_command 为 Claude 生成带 --resume 的命令。
///
/// 数据构造：
///   shell = "/bin/zsh"
///   session_id = "abc123"
///   hook_env = None
///
/// 执行过程：
///   build_agent_resume_command(Claude, "/bin/zsh", "abc123", None, None)
///   → PtyCommand { program="/bin/zsh", args=["-ic", "exec claude ... --resume abc123"] }
///
/// 预期结果：
///   - args 拼接后包含子字符串 "--resume abc123"
///   - args 不为空（确保命令结构存在）
#[test]
fn test_build_claude_agent_resume_command_contains_resume_flag() {
    let cmd = build_agent_resume_command(AgentProfile::Claude, "/bin/zsh", "abc123", None, None);
    let args_str = cmd.args.join(" ");
    assert!(
        args_str.contains("--resume 'abc123'"),
        "resume command 的参数应包含 shell-quoted --resume 'abc123'，实际: {args_str}"
    );
    assert!(
        !cmd.args.is_empty(),
        "build_agent_resume_command 应生成非空 args 列表"
    );
}

/// SS-7: build_agent_resume_command 为 Codex 生成交互式 resume 命令。
///
/// 数据构造：
///   profile = Codex
///   session_id = "019f1bdb-40df-7813-8057-60f429c38986"
///   workspace_root = "/tmp/multisoul desktop"，包含空格，用于验证 shell quoting
///
/// 执行过程：
///   1. build_agent_resume_command(Codex, "/bin/zsh", session_id, None, workspace_root)
///   2. 拼接 args 检查 Codex CLI 参数
///
/// 预期结果：
///   - 正断言：命令包含 codex resume、session_id、quoted --cd
///   - 负断言：不包含 Claude 专属 --resume flag
#[test]
fn test_build_codex_agent_resume_command_contains_resume_subcommand() {
    let workspace_root = std::path::Path::new("/tmp/multisoul desktop");
    let cmd = build_agent_resume_command(
        AgentProfile::Codex,
        "/bin/zsh",
        "019f1bdb-40df-7813-8057-60f429c38986",
        None,
        Some(workspace_root),
    );
    let args_str = cmd.args.join(" ");

    assert!(
        args_str.contains("codex"),
        "Codex resume command 应调用 codex CLI，实际: {args_str}"
    );
    assert!(
        args_str.contains(" resume '019f1bdb-40df-7813-8057-60f429c38986'"),
        "Codex resume command 应包含 resume subcommand 与 session id，实际: {args_str}"
    );
    assert!(
        args_str.contains("--cd '/tmp/multisoul desktop'"),
        "Codex resume command 应 shell-quote workspace path，实际: {args_str}"
    );
    assert!(
        !args_str.contains(" --resume "),
        "Codex interactive resume 不应使用 Claude 的 --resume flag，实际: {args_str}"
    );
}

/// SS-8: build_terminal_restore_command 生成的 PtyCommand cwd 为指定路径。
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

fn write_codex_session_file(
    dir: &std::path::Path,
    name: &str,
    session_id: &str,
    workspace: &std::path::Path,
) -> PathBuf {
    let path = dir.join(format!("rollout-{name}.jsonl"));
    fs::write(
        &path,
        format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{}\",\"cwd\":\"{}\"}}}}\n",
            session_id,
            workspace.display()
        ),
    )
    .unwrap();
    path
}

// Helper: encode workspace path the same way Claude does.
fn encode_path_for_claude(path: &str) -> String {
    path.replace('/', "-")
}
