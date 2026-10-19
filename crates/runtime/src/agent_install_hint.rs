//! CLI 未安装时的终端提示：显示官方安装命令与链接，并保留一个登录 shell，而不是一闪而过的
//! `zsh:1: command not found`。只打印，不代为执行安装。

/// 在 `exec_command`（形如 `exec hermes ...`）前加 `command -v` 检查。
/// 找不到命令时打印 `message` 并 `exec "$SHELL" -l` 留住终端；找到时行为与原命令一致。
pub(crate) fn with_install_hint(binary: &str, message: &str, exec_command: &str) -> String {
    format!(
        "command -v {binary} >/dev/null 2>&1 || {{ printf '%s\\n' {}; exec \"${{SHELL:-/bin/zsh}}\" -l; }}; {exec_command}",
        crate::session_snapshot::shell_quote(message)
    )
}

pub(crate) const HERMES_INSTALL_MESSAGE: &str = "Hermes Agent 未安装。官方安装命令（请自行确认后在此终端执行）：\ncurl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash\n官方站点：https://hermes-agent.nousresearch.com";
