use super::{build_agent_resume_command, find_agent_session_id_for_profile, shell_quote};
use crate::agent_hooks::AgentHookEnv;
use canvas::AgentProfile;
use std::path::Path;

/// Pi 会话恢复：保存的 hook session ID 必须通过已验证的 `--session-id` 传给 Pi。
///
/// 数据构造（含关键数值的推导过程）：
///   session_id = 完整 UUID `01a14cb5-4b48-71eb-af74-53296bd903ff`（36 字符，调研文档实测样例）
///   workspace_root = `/tmp/norma pi workspace`（含 1 个空格，验证 cwd 原样保存而非拼进 shell 字符串）
///   shell argv 段数 = 2：`-lic` + 一条 `exec pi --session-id '<UUID>'`
///
/// 执行过程：
///   1. 调用 `build_agent_resume_command(Pi, "/bin/zsh", UUID, None, Some(workspace))`。
///   2. 读取 program、args、cwd。
///
/// 预期结果：
///   - 正断言：args 精确为 `["-lic", "exec pi --session-id '<UUID>'"]`，cwd 为保存的 workspace。
///   - 负断言：不得使用 `-r/--resume`（实测会先弹选择器）、`-c`（会串到最近会话）。
///   - 负断言：不得带任何跳过审批类参数（`--approve`、`yolo`、`dangerously`）。
#[test]
fn pi_resume_uses_verified_session_id_flag_and_saved_cwd() {
    let uuid = "01a14cb5-4b48-71eb-af74-53296bd903ff";
    let workspace = Path::new("/tmp/norma pi workspace");

    let cmd = build_agent_resume_command(AgentProfile::Pi, "/bin/zsh", uuid, None, Some(workspace));

    assert_eq!(cmd.program, "/bin/zsh", "Pi 恢复应通过 zsh 启动");
    assert_eq!(
        cmd.args,
        vec!["-lic".to_string(), format!("exec pi --session-id '{uuid}'")],
        "Pi 恢复命令必须是 exec pi --session-id '<完整 UUID>'"
    );
    assert_eq!(
        cmd.cwd.as_deref(),
        Some(workspace),
        "Pi 恢复应在保存的 workspace cwd 中启动"
    );
    let line = &cmd.args[1];
    assert!(
        !line.contains("--resume") && !line.contains(" -r ") && !line.contains(" -c"),
        "Pi 恢复不得使用会弹选择器或串会话的 -r/-c：{line}"
    );
    assert!(
        !line.contains("--approve") && !line.contains("yolo") && !line.contains("dangerously"),
        "Pi 恢复不得带跳过审批类参数：{line}"
    );
}

/// Pi 恢复命令对含单引号的 session ID 做 shell 引用，避免命令注入。
///
/// 数据构造：session_id = `a'b;touch x`（含 1 个单引号与分号），`shell_quote` 结果为 `'a'\''b;touch x'`。
///
/// 执行过程：调用 `build_agent_resume_command(Pi, ..., id, None, None)`。
///
/// 预期结果：
///   - 正断言：命令中的 ID 使用 `shell_quote` 的转义结果。
///   - 负断言：未转义的 `--session-id a'b` 不得出现。
#[test]
fn pi_resume_quotes_hostile_session_id() {
    let id = "a'b;touch x";

    let cmd = build_agent_resume_command(AgentProfile::Pi, "/bin/zsh", id, None, None);

    assert!(
        cmd.args[1].ends_with(&format!("--session-id {}", shell_quote(id))),
        "session ID 必须被 shell_quote：{}",
        cmd.args[1]
    );
    assert!(
        !cmd.args[1].contains("--session-id a'b"),
        "不得出现未转义的 session ID"
    );
}

/// Pi 恢复命令仍注入 Norma hook 环境，扩展才能回传事件。
///
/// 数据构造：node_id=9，profile=Pi，endpoint/token 为测试值；期望 env 含 MULTISOUL_AGENT_PROFILE=pi。
///
/// 执行过程：构造 AgentHookEnv 后调用恢复命令构造器。
///
/// 预期结果：
///   - 正断言：env 含 `MULTISOUL_AGENT_PROFILE=pi` 与 `MULTISOUL_NODE_ID=9`。
///   - 负断言：不得出现 `claude` 作为 profile 值。
#[test]
fn pi_resume_carries_hook_env_with_pi_profile() {
    let env = AgentHookEnv {
        node_id: 9,
        profile: AgentProfile::Pi,
        endpoint: "http://127.0.0.1:1/hook".to_string(),
        token: "t".to_string(),
        workspace_id: "ws".to_string(),
        workspace_route_id: "route".to_string(),
        norma_socket: "/tmp/norma.sock".to_string(),
    };

    let cmd = build_agent_resume_command(AgentProfile::Pi, "/bin/zsh", "id-1", Some(&env), None);

    let has = |k: &str, v: &str| cmd.env.iter().any(|(ek, ev)| ek == k && ev == v);
    assert!(has("MULTISOUL_AGENT_PROFILE", "pi"), "env 应含 profile=pi");
    assert!(has("MULTISOUL_NODE_ID", "9"), "env 应含 node id");
    assert!(
        !has("MULTISOUL_AGENT_PROFILE", "claude"),
        "profile 不得是 claude"
    );
}

/// Pi 会话发现不扫描目录：没有 hook 缓存 ID 时必须返回 None。
///
/// 数据构造：workspace=`/tmp`；Pi 会话目录在 `~/.pi/agent/sessions/`，实现不得读取。
///
/// 执行过程：调用 `find_agent_session_id_for_profile(Pi, workspace)`。
///
/// 预期结果：
///   - 正断言：返回 None（会话 ID 只来自 hook 缓存）。
///   - 负断言：不得返回任何猜测的最近会话 ID。
#[test]
fn pi_session_discovery_never_scans_and_returns_none() {
    let found = find_agent_session_id_for_profile(AgentProfile::Pi, Path::new("/tmp"));

    assert_eq!(found, None, "Pi 无 hook 缓存时不得猜测会话 ID");
}
