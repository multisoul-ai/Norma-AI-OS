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

/// 场景：受控Agent的二进制覆写来自本次命令，不应误读App进程的全局环境。
/// 构造：OpenCode/Infcode/Cursor各使用中文空格、单引号和美元符号路径，命令保留hook环境。
/// 执行：走生产prepare入口，逐项比较PowerShell程序字面值和完整环境。
/// 预期：三个覆写均执行同一个字面路径，不能启动默认CLI或丢掉Cursor变量支持。
#[test]
fn agent_binary_overrides_use_the_command_environment() {
    for (variable, default) in [
        ("OPENCODE_BIN", "opencode"),
        ("INFCODE_BIN", "infcode"),
        ("CURSOR_AGENT_BIN", "cursor-agent"),
    ] {
        let path = "C:/用户 项目/agent's $bin.exe";
        let command = PtyCommand::new("/bin/zsh")
            .args([
                "-lic".into(),
                format!("exec \"${{{variable}:-{default}}}\" --resume 'session_7'"),
            ])
            .env(variable, path)
            .env("MULTISOUL_CONTROL_RUN_ID", "run-7");
        let expected_environment = command.env.clone();
        let actual = prepare(command).expect("受控Agent必须接受本次二进制覆写");
        assert_eq!(
            actual.args[3], "& 'C:/用户 项目/agent''s $bin.exe' '--resume' 'session_7'",
            "{variable}: 路径必须保持字面值，不能使用进程环境或默认CLI"
        );
        assert_eq!(
            actual.env, expected_environment,
            "{variable}: 原始hook和二进制环境必须保留"
        );
    }
}

/// 场景：Windows环境键不区分大小写，最后一个空覆写应遵守POSIX的:-默认语义。
/// 构造：同一命令先设错误OPENCODE_BIN，再以小写键设空值；不修改全局进程环境。
/// 执行：通过实际prepare转换原有OpenCode默认表达式。
/// 预期：执行默认opencode，既不残留早先路径，也不把空值作为可执行程序。
#[test]
fn empty_last_binary_override_uses_the_default_case_insensitively() {
    let command = PtyCommand::new("/bin/zsh")
        .args(["-lic", "exec \"${OPENCODE_BIN:-opencode}\""])
        .env("OPENCODE_BIN", "C:/obsolete.exe")
        .env("opencode_bin", "");
    let actual = prepare(command).expect("空覆写应回退默认程序");
    assert_eq!(
        actual.args[3], "& 'opencode'",
        "最后一个空值必须覆盖旧值，并按:-选择默认CLI"
    );
}
