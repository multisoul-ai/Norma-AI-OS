//! InfCode CLI 恢复命令与会话发现测试。

use crate::session_snapshot::{build_agent_resume_command, find_agent_session_id_for_profile};
use canvas::AgentProfile;
use std::path::Path;

/// InfCode 恢复命令：必须使用已验证的 `--session <ID>`，不得用 `resume` 子命令。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = "inf-session-42"（hook 缓存 ID 占位）
///   带单引号的 ID = "bad'id"（验证 shell 引用：' 被转义为 '\''，共 1 处）
///
/// 执行过程：
///   1. 用正常 ID 构造 InfCode 恢复命令，读取 `-lic` 之后的命令字符串；
///   2. 用含单引号的 ID 再构造一次；
///   3. 断言会话发现对 InfCode 返回 None（仅 hook 缓存）。
///
/// 预期：
///   - 正断言：命令精确为 `exec "${INFCODE_BIN:-infcode}" --session '<ID>'`；
///   - 负断言：不含 `resume` 子命令、不含 OpenCode 的 OPENCODE_BIN；
///   - 会话发现为 None。
#[test]
fn infcode_resume_uses_session_flag_not_resume_subcommand() {
    let id = "inf-session-42";
    let command = build_agent_resume_command(AgentProfile::InfCode, "/bin/zsh", id, None, None);
    assert_eq!(
        command.args.first().map(String::as_str),
        Some("-lic"),
        "InfCode 恢复命令应走 login interactive shell"
    );
    let line = command.args.get(1).map(String::as_str).unwrap_or_default();
    assert_eq!(
        line,
        format!("exec \"${{INFCODE_BIN:-infcode}}\" --session '{id}'"),
        "InfCode 恢复命令应精确使用 --session 与 INFCODE_BIN 覆盖"
    );
    assert!(
        !line.contains(" resume ") && !line.ends_with(" resume") && !line.contains("resume '"),
        "InfCode 不得使用 resume 子命令（与 OpenCode 不同）"
    );
    assert!(
        !line.contains("OPENCODE_BIN") && !line.contains("opencode"),
        "InfCode 不得误用 OpenCode 二进制"
    );

    let quoted =
        build_agent_resume_command(AgentProfile::InfCode, "/bin/zsh", "bad'id", None, None);
    assert_eq!(
        quoted.args.get(1).map(String::as_str),
        Some("exec \"${INFCODE_BIN:-infcode}\" --session 'bad'\\''id'"),
        "含单引号的 ID 必须被 shell 转义"
    );
    assert_eq!(
        find_agent_session_id_for_profile(AgentProfile::InfCode, Path::new("/tmp")),
        None,
        "InfCode 会话发现未验证，必须返回 None（仅 hook 缓存）"
    );
}
