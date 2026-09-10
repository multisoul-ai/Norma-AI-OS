use std::{path::PathBuf, process::Command};

/// 手机控制用的真实桌面安装包必须自带审批 Hook，不能依赖开发机的 PATH。
///
/// 数据构造：NORMA_MOBILE_APP_BUNDLE 指向待验收 .app；需要主程序和 Hook 共 2 个可执行文件。
/// 执行过程：检查实际 bundle → 通过正式解析入口定位 Hook → 执行只读 forward --help。
/// 预期结果：Hook 来自本包且支持请求审批；不得回退到其他目录或出现加载错误。
/// 仅验收真实打包产物，不启动桌面或 Agent，不代替完整界面验收。
#[test]
#[ignore = "requires an explicitly selected packaged desktop app"]
fn mobile_desktop_bundle_contains_working_approval_hook() {
    let bundle = PathBuf::from(
        std::env::var_os("NORMA_MOBILE_APP_BUNDLE").expect("必须明确指定待验收 .app"),
    );
    let binary = bundle.join("Contents/MacOS/norma-os");
    let hook = bundle.join("Contents/MacOS/multisoul-agent-hook");
    assert!(binary.is_file(), "桌面主程序必须存在");
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
}
