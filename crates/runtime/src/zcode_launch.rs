//! ZCode CLI launch script with a persistent "not installed" hint.
//!
//! Without the guard a missing `zcode` makes `zsh -lic "exec zcode"` print one
//! `command not found` line and exit, so the card flashes and the user never sees how to build it.

/// Build guidance shown when `zcode` is not on the login-shell PATH. Only facts verified in
/// docs/research/2026-10-18-zcode.md: no prebuilt CLI package, build from source.
const ZCODE_MISSING_NOTICE: [&str; 3] = [
    "ZCode CLI (zcode) was not found on PATH.",
    "There is no official prebuilt CLI package yet; build it from source with Node 24.14.0 and pnpm 10.33.2.",
    "Repository: https://github.com/zai-org/ZCode  (put the built zcode on PATH, then reopen this card.)",
];

/// Build the shell command for a ZCode card. `zcode_args` is appended verbatim after
/// `exec zcode` (already shell-quoted by the caller).
pub(crate) fn zcode_launch_script(zcode_args: &str) -> String {
    let notice = ZCODE_MISSING_NOTICE
        .iter()
        .map(|line| format!("'{}'", line.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ");
    let exec = if zcode_args.is_empty() {
        "exec zcode".to_string()
    } else {
        format!("exec zcode {zcode_args}")
    };
    format!(
        "if ! command -v zcode >/dev/null 2>&1; then printf '%s\\n' {notice}; exec zsh -l; fi; {exec}"
    )
}

#[cfg(test)]
mod tests {
    use super::zcode_launch_script;
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

    /// zcode 不在 PATH 时必须打印构建说明并保留 shell，而不是只留一行 command not found。
    ///
    /// 数据构造：PATH=`/usr/bin:/bin`（无 zcode）；stdin 为空，使兜底 `zsh -l` 立即正常结束。
    ///
    /// 执行过程：以 `/bin/zsh -fc` 运行 `zcode_launch_script("")`。
    ///
    /// 预期结果：
    ///   - 正断言：输出含 Node 24.14.0、pnpm 10.33.2、仓库链接；兜底 shell 正常退出。
    ///   - 负断言：输出不含 `command not found`（没有走到 exec zcode）。
    #[test]
    fn missing_zcode_prints_build_hint_and_keeps_shell() {
        let (stdout, ok) = run(&zcode_launch_script(""), "/usr/bin:/bin");

        assert!(
            stdout.contains("24.14.0") && stdout.contains("pnpm 10.33.2"),
            "应含已验证的 Node/pnpm 版本：{stdout}"
        );
        assert!(
            stdout.contains("https://github.com/zai-org/ZCode"),
            "应含官方仓库链接：{stdout}"
        );
        assert!(ok, "兜底 zsh 应可用并正常退出：{stdout}");
        assert!(
            !stdout.contains("command not found"),
            "不应走到 exec zcode：{stdout}"
        );
    }

    /// zcode 存在时守卫透明：原样 exec 并带上恢复参数，不打印提示。
    ///
    /// 数据构造：临时目录放可执行假 `zcode`（echo 参数），PATH 含该目录；参数 `--resume 'abc'`。
    ///
    /// 预期结果：
    ///   - 正断言：输出为 `ARGS:--resume abc`。
    ///   - 负断言：不含 `was not found`。
    #[test]
    fn present_zcode_is_exec_with_args_and_no_hint() {
        let dir = tempfile::tempdir().expect("临时目录");
        let fake = dir.path().join("zcode");
        std::fs::write(&fake, "#!/bin/sh\necho ARGS:$*\n").expect("写假 zcode");
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let path = format!("{}:/usr/bin:/bin", dir.path().display());

        let (stdout, ok) = run(&zcode_launch_script("--resume 'abc'"), &path);

        assert!(ok, "假 zcode 应成功退出");
        assert_eq!(stdout.trim(), "ARGS:--resume abc", "应透传恢复参数");
        assert!(!stdout.contains("was not found"), "已安装时不应提示未安装");
    }
}
