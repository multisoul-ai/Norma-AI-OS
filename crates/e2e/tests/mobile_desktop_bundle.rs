use std::{path::PathBuf, process::Command};

/// 手机控制用的真实桌面安装包必须自带编排 CLI、审批 Hook 和浏览器授权程序。
///
/// 数据构造：NORMA_MOBILE_APP_BUNDLE 指向待验收 .app；主程序 1 + 辅助程序 3 = 4 个文件。
/// 执行过程：检查实际 bundle → 解析 Hook → 执行 Hook forward --help 和 CLI --version。
/// 预期结果：CLI 可供 Repair 安装，Hook 支持请求审批；不得依赖 PATH 或出现加载错误。
/// 仅验收真实打包产物，不启动桌面或 Agent，不代替完整界面验收。
#[test]
#[ignore = "requires an explicitly selected packaged desktop app"]
fn mobile_desktop_bundle_contains_working_orchestration_binaries() {
    let bundle = PathBuf::from(
        std::env::var_os("NORMA_MOBILE_APP_BUNDLE").expect("必须明确指定待验收 .app"),
    );
    let binary = bundle.join("Contents/MacOS/norma-os");
    let hook = bundle.join("Contents/MacOS/multisoul-agent-hook");
    let cli = bundle.join("Contents/MacOS/norma");
    let browser_auth = bundle.join("Contents/MacOS/norma-browser-auth-host");
    assert!(binary.is_file(), "桌面主程序必须存在");
    assert!(cli.is_file(), "缺少随包 Norma CLI，编排 Repair 会安装失败");
    assert!(browser_auth.is_file(), "浏览器授权辅助程序必须随包提供");
    assert!(
        hook.is_file(),
        "缺少随包审批 Hook，手机创建的 Claude 无法启动"
    );
    assert_eq!(
        norma::agent_hook::resolve_agent_hook_command_path_from(&binary),
        Some(hook.clone()),
        "正式启动入口必须解析到随包 Hook，不能依赖 PATH"
    );
    let result = Command::new(hook)
        .args(["forward", "--help"])
        .output()
        .expect("随包 Hook 应可执行");
    assert!(
        result.status.success(),
        "Hook 必须正常运行，不能存在动态库或签名错误"
    );
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("--request-approval"),
        "随包 Hook 必须支持受控审批协议"
    );
    assert!(result.stderr.is_empty(), "只读帮助不得产生加载或运行错误");
    let cli_result = Command::new(cli)
        .arg("--version")
        .output()
        .expect("随包 Norma CLI 必须可执行");
    assert!(cli_result.status.success(), "Norma CLI 必须正常加载并退出");
    assert!(
        String::from_utf8_lossy(&cli_result.stdout).starts_with("norma "),
        "随包 Norma CLI 必须提供自身版本"
    );
    assert!(cli_result.stderr.is_empty(), "Norma CLI 不得出现加载错误");
}
