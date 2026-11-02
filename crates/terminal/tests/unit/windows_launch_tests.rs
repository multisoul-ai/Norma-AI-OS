use super::*;

/// 场景：Windows原生PTY必须保留CLI参数边界，不能执行POSIX shell或把用户正文作为脚本。
/// 构造：Codex远端地址包含空格、单引号与美元符号；固定flags并附带现有环境与cwd。
/// 执行：通过PTY实际使用的Windows启动准备入口转换。
/// 预期：PowerShell调用运算符与独立引用保留原始字面值，原环境和目录保持不变。
#[test]
fn native_agent_launch_quotes_each_argument_without_a_posix_shell() {
    let command = PtyCommand::new("/bin/zsh")
        .args([
            "-lic",
            "exec codex --remote 'unix://C:/用户 项目/a'\\''b$1.sock' -c disable_paste_burst=true",
        ])
        .cwd("C:/用户 项目")
        .env("MARKER", "kept");
    let actual = prepare(command).unwrap();
    assert_eq!(
        actual.program, "powershell.exe",
        "Windows不能尝试启动/bin/zsh"
    );
    assert_eq!(actual.args, ["-NoLogo", "-NoProfile", "-Command", "& 'codex' '--remote' 'unix://C:/用户 项目/a''b$1.sock' '-c' 'disable_paste_burst=true'"], "每个参数必须成为PowerShell字面值，不能进行变量插值");
    assert_eq!(
        actual.env,
        [("MARKER".into(), "kept".into())],
        "不能丢失已授权hook环境"
    );
    assert_eq!(actual.cwd, Some("C:/用户 项目".into()), "启动位置不能变化");
}

/// 场景：未解析的shell控制符不能被当成普通命令接受。
/// 构造：含有第二条命令的脚本及一个原生PowerShell命令。
/// 执行：调用相同转换入口。
/// 预期：未知shell脚本拒绝；原生程序逐字保留。
#[test]
fn unknown_shell_scripts_are_rejected_and_native_programs_are_preserved() {
    assert!(
        prepare(PtyCommand::new("/bin/zsh").args(["-lic", "exec codex; echo injected"])).is_err(),
        "多语句脚本不得隐式翻译执行"
    );
    let native = PtyCommand::new("powershell.exe").arg("-NoLogo");
    assert_eq!(
        prepare(native.clone()).unwrap(),
        native,
        "原生调用不能被重写"
    );
}
