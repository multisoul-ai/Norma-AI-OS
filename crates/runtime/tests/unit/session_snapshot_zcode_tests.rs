use crate::session_snapshot::{build_agent_resume_command, find_agent_session_id_for_profile};
use canvas::AgentProfile;
use std::path::Path;

const VALID_ID: &str = "sess_33c0d69e-1a2b-4c3d-8e4f-0123456789ab";

fn zcode_shell_line(session_id: &str) -> String {
    let cmd = build_agent_resume_command(AgentProfile::ZCode, "/bin/zsh", session_id, None, None);
    cmd.args.last().cloned().unwrap_or_default()
}

/// 场景：ZCode 卡片用 hook 缓存的合法会话 ID 恢复。
///
/// 数据构造：ID = `sess_` + 36 位 UUID 文本（8-4-4-4-12 个十六进制字符加 4 个连字符），
/// 即调研文档 §12 规定的 `^sess_[0-9a-fA-F-]{36}$` 形态。
///
/// 执行过程：调用 `build_agent_resume_command(ZCode, ...)`，取 `-lic` 后的 shell 命令行。
///
/// 预期结果：命令为 `exec zcode --resume '<id>'`；不得出现 `-c`（按目录取最近会产生串会话）
/// 与任何跳过审批的 `--mode`。
#[test]
fn zcode_resume_uses_explicit_session_id_without_continue_or_mode() {
    let line = zcode_shell_line(VALID_ID);
    assert_eq!(
        line,
        format!("exec zcode --resume '{VALID_ID}'"),
        "合法 sess_ ID 应拼成 --resume 命令"
    );
    assert!(!line.contains(" -c"), "不得使用按目录取最近的 -c: {line}");
    assert!(
        !line.contains("--mode"),
        "默认恢复命令不得带 --mode: {line}"
    );
}

/// 场景：缓存里的会话 ID 不合法（含 shell 元字符、缺前缀、长度错误、空串）。
///
/// 数据构造：四个坏 ID，分别是注入串 `sess_x'; touch /tmp/pwn #`、无前缀 UUID、
/// 比合法 ID 少 1 位的串、空串。
///
/// 执行过程：逐个构造恢复命令。
///
/// 预期结果：全部退化为普通启动 `exec zcode`，且不含 `--resume` 与注入片段。
#[test]
fn zcode_resume_rejects_invalid_session_id_and_falls_back_to_plain_launch() {
    let short = &VALID_ID[..VALID_ID.len() - 1];
    let bad = [
        "sess_x'; touch /tmp/pwn #",
        "33c0d69e-1a2b-4c3d-8e4f-0123456789ab",
        short,
        "",
    ];
    for id in bad {
        let line = zcode_shell_line(id);
        assert_eq!(line, "exec zcode", "非法 ID {id:?} 应退化为普通启动");
        assert!(!line.contains("--resume"), "非法 ID 不得进入命令: {line}");
        assert!(!line.contains("pwn"), "注入片段不得进入命令: {line}");
    }
}

/// 场景：ZCode 会话发现。调研确认会话只能经 hook 获得，库是 SQLite，禁止扫描。
///
/// 执行过程：对任意工作区调用 `find_agent_session_id_for_profile(ZCode, ...)`。
///
/// 预期结果：返回 None（即使 HOME 下存在 `.zcode` 目录也不扫描）。
#[test]
fn zcode_session_discovery_never_scans_and_returns_none() {
    let home = tempfile::tempdir().expect("应创建诱饵目录");
    std::fs::create_dir_all(home.path().join(".zcode/cli/rollout")).expect("应创建 rollout");
    std::fs::write(
        home.path()
            .join(format!(".zcode/cli/rollout/model-io-{VALID_ID}.jsonl")),
        b"{}",
    )
    .expect("应写入 rollout 诱饵");
    // 把 HOME 指向诱饵目录，使真正的扫描实现必然能看到 rollout 文件；调用后立即恢复。
    let previous = std::env::var_os("HOME");
    std::env::set_var("HOME", home.path());
    let found = find_agent_session_id_for_profile(AgentProfile::ZCode, Path::new("/tmp/zc-work"));
    match previous {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    assert_eq!(
        found, None,
        "HOME 下存在 rollout 诱饵时 ZCode 仍不得扫描发现会话"
    );
}
