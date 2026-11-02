#![cfg(windows)]
use norma::agent_hook::{default_install_paths, resolve_agent_hook_command_path_from};
use norma::orchestration::InstallPaths;
use std::process::Command;

/// 场景：便携包不依赖PATH里的开发版hook；构造中文空格目录并复制真实编译的PE helper。
/// 执行：从同目录App路径调用生产resolver，再运行解析出的helper --help。
/// 预期：精确找到.exe并正常执行，不能返回None或误用App自身。
#[test]
fn portable_hook_is_resolved_and_executed_from_its_real_exe_sibling() {
    let root = tempfile::Builder::new()
        .prefix("Norma 验收 包 ")
        .tempdir()
        .unwrap();
    let executable = root.path().join("norma-os.exe");
    let hook = root.path().join("multisoul-agent-hook.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_multisoul-agent-hook"), &hook).unwrap();
    let resolved = resolve_agent_hook_command_path_from(&executable)
        .expect("原生Windows便携目录必须发现真实.exe helper");
    assert_eq!(resolved, hook, "必须优先使用当前包内helper");
    assert_ne!(resolved, executable, "不能把App当作hook");
    let output = Command::new(resolved).arg("--help").output().unwrap();
    assert!(output.status.success(), "包内helper须可执行：{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("forward"),
        "须执行真实hook CLI帮助"
    );
}

/// 场景：用户安装编排支持；构造独立home和bundle目录并调用生产路径工厂。
/// 执行：核对源文件与托管安装目标的CLI/hook；预期四条路径均保留.exe后缀。
#[test]
fn orchestration_sources_and_managed_targets_keep_windows_executable_names() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let bundle = root.path().join("bundle");
    let paths = InstallPaths::for_home_and_bundle_bin(&home, &bundle);
    assert_eq!(
        paths.bundled_cli,
        bundle.join("norma.exe"),
        "源CLI必须是交付的PE文件"
    );
    assert_eq!(
        paths.bundled_hook,
        bundle.join("multisoul-agent-hook.exe"),
        "源hook必须是交付的PE文件"
    );
    assert_eq!(
        paths.managed_cli_path().file_name().unwrap(),
        "norma.exe",
        "安装后的CLI也须可启动"
    );
    assert_eq!(
        paths.managed_hook_path().file_name().unwrap(),
        "multisoul-agent-hook.exe",
        "安装后的hook也须可启动"
    );
}

/// 场景：原生Windows没有Unix HOME；在同一测试的受控子进程里移除HOME，保留真实USERPROFILE。
/// 执行：调用生产默认Hook目录解析；预期配置进入用户目录，不落在当前便携目录。
#[test]
fn native_hook_home_does_not_require_a_unix_home_variable() {
    const CHILD: &str = "NORMA_HOOK_HOME_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "native_hook_home_does_not_require_a_unix_home_variable",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env_remove("HOME")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "无HOME的原生子进程必须采用用户目录：{output:?}"
        );
        return;
    }
    let home = std::path::PathBuf::from(
        std::env::var_os("USERPROFILE").expect("Windows runner必须有用户profile"),
    );
    let paths = default_install_paths();
    assert_eq!(
        paths.claude_settings,
        home.join(".claude/settings.json"),
        "Claude设置不能误写包目录"
    );
    assert_eq!(
        paths.codex_hooks,
        home.join(".codex/hooks.json"),
        "Codex设置不能误写包目录"
    );
    assert!(
        !paths.claude_settings.starts_with("."),
        "不能把当前目录冒充用户home"
    );
}
