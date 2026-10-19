//! Pi CLI launch script with a persistent "not installed" hint.
//!
//! Without the guard a missing `pi` makes `zsh -lic "exec pi"` print one `command not found`
//! line and exit, so the card flashes and the user never sees how to install it.

/// Install guidance shown when `pi` is not on the login-shell PATH (verified install route in
/// docs/research/2026-10-18-pi-coding-agent.md section 10).
const PI_MISSING_NOTICE: [&str; 4] = [
    "Pi CLI (pi) was not found on PATH.",
    "Install (needs Node 22.19+): npm install --global --prefix \"$HOME/.local\" --ignore-scripts @earendil-works/pi-coding-agent",
    "Official site: https://pi.dev/  |  npm: https://www.npmjs.com/package/@earendil-works/pi-coding-agent",
    "Make sure ~/.local/bin is on PATH in your login shell, then reopen this card.",
];

/// Build the shell command for a Pi card. `pi_args` is appended verbatim after `exec pi`
/// (already shell-quoted by the caller).
pub(crate) fn pi_launch_script(pi_args: &str) -> String {
    let notice = PI_MISSING_NOTICE
        .iter()
        .map(|line| format!("'{}'", line.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ");
    let exec = if pi_args.is_empty() {
        "exec pi".to_string()
    } else {
        format!("exec pi {pi_args}")
    };
    format!(
        "if ! command -v pi >/dev/null 2>&1; then printf '%s\\n' {notice}; exec zsh -l; fi; {exec}"
    )
}

#[cfg(test)]
mod tests {
    use super::pi_launch_script;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    fn run(script: &str, path: &str) -> (String, bool) {
        let output = Command::new("/bin/zsh")
            .args(["-fc", script])
            .env_clear()
            .env("PATH", path)
            .stdin(Stdio::null())
            .output()
            .expect("zsh 应可运行");
        (
            String::from_utf8_lossy(&output.stdout).to_string(),
            output.status.success(),
        )
    }

    /// pi 不在 PATH 时必须打印安装命令与官方链接，而不是只留一行 command not found。
    ///
    /// 数据构造：PATH=`/usr/bin:/bin`（系统目录里没有 pi）；stdin 为空，使兜底 `zsh -l` 立即结束。
    ///
    /// 执行过程：以 `/bin/zsh -fc` 运行 `pi_launch_script("")`。
    ///
    /// 预期结果：
    ///   - 正断言：输出含 npm 安装命令、Node 22.19、https://pi.dev/。
    ///   - 负断言：输出不含 `command not found`（说明没有走到 exec pi）。
    #[test]
    fn missing_pi_prints_install_hint_instead_of_flashing_error() {
        let (stdout, _) = run(&pi_launch_script(""), "/usr/bin:/bin");

        assert!(
            stdout.contains("npm install --global --prefix \"$HOME/.local\" --ignore-scripts @earendil-works/pi-coding-agent"),
            "应打印已验证的安装命令：{stdout}"
        );
        assert!(
            stdout.contains("22.19") && stdout.contains("https://pi.dev/"),
            "应含 Node 要求与官方链接：{stdout}"
        );
        assert!(
            !stdout.contains("command not found"),
            "不应走到 exec pi：{stdout}"
        );
    }

    /// pi 存在时守卫透明：原样 exec pi 并带上恢复参数，不打印安装提示。
    ///
    /// 数据构造：临时目录放一个可执行假 `pi`（echo 参数），PATH=`<临时目录>:/usr/bin:/bin`；
    ///   参数 `--session-id 'abc'`。
    ///
    /// 预期结果：
    ///   - 正断言：输出为 `ARGS:--session-id abc`。
    ///   - 负断言：不含安装提示 `was not found`。
    #[test]
    fn present_pi_is_exec_with_resume_args_and_no_hint() {
        let dir = tempfile::tempdir().expect("临时目录");
        let fake = dir.path().join("pi");
        std::fs::write(&fake, "#!/bin/sh\necho ARGS:$*\n").expect("写假 pi");
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let path = format!("{}:/usr/bin:/bin", dir.path().display());

        let (stdout, ok) = run(&pi_launch_script("--session-id 'abc'"), &path);

        assert!(ok, "假 pi 应成功退出");
        assert_eq!(stdout.trim(), "ARGS:--session-id abc", "应透传恢复参数");
        assert!(!stdout.contains("was not found"), "已安装时不应提示未安装");
    }
}
