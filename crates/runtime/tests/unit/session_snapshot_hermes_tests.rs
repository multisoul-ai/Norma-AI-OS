//! Hermes Agent 恢复命令与启动命令的单元测试。

use std::path::Path;

use crate::nodes::agent::agent_command;
use crate::session_snapshot::{build_agent_resume_command, find_agent_session_id_for_profile};
use canvas::AgentProfile;

/// HM-1: 有 hook 缓存会话 ID 时，Hermes 用已实测的 `--resume <id>` 恢复。
///
/// 数据构造：
///   session_id = "20261018_101500_abcdef"（实测格式 YYYYMMDD_HHMMSS_hex）
///   shell = "/bin/zsh"，不注入 hook 环境，cwd 为 /tmp/hm-ws
///
/// 执行过程：调用 build_agent_resume_command(Hermes, ...)，读取 args 与 cwd。
///
/// 预期结果：
///   - 正断言：args 为 ["-lic", "exec hermes --resume '20261018_101500_abcdef'"]。
///   - 负断言：不得出现 -c（工作区作用域不可靠）、--yolo、--accept-hooks。
#[test]
fn hermes_resume_uses_verified_resume_flag_with_quoted_id() {
    let cmd = build_agent_resume_command(
        AgentProfile::Hermes,
        "/bin/zsh",
        "20261018_101500_abcdef",
        None,
        Some(Path::new("/tmp/hm-ws")),
    );
    assert_eq!(
        cmd.args,
        vec![
            "-lic".to_string(),
            "exec hermes --resume '20261018_101500_abcdef'".to_string()
        ],
        "Hermes 恢复命令应使用实测的 --resume <id>"
    );
    let joined = cmd.args.join(" ");
    assert!(
        !joined.contains(" -c"),
        "不得使用 hermes -c：续接最近会话的工作区作用域不可靠"
    );
    assert!(!joined.contains("--yolo"), "恢复命令不得跳过审批");
    assert!(
        !joined.contains("--accept-hooks"),
        "恢复命令不得自动同意 hooks"
    );
    assert_eq!(
        cmd.cwd.as_deref(),
        Some(Path::new("/tmp/hm-ws")),
        "恢复应复用保存的 cwd"
    );
}

/// HM-2: 会话 ID 含单引号时必须经 shell_quote 转义，防止命令注入。
///
/// 数据构造：session_id = "a'b;rm"，转义后应为 'a'\''b;rm'。
/// 预期：命令字符串以 `exec hermes --resume 'a'\''b;rm'` 结尾。
#[test]
fn hermes_resume_quotes_hostile_session_id() {
    let cmd = build_agent_resume_command(AgentProfile::Hermes, "/bin/zsh", "a'b;rm", None, None);
    assert_eq!(
        cmd.args.get(1).map(String::as_str),
        Some("exec hermes --resume 'a'\\''b;rm'"),
        "含单引号的会话 ID 必须被安全转义"
    );
}

/// HM-3: 默认启动命令不带任何跳过审批/自动同意参数，会话发现返回 None。
///
/// 数据构造：agent_command(Hermes, 无 hook env)；对任意工作区做会话发现。
/// 预期：args == ["-lic","exec hermes"]；发现结果 None（不扫描 state.db）。
#[test]
fn hermes_launch_is_plain_and_discovery_is_none() {
    let cmd = agent_command(AgentProfile::Hermes, None);
    assert_eq!(
        cmd.args,
        vec!["-lic".to_string(), "exec hermes".to_string()],
        "Hermes 默认启动命令只能是 exec hermes"
    );
    assert_eq!(
        find_agent_session_id_for_profile(AgentProfile::Hermes, Path::new("/tmp/hm-ws")),
        None,
        "Hermes 会话 ID 只来自 hook 缓存，不得扫描目录"
    );
}
