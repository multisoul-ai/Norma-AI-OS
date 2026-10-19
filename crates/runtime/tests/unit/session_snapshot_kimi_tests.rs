//! Kimi Code CLI 恢复命令与会话发现测试。

use crate::session_snapshot::{build_agent_resume_command, find_agent_session_id_for_profile};
use canvas::AgentProfile;
use std::path::Path;

/// Kimi 恢复命令：必须使用实测通过的 `--session <ID>`，而不是按目录取最近会话的 `--continue`。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = "session_e9029623-7275-4b57-8758-0e8cb054234f"（hook 载荷里真实出现的 ID 格式）
///   带单引号的 ID = "bad'id"（验证 shell 引用：' 被转义为 '\''，共 1 处）
///
/// 执行过程（逐步说明系统如何处理）：
///   1. 用真实格式 ID 构造 Kimi 恢复命令，读取 `-lic` 之后的命令字符串；
///   2. 用含单引号的 ID 再构造一次，检查引用结果；
///   3. 同时断言会话发现对 Kimi 仍返回 None（不扫描目录）。
///
/// 预期结果：
///   - 正断言：命令精确为 `exec kimi --session '<ID>'`，首个 shell 参数为 `-lic`；
///   - 负断言：不含 `--continue`、`--resume`、`--yolo`、`--auto` 等未采用或跳过审批的参数；
///   - 单引号 ID 被安全转义；会话发现为 None。
#[test]
fn kimi_resume_command_uses_session_flag_and_quotes_id() {
    let id = "session_e9029623-7275-4b57-8758-0e8cb054234f";
    let command = build_agent_resume_command(AgentProfile::Kimi, "/bin/zsh", id, None, None);
    assert_eq!(
        command.args.first().map(String::as_str),
        Some("-lic"),
        "Kimi 恢复命令应走 login interactive shell"
    );
    let line = command.args.get(1).map(String::as_str).unwrap_or_default();
    assert_eq!(
        line,
        format!("exec kimi --session '{id}'"),
        "Kimi 恢复命令应精确使用 --session 加引用后的 ID"
    );
    for forbidden in ["--continue", "--resume", "--yolo", "--auto"] {
        assert!(
            !line.contains(forbidden),
            "Kimi 恢复命令不得包含 {forbidden}"
        );
    }

    let quoted = build_agent_resume_command(AgentProfile::Kimi, "/bin/zsh", "bad'id", None, None);
    assert_eq!(
        quoted.args.get(1).map(String::as_str),
        Some("exec kimi --session 'bad'\\''id'"),
        "含单引号的 ID 必须被 shell 转义"
    );
    assert_eq!(
        find_agent_session_id_for_profile(AgentProfile::Kimi, Path::new("/tmp")),
        None,
        "Kimi 没有无需扫描目录的已验证会话发现方式，必须返回 None"
    );
}
